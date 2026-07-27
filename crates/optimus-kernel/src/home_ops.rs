//! Operations addressed by Optimus home rather than by an open kernel.
//!
//! Split out of `lib.rs` under the module-size law. Surface conveniences —
//! cron storage and session listing — that callers reach without a turn.

use super::*;

/// Open cron DB under Optimus home.
pub fn open_cron(home: impl AsRef<Path>) -> Result<CronStore> {
    Ok(CronStore::open(home.as_ref().join("cron.db"))?)
}

/// Run all due cron jobs with offline/codex/openai providers. Returns per-job result rows.
pub fn tick_cron(home: impl AsRef<Path>) -> Result<Vec<serde_json::Value>> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let home = home.as_ref();
    let mut store = open_cron(home)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let claims = store.claim_due(now, Uuid::new_v4(), 900)?;
    let mut out = Vec::new();
    for claim in claims {
        let job = claim.job();
        let status = (|| -> Result<String> {
            let mut kernel = Kernel::open(home, KernelConfig::default())?;
            let route = resolve_route(
                home,
                &RouteRequest::standard(RouteSurface::Cron, &job.provider, None),
            )?;
            match route.provider {
                ProviderId::Offline => {
                    let mut model = ScriptedModel::new(vec![CompletionResponse {
                        text: Some(format!("[cron:{}] {}", job.name, job.prompt)),
                        tool_calls: vec![],
                    }]);
                    let r = kernel.turn(&mut model, &job.prompt)?;
                    Ok(format!(
                        "ok steps={} text={}",
                        r.steps,
                        summarize(&r.assistant_text)
                    ))
                }
                ProviderId::Codex => {
                    let mut cfg = CodexOAuthConfig::from_env(home);
                    cfg.model = route.model.as_str().into();
                    let mut model = CodexOAuthModel::new(cfg)?;
                    let r = kernel.turn(&mut model, &job.prompt)?;
                    Ok(format!(
                        "ok steps={} text={}",
                        r.steps,
                        summarize(&r.assistant_text)
                    ))
                }
                ProviderId::OpenAiCompat => {
                    let cfg = OpenAiCompatConfig::from_env()?;
                    let mut model = OpenAiCompatModel::new(cfg);
                    let r = kernel.turn(&mut model, &job.prompt)?;
                    Ok(format!(
                        "ok steps={} text={}",
                        r.steps,
                        summarize(&r.assistant_text)
                    ))
                }
            }
        })();
        let mut status_s = match &status {
            Ok(s) => s.clone(),
            Err(e) => format!("err: {e}"),
        };
        let completed_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(now);
        if let Err(error) = store.complete_claim(&claim, &status_s, completed_unix) {
            status_s = format!("err: cron completion was not committed: {error}");
        }
        out.push(json!({
            "id": job.id.to_string(),
            "name": job.name,
            "status": status_s,
        }));
    }
    Ok(out)
}

/// List chat sessions under an Optimus home directory.
pub fn list_sessions(home: impl AsRef<Path>) -> Result<Vec<SessionMeta>> {
    let store = SessionStore::open(home.as_ref().join("sessions.db"))?;
    store.list()
}

/// Load one session's messages for UI resume (no model call).
pub fn get_session(home: impl AsRef<Path>, id: Uuid) -> Result<SessionDetail> {
    let store = SessionStore::open(home.as_ref().join("sessions.db"))?;
    let (packs, messages, title) = store.load(id)?;
    Ok(SessionDetail {
        id,
        title,
        packs,
        messages,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDetail {
    pub id: Uuid,
    pub title: String,
    pub packs: Vec<String>,
    pub messages: Vec<Message>,
}
