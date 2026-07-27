//! Codex device-code login.
//!
//! Split out of `codex_oauth.rs` so that module can stay under its size baseline,
//! and because the login handshake is a self-contained flow: request a user code,
//! prompt, poll, exchange, store.

use std::thread;
use std::time::{Duration, SystemTime};

use serde_json::{json, Value};

use crate::codex_oauth::{
    urlencoding_encode, CodexAuthStore, CodexTokens, CODEX_OAUTH_CLIENT_ID, CODEX_OAUTH_ISSUER,
    CODEX_OAUTH_TOKEN_URL, DEFAULT_CODEX_BASE_URL, USER_AGENT,
};
use crate::{KernelError, Result};

/// Interactive device-code login. Prints URL/code to stdout; blocks until done.
/// CLI entry point: same flow, printed to stdout.
pub fn device_code_login(store: &CodexAuthStore) -> Result<()> {
    device_code_login_with(store, &mut |prompt| {
        println!("OpenAI Codex device login");
        println!("  1. Open: {}", prompt.verification_url);
        println!("  2. Enter code: {}", prompt.user_code);
        println!("Waiting for authorization (Ctrl+C to cancel)...");
    })?;
    println!("Codex login successful. Tokens stored in Optimus auth.json (not Hermes/CLI).");
    Ok(())
}

/// What the user must do to finish a device login.
#[derive(Debug, Clone)]
pub struct DeviceCodePrompt {
    pub verification_url: String,
    pub user_code: String,
}

/// Device-code login, reporting the prompt through `on_prompt`.
///
/// Surfaces that own the screen (the TUI runs an alternate screen in raw mode)
/// cannot have this print to stdout, so the prompt is handed back instead of
/// written out. [`device_code_login`] is the stdout-printing CLI wrapper.
pub fn device_code_login_with(
    store: &CodexAuthStore,
    on_prompt: &mut dyn FnMut(DeviceCodePrompt),
) -> Result<()> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .build();
    let resp = agent
        .post(&format!(
            "{CODEX_OAUTH_ISSUER}/api/accounts/deviceauth/usercode"
        ))
        .set("Content-Type", "application/json")
        .set("User-Agent", USER_AGENT)
        .send_json(json!({ "client_id": CODEX_OAUTH_CLIENT_ID }))
        .map_err(|e| KernelError::Model(format!("device code request failed: {e}")))?;
    let status = resp.status();
    let text = resp
        .into_string()
        .map_err(|e| KernelError::Model(e.to_string()))?;
    if status != 200 {
        return Err(KernelError::Model(format!(
            "device code HTTP {status}: {}",
            text.chars().take(200).collect::<String>()
        )));
    }
    let data: Value = serde_json::from_str(&text)?;
    let user_code = data
        .get("user_code")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let device_auth_id = data
        .get("device_auth_id")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let interval = data
        .get("interval")
        .and_then(|x| {
            x.as_u64()
                .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(5)
        .max(3);
    if user_code.is_empty() || device_auth_id.is_empty() {
        return Err(KernelError::Model("device code response incomplete".into()));
    }
    on_prompt(DeviceCodePrompt {
        verification_url: format!("{CODEX_OAUTH_ISSUER}/codex/device"),
        user_code: user_code.clone(),
    });

    let start = SystemTime::now();
    let max_wait = Duration::from_secs(15 * 60);
    let code_resp = loop {
        if start.elapsed().unwrap_or_default() > max_wait {
            return Err(KernelError::Model("device login timed out".into()));
        }
        thread::sleep(Duration::from_secs(interval));
        let poll = agent
            .post(&format!(
                "{CODEX_OAUTH_ISSUER}/api/accounts/deviceauth/token"
            ))
            .set("Content-Type", "application/json")
            .set("User-Agent", USER_AGENT)
            .send_json(json!({
                "device_auth_id": device_auth_id,
                "user_code": user_code,
            }));
        match poll {
            Ok(r) => {
                let st = r.status();
                let body = r.into_string().unwrap_or_default();
                if st == 200 {
                    break serde_json::from_str::<Value>(&body)
                        .map_err(|e| KernelError::Model(e.to_string()))?;
                }
                if st == 403 || st == 404 {
                    continue;
                }
                return Err(KernelError::Model(format!(
                    "device poll HTTP {st}: {}",
                    body.chars().take(200).collect::<String>()
                )));
            }
            Err(_) => continue,
        }
    };

    let authorization_code = code_resp
        .get("authorization_code")
        .and_then(|x| x.as_str())
        .unwrap_or("");
    let code_verifier = code_resp
        .get("code_verifier")
        .and_then(|x| x.as_str())
        .unwrap_or("");
    if authorization_code.is_empty() || code_verifier.is_empty() {
        return Err(KernelError::Model(
            "device auth missing authorization_code/code_verifier".into(),
        ));
    }
    let redirect_uri = format!("{CODEX_OAUTH_ISSUER}/deviceauth/callback");
    let exchange = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
        urlencoding_encode(authorization_code),
        urlencoding_encode(&redirect_uri),
        urlencoding_encode(CODEX_OAUTH_CLIENT_ID),
        urlencoding_encode(code_verifier),
    );
    let tok = agent
        .post(CODEX_OAUTH_TOKEN_URL)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .set("Accept", "application/json")
        .set("User-Agent", USER_AGENT)
        .send_string(&exchange)
        .map_err(|e| KernelError::Model(format!("token exchange failed: {e}")))?;
    let st = tok.status();
    let body = tok
        .into_string()
        .map_err(|e| KernelError::Model(e.to_string()))?;
    if st != 200 {
        return Err(KernelError::Model(format!(
            "token exchange HTTP {st}: {}",
            body.chars().take(200).collect::<String>()
        )));
    }
    let v: Value = serde_json::from_str(&body)?;
    let access = v
        .get("access_token")
        .and_then(|x| x.as_str())
        .ok_or_else(|| KernelError::Model("no access_token".into()))?
        .to_string();
    let refresh = v
        .get("refresh_token")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    store.save_tokens(
        CodexTokens {
            access_token: access,
            refresh_token: refresh,
            account_id: None,
            id_token: v
                .get("id_token")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string()),
        },
        DEFAULT_CODEX_BASE_URL,
        "device_code",
    )?;
    Ok(())
}
