//! Bounded native IPC workers.
//!
//! The tao event loop owns all live window/dialog work. Everything else is
//! queued here so slow runtime, filesystem, or provider calls cannot block UI
//! dispatch or create an unbounded number of operating-system threads.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    mpsc::{self, Receiver, SyncSender, TrySendError},
    Arc, Mutex,
};
use std::thread;

use serde_json::json;
use tao::event_loop::EventLoopProxy;
use uuid::Uuid;

use optimus_kernel::CancellationToken;

use crate::UserEvent;
use optimus_host::{
    auth_status_json, doctor_json, handle_ipc, sessions_json, stream_delivery_control,
    stream_event_to_json, IpcReply,
};

const IPC_QUEUE_CAPACITY: usize = 64;
const CHAT_QUEUE_CAPACITY: usize = 8;
const CHAT_WORKERS: usize = 2;
const ACTIVE_STREAM_CAPACITY: usize = CHAT_QUEUE_CAPACITY + CHAT_WORKERS;

#[derive(Debug, Clone)]
struct ActiveStream {
    id: u64,
    generation: Uuid,
    cancellation: CancellationToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamRegistrationError {
    Duplicate,
    Full,
}

#[derive(Clone, Default)]
struct ActiveStreams(Arc<Mutex<HashMap<u64, ActiveStream>>>);

impl ActiveStreams {
    fn register(&self, id: u64) -> Result<ActiveStream, StreamRegistrationError> {
        let mut streams = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if streams.contains_key(&id) {
            return Err(StreamRegistrationError::Duplicate);
        }
        if streams.len() >= ACTIVE_STREAM_CAPACITY {
            return Err(StreamRegistrationError::Full);
        }
        let owner = ActiveStream {
            id,
            generation: Uuid::new_v4(),
            cancellation: CancellationToken::new(),
        };
        streams.insert(id, owner.clone());
        Ok(owner)
    }

    fn cancel(&self, id: u64) -> bool {
        let cancellation = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&id)
            .map(|owner| owner.cancellation.clone());
        if let Some(cancellation) = cancellation {
            cancellation.cancel();
            true
        } else {
            false
        }
    }

    fn unregister(&self, owner: &ActiveStream) -> bool {
        let mut streams = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if streams
            .get(&owner.id)
            .is_some_and(|current| current.generation == owner.generation)
        {
            streams.remove(&owner.id);
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EnqueueError {
    Full,
    Disconnected,
    Duplicate,
}

impl std::fmt::Display for EnqueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => f.write_str("queue full"),
            Self::Disconnected => f.write_str("worker unavailable"),
            Self::Duplicate => f.write_str("duplicate stream id"),
        }
    }
}

enum IpcWork {
    Ready,
    Request {
        id: u64,
        method: String,
        params: serde_json::Value,
    },
}

enum ChatWork {
    Request {
        id: u64,
        method: String,
        params: serde_json::Value,
    },
    Stream {
        owner: ActiveStream,
        params: serde_json::Value,
    },
}

pub(super) struct NativeWorkers {
    ipc_tx: SyncSender<IpcWork>,
    chat_tx: SyncSender<ChatWork>,
    active_streams: ActiveStreams,
}

impl NativeWorkers {
    pub(super) fn start(home: PathBuf, proxy: EventLoopProxy<UserEvent>) -> std::io::Result<Self> {
        let (ipc_tx, ipc_rx) = mpsc::sync_channel(IPC_QUEUE_CAPACITY);
        let ipc_home = home.clone();
        let ipc_proxy = proxy.clone();
        thread::Builder::new()
            .name("optimus-ipc-worker".into())
            .spawn(move || run_ipc_worker(ipc_rx, ipc_home, ipc_proxy))?;

        let (chat_tx, chat_rx) = mpsc::sync_channel(CHAT_QUEUE_CAPACITY);
        let chat_rx = Arc::new(Mutex::new(chat_rx));
        let active_streams = ActiveStreams::default();
        for index in 0..CHAT_WORKERS {
            let worker_rx = Arc::clone(&chat_rx);
            let worker_home = home.clone();
            let worker_proxy = proxy.clone();
            let worker_streams = active_streams.clone();
            thread::Builder::new()
                .name(format!("optimus-chat-worker-{index}"))
                .spawn(move || {
                    run_chat_worker(worker_rx, worker_home, worker_proxy, worker_streams)
                })?;
        }
        Ok(Self {
            ipc_tx,
            chat_tx,
            active_streams,
        })
    }

