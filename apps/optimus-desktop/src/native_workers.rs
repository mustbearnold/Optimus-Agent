//! Bounded native IPC workers.
//!
//! The tao event loop owns all live window/dialog work. Everything else is
//! queued here so slow runtime, filesystem, or provider calls cannot block UI
//! dispatch or create an unbounded number of operating-system threads.

use std::path::PathBuf;
use std::sync::{
    mpsc::{self, Receiver, SyncSender, TrySendError},
    Arc, Mutex,
};
use std::thread;

use serde_json::json;
use tao::event_loop::EventLoopProxy;

use crate::ipc::{
    auth_status_json, chat_turn, doctor_json, handle_ipc, sessions_json, stream_event_to_json,
    IpcReply,
};
use crate::UserEvent;

const IPC_QUEUE_CAPACITY: usize = 64;
const CHAT_QUEUE_CAPACITY: usize = 8;
const CHAT_WORKERS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EnqueueError {
    Full,
    Disconnected,
}

impl std::fmt::Display for EnqueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => f.write_str("queue full"),
            Self::Disconnected => f.write_str("worker unavailable"),
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
        id: u64,
        params: serde_json::Value,
    },
}

pub(super) struct NativeWorkers {
    ipc_tx: SyncSender<IpcWork>,
    chat_tx: SyncSender<ChatWork>,
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
        for index in 0..CHAT_WORKERS {
            let worker_rx = Arc::clone(&chat_rx);
            let worker_home = home.clone();
            let worker_proxy = proxy.clone();
            thread::Builder::new()
                .name(format!("optimus-chat-worker-{index}"))
                .spawn(move || run_chat_worker(worker_rx, worker_home, worker_proxy))?;
        }

        Ok(Self { ipc_tx, chat_tx })
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
        if matches!(method.as_str(), "chat" | "chat_offline") {
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
        map_enqueue(self.chat_tx.try_send(ChatWork::Stream { id, params }))
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
            ChatWork::Stream { id, params } => {
                let mut on_event = |event| {
                    let payload = stream_event_to_json(&event);
                    let _ = proxy.send_event(UserEvent::Stream { id, payload });
                };
                match chat_turn(&home, params, Some(&mut on_event)) {
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

    use super::{map_enqueue, EnqueueError, CHAT_QUEUE_CAPACITY, CHAT_WORKERS, IPC_QUEUE_CAPACITY};

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
}
