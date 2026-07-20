use js_sys::{Function, Promise, Reflect};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Serialize;
use serde_json::{json, Value};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

#[derive(Clone)]
struct Message {
    role: &'static str,
    text: String,
}

async fn optimus_invoke(method: &str, params: Value) -> Result<Value, String> {
    let window = web_sys::window().ok_or_else(|| "window unavailable".to_string())?;
    let optimus = Reflect::get(window.as_ref(), &JsValue::from_str("optimus"))
        .map_err(|error| format!("window.optimus missing: {error:?}"))?;
    let invoke = Reflect::get(&optimus, &JsValue::from_str("invoke"))
        .map_err(|error| format!("window.optimus.invoke missing: {error:?}"))?
        .dyn_into::<Function>()
        .map_err(|_| "window.optimus.invoke is not a function".to_string())?;
    let params = params
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|error| format!("serialize IPC params: {error}"))?;
    let promise = invoke
        .call2(&optimus, &JsValue::from_str(method), &params)
        .map_err(|error| format!("invoke {method}: {error:?}"))?
        .dyn_into::<Promise>()
        .map_err(|_| format!("invoke {method} did not return a Promise"))?;
    let result = JsFuture::from(promise)
        .await
        .map_err(|error| format!("IPC {method}: {error:?}"))?;
    serde_wasm_bindgen::from_value(result).map_err(|error| format!("decode IPC {method}: {error}"))
}

#[component]
fn App() -> impl IntoView {
    let health = RwSignal::new("checking IPC…".to_string());
    let input = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let session = RwSignal::new(None::<String>);
    let messages = RwSignal::new(vec![Message {
        role: "assistant",
        text: "Leptos CSR mounted. Send an offline turn through the existing Optimus IPC.".into(),
    }]);
    let message_list = NodeRef::<leptos::html::Div>::new();

    Effect::new(move |_| {
        messages.track();
        if let Some(element) = message_list.get() {
            element.set_scroll_top(element.scroll_height());
        }
    });

    spawn_local(async move {
        match optimus_invoke("doctor", json!({})).await {
            Ok(value) => {
                let schema = value
                    .get("core_schema_tokens")
                    .and_then(Value::as_u64)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "ready".into());
                health.set(format!("IPC online · schema {schema}"));
            }
            Err(error) => health.set(format!("IPC unavailable · {error}")),
        }
    });

    let send = move |_| {
        let text = input.get().trim().to_string();
        if text.is_empty() || busy.get() {
            return;
        }
        input.set(String::new());
        busy.set(true);
        messages.update(|items| {
            items.push(Message {
                role: "user",
                text: text.clone(),
            });
        });

        spawn_local(async move {
            let mut params = json!({"message": text, "provider": "offline"});
            if let Some(id) = session.get_untracked() {
                params["session"] = Value::String(id);
            }
            match optimus_invoke("chat", params).await {
                Ok(value) => {
                    if let Some(id) = value.get("session_id").and_then(Value::as_str) {
                        session.set(Some(id.to_string()));
                    }
                    let text = value
                        .get("assistant_text")
                        .and_then(Value::as_str)
                        .unwrap_or("IPC returned no assistant_text")
                        .to_string();
                    messages.update(|items| {
                        items.push(Message {
                            role: "assistant",
                            text,
                        })
                    });
                    health.set("IPC online · offline chat verified".into());
                }
                Err(error) => {
                    messages.update(|items| {
                        items.push(Message {
                            role: "error",
                            text: error,
                        });
                    });
                }
            }
            busy.set(false);
        });
    };

    view! {
        <div class="shell" data-testid="leptos-shell">
            <header class="titlebar">
                <div class="title">"Leptos frontend spike"</div>
                <div class="title-actions">
                    <span class="badge">"CSR · WASM"</span>
                    <button type="button">"Files"</button>
                    <button type="button">"Term"</button>
                    <button type="button">"Logs"</button>
                </div>
            </header>
            <div class="body">
                <aside class="rail">
                    <button class="nav active">"+  New session"</button>
                    <button class="nav">"◇  Capabilities"</button>
                    <button class="nav">"●  Messaging"</button>
                    <button class="nav">"▣  Artifacts"</button>
                    <input class="search" placeholder="Search sessions…" />
                    <div class="section-label">"Pinned"</div>
                    <div class="empty">"Drag sessions or projects here"</div>
                    <div class="project-block">
                        <div class="section-label">"Projects"</div>
                        <button class="thread active">"Leptos spike"</button>
                    </div>
                    <button class="settings">"⚙ Settings"</button>
                </aside>
                <main class="main">
                    <div class="messages" aria-live="polite" node_ref=message_list>
                        {move || messages.get().into_iter().enumerate().map(|(index, message)| {
                            view! {
                                <article class=format!("message {}", message.role) data-index=index>
                                    <span class="role">{message.role}</span>
                                    <div class="bubble">{message.text}</div>
                                </article>
                            }
                        }).collect_view()}
                    </div>
                    <form class="composer" on:submit=move |event| {
                        event.prevent_default();
                        send(());
                    }>
                        <textarea
                            aria-label="Message"
                            placeholder="Message Optimus…"
                            prop:value=move || input.get()
                            on:input=move |event| input.set(event_target_value(&event))
                            disabled=move || busy.get()
                        ></textarea>
                        <div class="composer-row">
                            <span>"PROV offline · MODEL offline-echo · ACCESS full"</span>
                            <button class="send" type="submit" disabled=move || busy.get()>
                                {move || if busy.get() { "…" } else { "↑" }}
                            </button>
                        </div>
                    </form>
                </main>
            </div>
            <footer class="statusbar">
                <span class="dot"></span>
                <span data-testid="ipc-status">{move || health.get()}</span>
                <span class="status-right">"Leptos 0.8.20 · Trunk 0.21.14"</span>
            </footer>
        </div>
    }
}

fn main() {
    leptos::mount::mount_to_body(App);
}
