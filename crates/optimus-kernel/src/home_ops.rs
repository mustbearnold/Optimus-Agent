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
    tick_cron_with(home, execute_cron_job)
}

fn execute_cron_job(home: &Path, job: &CronJob, route: &RouteDecision) -> Result<String> {
    let mut kernel = Kernel::open(home, KernelConfig::default())?;
    match route.provider {
        ProviderId::Offline => {
            let mut model = ScriptedModel::new(vec![CompletionResponse {
                text: Some(format!("[cron:{}] {}", job.name, job.prompt)),
                tool_calls: vec![],
            }]);
            let result = kernel.turn(&mut model, &job.prompt)?;
            Ok(format!(
                "ok steps={} text={}",
                result.steps,
                summarize(&result.assistant_text)
            ))
        }
        ProviderId::Codex => {
            let mut config = CodexOAuthConfig::from_env(home);
            config.model = route.model.as_str().into();
            let mut model = CodexOAuthModel::new(config)?;
            let result = kernel.turn(&mut model, &job.prompt)?;
            Ok(format!(
                "ok steps={} text={}",
                result.steps,
                summarize(&result.assistant_text)
            ))
        }
        ProviderId::OpenAiCompat => {
            let config =
                apply_resolved_openai_model(OpenAiCompatConfig::from_env()?, route.model.as_str());
            let mut model = OpenAiCompatModel::new(config);
            let result = kernel.turn(&mut model, &job.prompt)?;
            Ok(format!(
                "ok steps={} text={}",
                result.steps,
                summarize(&result.assistant_text)
            ))
        }
    }
}

fn tick_cron_with<F>(home: impl AsRef<Path>, mut execute: F) -> Result<Vec<serde_json::Value>>
where
    F: FnMut(&Path, &CronJob, &RouteDecision) -> Result<String>,
{
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
        let status = (|| {
            let route = resolve_route(
                home,
                &RouteRequest::standard(RouteSurface::Cron, &job.provider, None),
            )?;
            execute(home, job, &route)
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

fn apply_resolved_openai_model(
    mut config: OpenAiCompatConfig,
    resolved_model: &str,
) -> OpenAiCompatConfig {
    config.model = resolved_model.into();
    config
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

#[cfg(test)]
mod tests {
    use super::{apply_resolved_openai_model, tick_cron_with};
    use crate::{open_cron, OpenAiCompatConfig, ProviderId};

    #[test]
    fn cron_openai_adapter_uses_the_resolved_route_model() {
        let config = OpenAiCompatConfig {
            base_url: "https://example.invalid/v1".into(),
            api_key: "test-key".into(),
            model: "ambient-model".into(),
            organization: None,
            timeout_secs: 1,
        };

        assert_eq!(
            apply_resolved_openai_model(config, "routed-model").model,
            "routed-model"
        );
    }

    #[test]
    fn legacy_persisted_openai_schedule_reaches_a_canonical_tick_route() {
        let home = tempfile::tempdir().unwrap();
        let store = open_cron(home.path()).unwrap();
        let job = store.add("legacy", 5, "tick", "openai_compat").unwrap();
        rusqlite::Connection::open(home.path().join("cron.db"))
            .unwrap()
            .execute(
                "UPDATE cron_jobs SET next_run_unix=0 WHERE id=?1",
                [job.id.to_string()],
            )
            .unwrap();

        let rows = tick_cron_with(home.path(), |_home, scheduled, route| {
            assert_eq!(scheduled.provider, "openai_compat");
            assert_eq!(route.provider, ProviderId::OpenAiCompat);
            assert_eq!(route.model.as_str(), "gpt-4.1");
            Ok("ok legacy canonical route".into())
        })
        .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["status"], "ok legacy canonical route");
    }
}