    pub(super) fn enqueue_ready(&self) -> Result<(), EnqueueError> {
        map_enqueue(self.ipc_tx.try_send(IpcWork::Ready))
    }

    pub(super) fn enqueue_request(
        &self,
        id: u64,
        method: String,
        params: serde_json::Value,
    ) -> Result<(), EnqueueError> {
        // `chat_approval_resolve` is a streaming turn like `chat` (ADR-0046):
        // it settles the effect and then runs the continuation, which can take
        // minutes. It must not occupy the single IPC worker, or every other
        // request — sessions, doctor, ready — queues behind the continuation.
        if matches!(
            method.as_str(),
            "chat" | "chat_offline" | "chat_approval_resolve"
        ) {
            map_enqueue(
                self.chat_tx
                    .try_send(ChatWork::Request { id, method, params }),
            )
        } else {
            map_enqueue(
                self.ipc_tx
                    .try_send(IpcWork::Request { id, method, params }),
            )
        }
    }

    pub(super) fn enqueue_stream(
        &self,
        id: u64,
        params: serde_json::Value,
    ) -> Result<(), EnqueueError> {
        let owner = self
            .active_streams
            .register(id)
            .map_err(|error| match error {
                StreamRegistrationError::Duplicate => EnqueueError::Duplicate,
                StreamRegistrationError::Full => EnqueueError::Full,
            })?;
        let result = map_enqueue(self.chat_tx.try_send(ChatWork::Stream {
            owner: owner.clone(),
            params,
        }));
        if result.is_err() {
            self.active_streams.unregister(&owner);
        }
        result
    }

    pub(super) fn cancel_stream(&self, id: u64) -> bool {
        self.active_streams.cancel(id)
    }
}

fn map_enqueue<T>(result: Result<(), TrySendError<T>>) -> Result<(), EnqueueError> {
    match result {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(_)) => Err(EnqueueError::Full),
        Err(TrySendError::Disconnected(_)) => Err(EnqueueError::Disconnected),
    }
}

fn run_ipc_worker(rx: Receiver<IpcWork>, home: PathBuf, proxy: EventLoopProxy<UserEvent>) {
    while let Ok(work) = rx.recv() {
        match work {
            IpcWork::Ready => {
                let status = doctor_json(&home);
                let sessions = sessions_json(&home);
                let auth = auth_status_json(&home);
                eprintln!(
                    "[optimus-desktop] push ready codex_present={}",
                    status
                        .get("codex_present")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false)
                );
                let _ = proxy.send_event(UserEvent::IpcDone(IpcReply {
                    id: 0,
                    ok: true,
                    result: Some(json!({
                        "event": "ready",
                        "doctor": status,
                        "sessions": sessions,
                        "auth": auth,
                        "chrome": "custom-titlebar",
                    })),
                    error: None,
                }));
            }
            IpcWork::Request { id, method, params } => {
                send_reply(&proxy, id, handle_ipc(&home, &method, params));
            }
        }
    }
}

fn run_chat_worker(
    rx: Arc<Mutex<Receiver<ChatWork>>>,
    home: PathBuf,
    proxy: EventLoopProxy<UserEvent>,
    active_streams: ActiveStreams,
) {
    loop {
        let work = {
            let receiver = rx.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            receiver.recv()
        };
        let Ok(work) = work else {
            break;
        };
        match work {
            ChatWork::Request { id, method, params } => {
                send_reply(&proxy, id, handle_ipc(&home, &method, params));
            }
            ChatWork::Stream { owner, params } => {
                let id = owner.id;
                let mut on_event = |event| {
                    let payload = stream_event_to_json(&event);
                    stream_delivery_control(
                        proxy.send_event(UserEvent::Stream { id, payload }).is_ok(),
                    )
                };
                let result = optimus_host::chat_turn_cancellable(
                    &home,
                    params,
                    Some(&mut on_event),
                    &owner.cancellation,
                );
                active_streams.unregister(&owner);
                match result {
                    Ok(result) => {
                        let _ = proxy.send_event(UserEvent::Stream {
                            id,
                            payload: json!({"type":"done","result": result}),
                        });
                    }
                    Err(error) => {
                        let _ = proxy.send_event(UserEvent::Stream {
                            id,
                            payload: json!({"type":"error","error": error}),
                        });
                    }
                }
            }
        }
    }
}

fn send_reply(
    proxy: &EventLoopProxy<UserEvent>,
    id: u64,
    result: Result<serde_json::Value, String>,
) {
    let reply = match result {
        Ok(result) => IpcReply {
            id,
            ok: true,
            result: Some(result),
            error: None,
        },
        Err(error) => IpcReply {
            id,
            ok: false,
            result: None,
            error: Some(error),
        },
    };
    let _ = proxy.send_event(UserEvent::IpcDone(reply));
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use serde_json::json;

    use super::{
        map_enqueue, ActiveStreams, ChatWork, EnqueueError, IpcWork, NativeWorkers,
        StreamRegistrationError, ACTIVE_STREAM_CAPACITY, CHAT_QUEUE_CAPACITY, CHAT_WORKERS,
        IPC_QUEUE_CAPACITY,
    };

    #[test]
    fn approval_resolution_routes_to_chat_workers_not_the_single_ipc_worker() {
        let (ipc_tx, ipc_rx) = mpsc::sync_channel(8);
        let (chat_tx, chat_rx) = mpsc::sync_channel(8);
        let workers = NativeWorkers {
            ipc_tx,
            chat_tx,
            active_streams: ActiveStreams::default(),
        };

        workers
            .enqueue_request(1, "doctor".into(), json!({}))
            .expect("ipc admission");
        workers
            .enqueue_request(2, "chat_approval_resolve".into(), json!({}))
            .expect("chat admission");
        workers
            .enqueue_request(3, "chat".into(), json!({}))
            .expect("chat admission");

        // `chat_approval_resolve` runs the resumed continuation (ADR-0046), so
        // it must ride the chat queue like `chat` — the single IPC worker must
        // stay free for sessions/doctor/ready while a continuation streams.
        assert!(matches!(
            ipc_rx.try_recv(),
            Ok(IpcWork::Request { id: 1, .. })
        ));
        assert!(matches!(
            chat_rx.try_recv(),
            Ok(ChatWork::Request { id: 2, .. })
        ));
        assert!(matches!(
            chat_rx.try_recv(),
            Ok(ChatWork::Request { id: 3, .. })
        ));
        assert!(ipc_rx.try_recv().is_err());
    }

    #[test]
    fn worker_and_queue_limits_are_bounded() {
        assert_eq!(IPC_QUEUE_CAPACITY, 64);
        assert_eq!(CHAT_QUEUE_CAPACITY, 8);
        assert_eq!(CHAT_WORKERS, 2);
    }

    #[test]
    fn full_queue_fails_without_blocking() {
        let (tx, _rx) = mpsc::sync_channel(1);
        tx.try_send(1).expect("first item");
        assert_eq!(map_enqueue(tx.try_send(2)), Err(EnqueueError::Full));
    }

    #[test]
    fn active_stream_registration_is_exact_bounded_and_idempotently_cancellable() {
        let streams = ActiveStreams::default();
        let owner = streams.register(7).expect("register");
        assert!(matches!(
            streams.register(7),
            Err(StreamRegistrationError::Duplicate)
        ));
        assert!(!owner.cancellation.is_cancelled());
        assert!(streams.cancel(7));
        assert!(streams.cancel(7));
        assert!(owner.cancellation.is_cancelled());
        assert!(!streams.cancel(8));
        assert!(streams.unregister(&owner));
        assert!(!streams.cancel(7));

        for id in 0..ACTIVE_STREAM_CAPACITY as u64 {
            streams.register(100 + id).expect("bounded registration");
        }
        assert!(matches!(
            streams.register(999),
            Err(StreamRegistrationError::Full)
        ));
    }

    #[test]
    fn stale_owner_cannot_unregister_reused_stream_id() {
        let streams = ActiveStreams::default();
        let first = streams.register(11).expect("first");
        assert!(streams.unregister(&first));
        let second = streams.register(11).expect("second");

        assert!(!streams.unregister(&first));
        assert!(streams.cancel(11));
        assert!(second.cancellation.is_cancelled());
    }

    #[test]
    fn failed_queue_admission_rolls_back_only_rejected_registration() {
        let (ipc_tx, _ipc_rx) = mpsc::sync_channel(1);
        let (chat_tx, _chat_rx) = mpsc::sync_channel(1);
        let active_streams = ActiveStreams::default();
        let workers = NativeWorkers {
            ipc_tx,
            chat_tx,
            active_streams: active_streams.clone(),
        };

        workers.enqueue_stream(1, json!({})).expect("admitted");
        assert_eq!(
            workers.enqueue_stream(2, json!({})),
            Err(EnqueueError::Full)
        );
        assert!(active_streams.cancel(1));
        assert!(!active_streams.cancel(2));
    }
}
