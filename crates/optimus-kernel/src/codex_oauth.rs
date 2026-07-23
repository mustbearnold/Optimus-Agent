//! OpenAI Codex OAuth: token store, refresh, device login, Responses API provider.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    atomic_write_user_only, CancellationToken, CompletionRequest, CompletionResponse,
    CredentialProtector, KernelError, ModelProvider, Result, Role, SystemCredentialProtector,
    ToolCall, ToolSchema,
};

pub const CODEX_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const CODEX_OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub const CODEX_OAUTH_ISSUER: &str = "https://auth.openai.com";
pub const DEFAULT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
const REFRESH_SKEW_SECS: u64 = 120;
const USER_AGENT: &str = "codex_cli_rs/0.0.0 (Optimus Agent)";
const ORIGINATOR: &str = "codex_cli_rs";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodexTokens {
    pub access_token: String,
    pub refresh_token: String,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub id_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AuthFile {
    version: u32,
    #[serde(default)]
    providers: AuthProviders,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProtectedAuthEnvelope {
    version: u32,
    protection: String,
    ciphertext_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AuthProviders {
    #[serde(rename = "openai-codex", default)]
    openai_codex: Option<CodexProviderState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CodexProviderState {
    tokens: CodexTokens,
    #[serde(default)]
    last_refresh: Option<String>,
    #[serde(default)]
    auth_mode: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CodexAuthStatus {
    pub present: bool,
    pub access_expiring: bool,
    pub has_refresh: bool,
    pub source_note: String,
    pub base_url: String,
    pub account_id: Option<String>,
}

pub struct CodexAuthStore {
    path: PathBuf,
    lock_path: PathBuf,
    protector: Arc<dyn CredentialProtector>,
}

impl CodexAuthStore {
    pub fn open(home: impl AsRef<Path>) -> Result<Self> {
        let home = home.as_ref();
        fs::create_dir_all(home)?;
        Ok(Self {
            path: home.join("auth.json"),
            lock_path: home.join("auth.lock"),
            protector: Arc::new(SystemCredentialProtector),
        })
    }

    pub fn open_with_protector(
        home: impl AsRef<Path>,
        protector: Arc<dyn CredentialProtector>,
    ) -> Result<Self> {
        let home = home.as_ref();
        fs::create_dir_all(home)?;
        Ok(Self {
            path: home.join("auth.json"),
            lock_path: home.join("auth.lock"),
            protector,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn lock_exclusive(&self) -> Result<File> {
        if fs::symlink_metadata(&self.lock_path)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(KernelError::Model(
                "credential lock file must not be a symlink".into(),
            ));
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&self.lock_path)?;
        FileExt::lock_exclusive(&file)?;
        Ok(file)
    }

    fn load_unlocked(&self) -> Result<AuthFile> {
        if !self.path.exists() {
            return Ok(AuthFile {
                version: 1,
                ..Default::default()
            });
        }
        let metadata = fs::symlink_metadata(&self.path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(KernelError::Model(
                "credential file must be a non-symlink regular file".into(),
            ));
        }
        let bytes = fs::read(&self.path)?;
        if let Ok(envelope) = serde_json::from_slice::<ProtectedAuthEnvelope>(&bytes) {
            crate::verify_user_only(&self.path)?;
            if envelope.version != 2 || envelope.protection != self.protector.label() {
                return Err(KernelError::Model(
                    "unsupported credential envelope or protection backend".into(),
                ));
            }
            let ciphertext = decode_hex(&envelope.ciphertext_hex)?;
            let plaintext = self.protector.unprotect(&ciphertext)?;
            return Ok(serde_json::from_slice(&plaintext)?);
        }
        let mut legacy: AuthFile = serde_json::from_slice(&bytes)?;
        legacy.version = 2;
        self.save_unlocked(&legacy)?;
        Ok(legacy)
    }

    fn save_unlocked(&self, file: &AuthFile) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let plaintext = serde_json::to_vec(file)?;
        let ciphertext = self.protector.protect(&plaintext)?;
        let envelope = ProtectedAuthEnvelope {
            version: 2,
            protection: self.protector.label().into(),
            ciphertext_hex: encode_hex(&ciphertext),
        };
        atomic_write_user_only(&self.path, &serde_json::to_vec_pretty(&envelope)?)
    }

    pub fn status(&self) -> Result<CodexAuthStatus> {
        let _lock = self.lock_exclusive()?;
        let file = self.load_unlocked()?;
        match file.providers.openai_codex {
            None => Ok(CodexAuthStatus {
                present: false,
                access_expiring: true,
                has_refresh: false,
                source_note: "no Optimus Codex credentials".into(),
                base_url: DEFAULT_CODEX_BASE_URL.into(),
                account_id: None,
            }),
            Some(st) => Ok(CodexAuthStatus {
                present: !st.tokens.access_token.is_empty(),
                access_expiring: jwt_expiring(&st.tokens.access_token, REFRESH_SKEW_SECS),
                has_refresh: !st.tokens.refresh_token.is_empty(),
                source_note: st.auth_mode.unwrap_or_else(|| "stored".into()),
                base_url: st.base_url.unwrap_or_else(|| DEFAULT_CODEX_BASE_URL.into()),
                account_id: st.tokens.account_id,
            }),
        }
    }

    pub fn save_tokens(&self, tokens: CodexTokens, base_url: &str, auth_mode: &str) -> Result<()> {
        let _lock = self.lock_exclusive()?;
        let mut file = self.load_unlocked()?;
        self.save_tokens_unlocked(&mut file, tokens, base_url, auth_mode)
    }

    fn save_tokens_unlocked(
        &self,
        file: &mut AuthFile,
        tokens: CodexTokens,
        base_url: &str,
        auth_mode: &str,
    ) -> Result<()> {
        file.version = 2;
        file.providers.openai_codex = Some(CodexProviderState {
            tokens,
            last_refresh: Some(now_iso()),
            auth_mode: Some(auth_mode.into()),
            base_url: Some(base_url.trim_end_matches('/').into()),
        });
        self.save_unlocked(file)
    }

    pub fn clear(&self) -> Result<()> {
        let _lock = self.lock_exclusive()?;
        let mut file = self.load_unlocked()?;
        file.providers.openai_codex = None;
        self.save_unlocked(&file)
    }

    pub fn import_from_hermes(&self) -> Result<String> {
        let path = hermes_auth_path().ok_or_else(|| {
            KernelError::Model("Hermes auth.json not found (LOCALAPPDATA/hermes/auth.json)".into())
        })?;
        let text = fs::read_to_string(&path)?;
        let v: Value = serde_json::from_str(&text)?;
        let tokens = extract_codex_tokens_from_hermes(&v).ok_or_else(|| {
            KernelError::Model("Hermes auth.json has no openai-codex tokens".into())
        })?;
        let base = v
            .pointer("/credential_pool/openai-codex/0/base_url")
            .and_then(|x| x.as_str())
            .unwrap_or(DEFAULT_CODEX_BASE_URL);
        self.save_tokens(tokens, base, "import:hermes")?;
        Ok(format!("imported Codex OAuth from {}", path.display()))
    }

    pub fn import_from_codex_cli(&self) -> Result<String> {
        let path = codex_cli_auth_path();
        if !path.exists() {
            return Err(KernelError::Model(format!(
                "Codex CLI auth not found at {}",
                path.display()
            )));
        }
        let text = fs::read_to_string(&path)?;
        let v: Value = serde_json::from_str(&text)?;
        let tokens = extract_codex_tokens_from_codex_cli(&v).ok_or_else(|| {
            KernelError::Model("Codex CLI auth.json missing tokens.access_token".into())
        })?;
        self.save_tokens(tokens, DEFAULT_CODEX_BASE_URL, "import:codex_cli")?;
        Ok(format!("imported Codex OAuth from {}", path.display()))
    }

    /// Resolve a fresh access token, refreshing if near expiry.
    pub fn resolve_access_token(&self) -> Result<(String, CodexTokens, String)> {
        let _lock = self.lock_exclusive()?;
        let mut file = self.load_unlocked()?;
        let mut st = file.providers.openai_codex.take().ok_or_else(|| {
            KernelError::Model(
                "No Codex credentials. Run `optimus auth codex import` or `optimus auth codex login`."
                    .into(),
            )
        })?;
        if st.tokens.access_token.is_empty() {
            return Err(KernelError::Model("Codex access_token empty".into()));
        }
        if jwt_expiring(&st.tokens.access_token, REFRESH_SKEW_SECS) {
            if st.tokens.refresh_token.is_empty() {
                return Err(KernelError::Model(
                    "Codex access expiring and no refresh_token".into(),
                ));
            }
            let refreshed = refresh_codex_tokens(&st.tokens.refresh_token)?;
            st.tokens.access_token = refreshed.access_token;
            if !refreshed.refresh_token.is_empty() {
                st.tokens.refresh_token = refreshed.refresh_token;
            }
            if refreshed.account_id.is_some() {
                st.tokens.account_id = refreshed.account_id;
            }
            let base = st
                .base_url
                .clone()
                .unwrap_or_else(|| DEFAULT_CODEX_BASE_URL.into());
            self.save_tokens_unlocked(&mut file, st.tokens.clone(), &base, "refresh")?;
        }
        let base = st.base_url.unwrap_or_else(|| DEFAULT_CODEX_BASE_URL.into());
        Ok((st.tokens.access_token.clone(), st.tokens, base))
    }
}

fn hermes_auth_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("HERMES_HOME") {
        let cand = PathBuf::from(p).join("auth.json");
        if cand.exists() {
            return Some(cand);
        }
    }
    if let Ok(la) = std::env::var("LOCALAPPDATA") {
        let cand = PathBuf::from(la).join("hermes").join("auth.json");
        if cand.exists() {
            return Some(cand);
        }
    }
    let home = dirs_home()?;
    let cand = home.join(".hermes").join("auth.json");
    if cand.exists() {
        Some(cand)
    } else {
        None
    }
}

fn codex_cli_auth_path() -> PathBuf {
    if let Ok(h) = std::env::var("CODEX_HOME") {
        return PathBuf::from(h).join("auth.json");
    }
    dirs_home()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codex")
        .join("auth.json")
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

pub fn extract_codex_tokens_from_hermes(v: &Value) -> Option<CodexTokens> {
    if let Some(t) = v.pointer("/providers/openai-codex/tokens") {
        if let Some(tok) = tokens_from_value(t) {
            return Some(tok);
        }
    }
    if let Some(arr) = v
        .pointer("/credential_pool/openai-codex")
        .and_then(|x| x.as_array())
    {
        for e in arr {
            if let Some(tok) = tokens_from_pool_entry(e) {
                return Some(tok);
            }
        }
    }
    None
}

pub fn extract_codex_tokens_from_codex_cli(v: &Value) -> Option<CodexTokens> {
    v.get("tokens").and_then(tokens_from_value)
}

fn tokens_from_value(t: &Value) -> Option<CodexTokens> {
    let access = t.get("access_token")?.as_str()?.to_string();
    if access.is_empty() {
        return None;
    }
    let refresh = t
        .get("refresh_token")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let account_id = t
        .get("account_id")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    let id_token = t
        .get("id_token")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    Some(CodexTokens {
        access_token: access,
        refresh_token: refresh,
        account_id,
        id_token,
    })
}

fn tokens_from_pool_entry(e: &Value) -> Option<CodexTokens> {
    let access = e.get("access_token")?.as_str()?.to_string();
    if access.is_empty() {
        return None;
    }
    Some(CodexTokens {
        access_token: access,
        refresh_token: e
            .get("refresh_token")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        account_id: None,
        id_token: None,
    })
}

pub fn refresh_codex_tokens(refresh_token: &str) -> Result<CodexTokens> {
    let body = format!(
        "grant_type=refresh_token&refresh_token={}&client_id={}",
        urlencoding_encode(refresh_token),
        urlencoding_encode(CODEX_OAUTH_CLIENT_ID)
    );
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .build();
    let resp = agent
        .post(CODEX_OAUTH_TOKEN_URL)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .set("Accept", "application/json")
        .set("User-Agent", USER_AGENT)
        .send_string(&body)
        .map_err(|e| KernelError::Model(format!("Codex refresh failed: {e}")))?;
    let status = resp.status();
    let text = resp
        .into_string()
        .map_err(|e| KernelError::Model(e.to_string()))?;
    if status != 200 {
        let snip: String = text.chars().take(300).collect();
        return Err(KernelError::Model(format!(
            "Codex refresh HTTP {status}: {snip}"
        )));
    }
    let v: Value = serde_json::from_str(&text)?;
    let access = v
        .get("access_token")
        .and_then(|x| x.as_str())
        .ok_or_else(|| KernelError::Model("refresh missing access_token".into()))?
        .to_string();
    let refresh = v
        .get("refresh_token")
        .and_then(|x| x.as_str())
        .unwrap_or(refresh_token)
        .to_string();
    Ok(CodexTokens {
        access_token: access,
        refresh_token: refresh,
        account_id: None,
        id_token: v
            .get("id_token")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
    })
}

/// Interactive device-code login. Prints URL/code to stdout; blocks until done.
pub fn device_code_login(store: &CodexAuthStore) -> Result<()> {
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
    println!("OpenAI Codex device login");
    println!("  1. Open: {CODEX_OAUTH_ISSUER}/codex/device");
    println!("  2. Enter code: {user_code}");
    println!("Waiting for authorization (Ctrl+C to cancel)...");

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
    println!("Codex login successful. Tokens stored in Optimus auth.json (not Hermes/CLI).");
    Ok(())
}

// --- Responses API provider ---

#[derive(Debug, Clone)]
pub struct CodexOAuthConfig {
    pub home: PathBuf,
    pub model: String,
    pub timeout_secs: u64,
}

impl CodexOAuthConfig {
    pub fn from_env(home: impl AsRef<Path>) -> Self {
        let raw = std::env::var("OPTIMUS_CODEX_MODEL")
            .unwrap_or_else(|_| crate::DEFAULT_CODEX_MODEL.into());
        Self {
            home: home.as_ref().to_path_buf(),
            model: crate::sanitize_codex_oauth_model(&raw),
            timeout_secs: 180,
        }
    }
}

pub struct CodexOAuthModel {
    pub config: CodexOAuthConfig,
    /// Test override for responses URL.
    pub responses_url_override: Option<String>,
    pub store: CodexAuthStore,
}

impl CodexOAuthModel {
    pub fn new(config: CodexOAuthConfig) -> Result<Self> {
        let store = CodexAuthStore::open(&config.home)?;
        Ok(Self {
            config,
            responses_url_override: None,
            store,
        })
    }
}

impl ModelProvider for CodexOAuthModel {
    fn identity(&self) -> (String, String) {
        ("codex".into(), self.config.model.clone())
    }

    fn complete(&mut self, request: CompletionRequest) -> Result<CompletionResponse> {
        self.complete_streaming(request, &mut |_| {})
    }

    fn complete_streaming(
        &mut self,
        request: CompletionRequest,
        sink: &mut dyn FnMut(crate::StreamEvent),
    ) -> Result<CompletionResponse> {
        let cancellation = CancellationToken::new();
        self.complete_streaming_cancellable(request, sink, &cancellation)
    }

    fn complete_streaming_cancellable(
        &mut self,
        request: CompletionRequest,
        sink: &mut dyn FnMut(crate::StreamEvent),
        cancellation: &CancellationToken,
    ) -> Result<CompletionResponse> {
        use std::io::{BufReader, Read};

        cancellation.check()?;
        let response_deadline = Instant::now() + Duration::from_secs(self.config.timeout_secs);
        let (access, tokens, base) = self.store.resolve_access_token()?;
        let url = self.responses_url_override.clone().unwrap_or_else(|| {
            let b = base.trim_end_matches('/');
            if b.ends_with("/responses") {
                b.to_string()
            } else {
                format!("{b}/responses")
            }
        });
        let model = crate::sanitize_codex_oauth_model(&self.config.model);
        let mut body = to_codex_responses_request(&request, &model);
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(self.config.timeout_secs))
            .timeout_read(Duration::from_millis(250))
            .build();
        let account_id = tokens
            .account_id
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| chatgpt_account_id_from_jwt(&access));

        let post_once = |body: &Value| -> Result<ureq::Response> {
            let mut req = agent
                .post(&url)
                .set("Content-Type", "application/json")
                .set("Authorization", &format!("Bearer {access}"))
                .set("User-Agent", USER_AGENT)
                .set("originator", ORIGINATOR)
                .set("OpenAI-Beta", "responses=experimental")
                .set("Accept", "text/event-stream");
            if let Some(aid) = &account_id {
                req = req.set("ChatGPT-Account-ID", aid);
            }
            match req.send_json(body) {
                Ok(r) => Ok(r),
                Err(ureq::Error::Status(code, r)) => {
                    let text = r
                        .into_string()
                        .unwrap_or_else(|e| format!("<body unreadable: {e}>"));
                    let snip: String = text.chars().take(500).collect();
                    Err(KernelError::Model(format!("Codex HTTP {code}: {snip}")))
                }
                Err(e) => Err(KernelError::Model(format!("Codex responses failed: {e}"))),
            }
        };

        let resp = match post_once(&body) {
            Ok(r) => r,
            Err(first_err) => {
                // Retry once: slim history to system + last user, no reasoning.
                let mut slim = request.clone();
                let last_user = request
                    .messages
                    .iter()
                    .rev()
                    .find(|m| m.role == Role::User)
                    .cloned();
                slim.messages = request
                    .messages
                    .iter()
                    .filter(|m| m.role == Role::System)
                    .cloned()
                    .chain(last_user)
                    .collect();
                slim.reasoning_effort = None;
                body = to_codex_responses_request(&slim, &model);
                match post_once(&body) {
                    Ok(r) => r,
                    Err(e2) => {
                        return Err(KernelError::Model(format!("{first_err} | retry: {e2}")));
                    }
                }
            }
        };
        let status = resp.status();
        if !(200..300).contains(&status) {
            let text = resp
                .into_string()
                .map_err(|e| KernelError::Model(e.to_string()))?;
            let snip: String = text.chars().take(500).collect();
            return Err(KernelError::Model(format!("Codex HTTP {status}: {snip}")));
        }

        // Peek whether body is JSON (mock) or SSE.
        let mut reader = BufReader::new(resp.into_reader());
        let mut first = String::new();
        read_line_cancellable(&mut reader, &mut first, cancellation, response_deadline)?;
        let trimmed = first.trim_start().to_string();
        if trimmed.starts_with('{') {
            let mut rest = String::new();
            reader
                .read_to_string(&mut rest)
                .map_err(|e| KernelError::Model(e.to_string()))?;
            let text = format!("{first}{rest}");
            let value: Value =
                serde_json::from_str(&text).map_err(|e| KernelError::Model(e.to_string()))?;
            let resp = from_codex_responses_response(&value)?;
            if let Some(t) = &resp.text {
                if !t.is_empty() {
                    sink(crate::StreamEvent::TextDelta(t.clone()));
                }
            }
            return Ok(resp);
        }

        // SSE path: process first line then rest.
        let mut text_buf = String::new();
        let mut tool_calls = Vec::new();
        let mut completed_output: Option<Value> = None;
        let mut process_line = |line: &str| -> Result<()> {
            let line = line.trim();
            let Some(data) = line.strip_prefix("data:") else {
                return Ok(());
            };
            let data = data.strip_prefix(' ').unwrap_or(data);
            if data == "[DONE]" {
                return Ok(());
            }
            let ev: Value = serde_json::from_str(data).map_err(|error| {
                KernelError::Model(format!("Codex SSE event has invalid JSON: {error}"))
            })?;
            let ty = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match ty {
                "response.output_text.delta" => {
                    if let Some(d) = ev.get("delta").and_then(|x| x.as_str()) {
                        text_buf.push_str(d);
                        sink(crate::StreamEvent::TextDelta(d.to_string()));
                    }
                }
                "response.output_item.done" => {
                    if let Some(item) = ev.get("item") {
                        let item_ty = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        if item_ty == "function_call" {
                            let id = item
                                .get("call_id")
                                .or_else(|| item.get("id"))
                                .and_then(|x| x.as_str())
                                .filter(|value| !value.is_empty())
                                .ok_or_else(|| {
                                    KernelError::Model("tool_call missing non-empty id".into())
                                })?
                                .to_string();
                            let name = item
                                .get("name")
                                .and_then(|x| x.as_str())
                                .filter(|value| !value.is_empty())
                                .ok_or_else(|| {
                                    KernelError::Model("tool_call missing function name".into())
                                })?
                                .to_string();
                            let args_raw = item
                                .get("arguments")
                                .and_then(|x| x.as_str())
                                .ok_or_else(|| {
                                    KernelError::Model(format!(
                                        "tool_call {name} missing string arguments"
                                    ))
                                })?;
                            let arguments: Value =
                                serde_json::from_str(args_raw).map_err(|error| {
                                    KernelError::Model(format!(
                                        "tool_call {name} has invalid JSON arguments: {error}"
                                    ))
                                })?;
                            tool_calls.push(ToolCall {
                                id,
                                name,
                                arguments,
                            });
                        }
                    }
                }
                "response.completed" => {
                    if let Some(out) = ev.pointer("/response/output").cloned() {
                        completed_output = Some(out);
                    }
                }
                _ => {}
            }
            Ok(())
        };
        process_line(&first)?;
        let mut line = String::new();
        loop {
            line.clear();
            let n = read_line_cancellable(&mut reader, &mut line, cancellation, response_deadline)?;
            if n == 0 {
                break;
            }
            process_line(&line)?;
        }

        if let Some(out) = completed_output {
            if !out.as_array().is_some_and(|items| items.is_empty()) {
                let parsed = from_codex_responses_response(&json!({"output": out}))?;
                if tool_calls.is_empty() {
                    if !parsed.tool_calls.is_empty() {
                        return Ok(parsed);
                    }
                    if parsed.text.is_some() && text_buf.is_empty() {
                        if let Some(t) = &parsed.text {
                            sink(crate::StreamEvent::TextDelta(t.clone()));
                        }
                        return Ok(parsed);
                    }
                }
            }
        }
        if text_buf.is_empty() && tool_calls.is_empty() {
            return Err(KernelError::Model(
                "Codex SSE: empty text and no tool_calls".into(),
            ));
        }
        Ok(CompletionResponse {
            text: if text_buf.is_empty() {
                None
            } else {
                Some(text_buf)
            },
            tool_calls,
        })
    }
}

fn read_line_cancellable(
    reader: &mut impl std::io::BufRead,
    line: &mut String,
    cancellation: &CancellationToken,
    deadline: Instant,
) -> Result<usize> {
    loop {
        cancellation.check()?;
        match reader.read_line(line) {
            Ok(read) => return Ok(read),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                if Instant::now() >= deadline {
                    return Err(KernelError::Model("Codex response timed out".into()));
                }
            }
            Err(error) => return Err(KernelError::Model(error.to_string())),
        }
    }
}

pub fn chatgpt_account_id_from_jwt(access_token: &str) -> Option<String> {
    let mut parts = access_token.split('.');
    let _h = parts.next()?;
    let payload = parts.next()?;
    let bytes = b64url_decode(payload)?;
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    v.pointer("/https://api.openai.com/auth/chatgpt_account_id")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
}

pub fn from_codex_responses_sse(stream: &str) -> Result<CompletionResponse> {
    let mut text_buf = String::new();
    let mut tool_calls = Vec::new();
    let mut completed_output: Option<Value> = None;

    for line in stream.lines() {
        let line = line.trim();
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.strip_prefix(' ').unwrap_or(data);
        if data == "[DONE]" {
            break;
        }
        let ev: Value = serde_json::from_str(data).map_err(|error| {
            KernelError::Model(format!("Codex SSE event has invalid JSON: {error}"))
        })?;
        let ty = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match ty {
            "response.output_text.delta" => {
                if let Some(d) = ev.get("delta").and_then(|x| x.as_str()) {
                    text_buf.push_str(d);
                }
            }
            "response.output_item.done" => {
                if let Some(item) = ev.get("item") {
                    let item_ty = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if item_ty == "function_call" {
                        let id = item
                            .get("call_id")
                            .or_else(|| item.get("id"))
                            .and_then(|x| x.as_str())
                            .filter(|value| !value.is_empty())
                            .ok_or_else(|| {
                                KernelError::Model("tool_call missing non-empty id".into())
                            })?
                            .to_string();
                        let name = item
                            .get("name")
                            .and_then(|x| x.as_str())
                            .filter(|value| !value.is_empty())
                            .ok_or_else(|| {
                                KernelError::Model("tool_call missing function name".into())
                            })?
                            .to_string();
                        let args_raw =
                            item.get("arguments")
                                .and_then(|x| x.as_str())
                                .ok_or_else(|| {
                                    KernelError::Model(format!(
                                        "tool_call {name} missing string arguments"
                                    ))
                                })?;
                        let arguments: Value = serde_json::from_str(args_raw).map_err(|error| {
                            KernelError::Model(format!(
                                "tool_call {name} has invalid JSON arguments: {error}"
                            ))
                        })?;
                        tool_calls.push(ToolCall {
                            id,
                            name,
                            arguments,
                        });
                    }
                }
            }
            "response.completed" => {
                completed_output = ev.get("response").and_then(|r| r.get("output")).cloned();
            }
            _ => {}
        }
    }

    if let Some(output) = completed_output {
        if !output.as_array().is_some_and(|items| items.is_empty()) {
            let parsed = from_codex_responses_response(&json!({ "output": output }))?;
            if tool_calls.is_empty()
                && (!parsed.tool_calls.is_empty() || (text_buf.is_empty() && parsed.text.is_some()))
            {
                return Ok(parsed);
            }
        }
    }

    if text_buf.is_empty() && tool_calls.is_empty() {
        return Err(KernelError::Model(
            "Codex SSE: empty text and no tool_calls".into(),
        ));
    }
    Ok(CompletionResponse {
        text: if text_buf.is_empty() {
            None
        } else {
            Some(text_buf)
        },
        tool_calls,
    })
}

pub fn to_codex_responses_request(request: &CompletionRequest, model: &str) -> Value {
    let mut instructions = String::new();
    let mut input: Vec<Value> = Vec::new();
    let mut open_calls: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for m in &request.messages {
        match m.role {
            Role::System => {
                if !instructions.is_empty() {
                    instructions.push('\n');
                }
                instructions.push_str(&m.content);
            }
            Role::User => {
                // User message closes any dangling calls with synthetic empty outputs
                for id in std::mem::take(&mut open_calls) {
                    input.push(json!({
                        "type": "function_call_output",
                        "call_id": id,
                        "output": "{\"ok\":false,\"note\":\"dropped unpaired tool call before next user turn\"}",
                    }));
                }
                input.push(json!({
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": m.content}],
                }));
            }
            Role::Assistant => {
                if let Ok(calls) = serde_json::from_str::<Vec<ToolCall>>(&m.content) {
                    if !calls.is_empty() {
                        for c in calls {
                            let args = match &c.arguments {
                                Value::String(s) => s.clone(),
                                other => other.to_string(),
                            };
                            open_calls.insert(c.id.clone());
                            input.push(json!({
                                "type": "function_call",
                                "call_id": c.id,
                                "name": c.name,
                                "arguments": args,
                            }));
                        }
                        continue;
                    }
                }
                // Plain assistant text — don't send raw tool-call JSON as prose
                let text = m.content.trim();
                if text.starts_with("[{") && text.contains("\"name\"") {
                    continue;
                }
                if text.is_empty() {
                    continue;
                }
                input.push(json!({
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": m.content}],
                }));
            }
            Role::Tool => {
                let call_id = m.tool_call_id.clone().unwrap_or_else(|| "call".into());
                if !open_calls.remove(&call_id) {
                    // Orphan tool output with no matching function_call — skip (causes Codex 400)
                    continue;
                }
                let output = truncate_tool_output(&m.content, 4_000);
                input.push(json!({
                    "type": "function_call_output",
                    "call_id": call_id,
                    "output": output,
                }));
            }
        }
    }
    // Close any trailing open calls
    for id in std::mem::take(&mut open_calls) {
        input.push(json!({
            "type": "function_call_output",
            "call_id": id,
            "output": "{\"ok\":false,\"note\":\"unpaired tool call at end of history\"}",
        }));
    }

    // Bound history size: keep system instructions + last ~40 input items
    const MAX_INPUT_ITEMS: usize = 48;
    if input.len() > MAX_INPUT_ITEMS {
        let drop_n = input.len() - MAX_INPUT_ITEMS;
        input = input.split_off(drop_n);
        // If we split mid tool-call pair, drop leading outputs until a safe boundary
        while let Some(first) = input.first() {
            let ty = first.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if ty == "function_call_output" {
                input.remove(0);
            } else {
                break;
            }
        }
    }

    let tools: Vec<Value> = request.tools.iter().map(tool_to_responses).collect();
    let mut body = json!({
        "model": model,
        "input": input,
        "store": false,
        // ChatGPT Codex backend requires stream=true.
        "stream": true,
    });
    if !instructions.is_empty() {
        body["instructions"] = json!(instructions);
    }
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
        body["tool_choice"] = json!("auto");
    }
    if let Some(effort) = request.reasoning_effort.as_deref() {
        // ChatGPT Codex OAuth accepts nested reasoning.effort only.
        body["reasoning"] = json!({ "effort": effort });
    }
    body
}

fn truncate_tool_output(s: &str, max_chars: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= max_chars {
        return t.to_string();
    }
    let head: String = t.chars().take(max_chars.saturating_sub(40)).collect();
    format!("{head}\n…[truncated {} chars]", t.chars().count())
}

fn tool_to_responses(t: &ToolSchema) -> Value {
    json!({
        "type": "function",
        "name": t.id.as_str(),
        "description": t.description,
        "parameters": t.input_schema
    })
}

pub fn from_codex_responses_response(value: &Value) -> Result<CompletionResponse> {
    let mut text: Option<String> = None;
    let mut tool_calls = Vec::new();
    if let Some(output_value) = value.get("output") {
        let output = output_value
            .as_array()
            .ok_or_else(|| KernelError::Model("Codex output must be an array".into()))?;
        for item in output {
            let ty = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match ty {
                "message" => {
                    if let Some(parts) = item.get("content").and_then(|c| c.as_array()) {
                        let mut buf = String::new();
                        for p in parts {
                            let pt = p.get("type").and_then(|t| t.as_str()).unwrap_or("");
                            if pt == "output_text" || pt == "text" {
                                if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                                    buf.push_str(t);
                                }
                            }
                        }
                        if !buf.is_empty() {
                            text = Some(buf);
                        }
                    }
                }
                "function_call" => {
                    let id = item
                        .get("call_id")
                        .or_else(|| item.get("id"))
                        .and_then(|x| x.as_str())
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| KernelError::Model("tool_call missing non-empty id".into()))?
                        .to_string();
                    let name = item
                        .get("name")
                        .and_then(|x| x.as_str())
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| {
                            KernelError::Model("tool_call missing function name".into())
                        })?
                        .to_string();
                    let args_raw =
                        item.get("arguments")
                            .and_then(|x| x.as_str())
                            .ok_or_else(|| {
                                KernelError::Model(format!(
                                    "tool_call {name} missing string arguments"
                                ))
                            })?;
                    let arguments: Value = serde_json::from_str(args_raw).map_err(|error| {
                        KernelError::Model(format!(
                            "tool_call {name} has invalid JSON arguments: {error}"
                        ))
                    })?;
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments,
                    });
                }
                _ => {}
            }
        }
    }
    // Fallback: some gateways put final text on output_text
    if text.is_none() {
        if let Some(t) = value.get("output_text").and_then(|x| x.as_str()) {
            if !t.is_empty() {
                text = Some(t.to_string());
            }
        }
    }
    if text.is_none() && tool_calls.is_empty() {
        return Err(KernelError::Model(
            "Codex responses: empty text and no tool_calls".into(),
        ));
    }
    Ok(CompletionResponse { text, tool_calls })
}

// --- helpers ---

pub fn jwt_expiring(token: &str, skew_secs: u64) -> bool {
    let exp = match jwt_exp(token) {
        Some(e) => e,
        None => return false, // opaque token — don't force refresh
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    exp <= now.saturating_add(skew_secs)
}

fn jwt_exp(token: &str) -> Option<u64> {
    let mut parts = token.split('.');
    let _h = parts.next()?;
    let payload = parts.next()?;
    let bytes = b64url_decode(payload)?;
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    v.get("exp")?.as_u64()
}

fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    let mut s = s.replace('-', "+").replace('_', "/");
    while s.len() % 4 != 0 {
        s.push('=');
    }
    // minimal base64 decode without crate
    base64_decode(&s)
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut table = [255u8; 256];
    for (i, &c) in T.iter().enumerate() {
        table[c as usize] = i as u8;
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &b in bytes {
        if b == b'=' {
            break;
        }
        let v = table[b as usize];
        if v == 255 {
            continue;
        }
        buf = (buf << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn urlencoding_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn now_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("ts:{secs}")
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn decode_hex(value: &str) -> Result<Vec<u8>> {
    if value.len() % 2 != 0 {
        return Err(KernelError::Model(
            "credential envelope has malformed ciphertext".into(),
        ));
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16);
            let low = (pair[1] as char).to_digit(16);
            match (high, low) {
                (Some(high), Some(low)) => Ok(((high << 4) | low) as u8),
                _ => Err(KernelError::Model(
                    "credential envelope has malformed ciphertext".into(),
                )),
            }
        })
        .collect()
}

// silence unused import warnings in some build modes
#[allow(dead_code)]
fn _touch_io() {
    let _: fn(&mut dyn Read) = |_| {};
    let _: fn(&mut dyn Write) = |_| {};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Message;
    use optimus_packs::CapabilitySession;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;

    struct SlowIdentityProtector {
        active: AtomicUsize,
        max_active: AtomicUsize,
    }

    impl SlowIdentityProtector {
        fn new() -> Self {
            Self {
                active: AtomicUsize::new(0),
                max_active: AtomicUsize::new(0),
            }
        }
    }

    impl CredentialProtector for SlowIdentityProtector {
        fn label(&self) -> &'static str {
            "test-identity"
        }

        fn protect(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(20));
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(plaintext.to_vec())
        }

        fn unprotect(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
            Ok(ciphertext.to_vec())
        }
    }

    #[test]
    fn auth_store_serializes_cross_instance_mutations() {
        let home = tempdir().unwrap();
        let protector = Arc::new(SlowIdentityProtector::new());
        let mut workers = Vec::new();
        for worker in 0..8 {
            let home = home.path().to_path_buf();
            let protector_for_store: Arc<dyn CredentialProtector> = protector.clone();
            workers.push(std::thread::spawn(move || {
                let store = CodexAuthStore::open_with_protector(home, protector_for_store).unwrap();
                store
                    .save_tokens(
                        CodexTokens {
                            access_token: format!("access-{worker}"),
                            refresh_token: format!("refresh-{worker}"),
                            account_id: None,
                            id_token: None,
                        },
                        DEFAULT_CODEX_BASE_URL,
                        "concurrency-test",
                    )
                    .unwrap();
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }

        assert_eq!(protector.max_active.load(Ordering::SeqCst), 1);
        let protector_for_store: Arc<dyn CredentialProtector> = protector;
        let status = CodexAuthStore::open_with_protector(home.path(), protector_for_store)
            .unwrap()
            .status()
            .unwrap();
        assert!(status.present);
    }

    #[test]
    fn extracts_hermes_provider_tokens() {
        let v = json!({
            "providers": {
                "openai-codex": {
                    "tokens": {
                        "access_token": "acc",
                        "refresh_token": "ref",
                        "account_id": "aid"
                    }
                }
            }
        });
        let t = extract_codex_tokens_from_hermes(&v).unwrap();
        assert_eq!(t.access_token, "acc");
        assert_eq!(t.refresh_token, "ref");
        assert_eq!(t.account_id.as_deref(), Some("aid"));
    }

    #[test]
    fn extracts_hermes_pool_tokens() {
        let v = json!({
            "credential_pool": {
                "openai-codex": [{
                    "access_token": "pool-acc",
                    "refresh_token": "pool-ref"
                }]
            }
        });
        let t = extract_codex_tokens_from_hermes(&v).unwrap();
        assert_eq!(t.access_token, "pool-acc");
    }

    #[test]
    fn maps_responses_tools_and_parses_function_call() {
        let req = CompletionRequest {
            messages: vec![
                Message {
                    role: Role::System,
                    content: "sys".into(),
                    tool_call_id: None,
                    name: None,
                },
                Message {
                    role: Role::User,
                    content: "hi".into(),
                    tool_call_id: None,
                    name: None,
                },
            ],
            tools: vec![CapabilitySession::with_defaults()
                .resolve_loaded_tool("memory_recall")
                .unwrap()
                .clone()],
            ..Default::default()
        };
        let body = to_codex_responses_request(&req, "gpt-5.4");
        assert_eq!(body["model"], "gpt-5.4");
        assert_eq!(body["stream"], true);
        assert_eq!(body["instructions"], "sys");
        assert_eq!(body["tools"][0]["name"], "memory_recall");
        assert_eq!(body["tools"][0]["parameters"], req.tools[0].input_schema);
        assert_eq!(
            body["tools"][0]["parameters"]["additionalProperties"],
            false
        );

        let req2 = CompletionRequest {
            messages: req.messages.clone(),
            tools: req.tools.clone(),
            reasoning_effort: Some("xhigh".into()),
            fast_mode: true,
        };
        let body2 = to_codex_responses_request(&req2, "gpt-5.4");
        assert_eq!(body2["reasoning"]["effort"], "xhigh");
        assert!(body2.get("reasoning_effort").is_none());
        assert!(body2.get("service_tier").is_none());

        let resp = json!({
            "output": [
                {
                    "type": "function_call",
                    "call_id": "c1",
                    "name": "memory_recall",
                    "arguments": "{\"subject\":\"user\"}"
                }
            ]
        });
        let r = from_codex_responses_response(&resp).unwrap();
        assert_eq!(r.tool_calls[0].name, "memory_recall");
        assert_eq!(r.tool_calls[0].arguments["subject"], "user");
    }

    #[test]
    fn parses_sse_text_deltas() {
        let sse = "\
event: response.output_text.delta\n\
data: {\"type\":\"response.output_text.delta\",\"delta\":\"he\"}\n\
\n\
event: response.output_text.delta\n\
data: {\"type\":\"response.output_text.delta\",\"delta\":\"llo\"}\n\
\n\
event: response.completed\n\
data: {\"type\":\"response.completed\",\"response\":{\"output\":[]}}\n\
";
        let r = from_codex_responses_sse(sse).unwrap();
        assert_eq!(r.text.as_deref(), Some("hello"));
    }

    #[test]
    fn rejects_malformed_tool_arguments_in_json_and_sse() {
        let response = json!({
            "output": [{
                "type": "function_call",
                "call_id": "bad-json",
                "name": "memory_recall",
                "arguments": "{not-json"
            }]
        });
        assert!(matches!(
            from_codex_responses_response(&response),
            Err(KernelError::Model(message))
                if message.contains("memory_recall") && message.contains("invalid JSON arguments")
        ));

        let sse = "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",\"call_id\":\"bad-sse\",\"name\":\"memory_recall\",\"arguments\":\"{not-json\"}}\n\n";
        assert!(matches!(
            from_codex_responses_sse(sse),
            Err(KernelError::Model(message))
                if message.contains("memory_recall") && message.contains("invalid JSON arguments")
        ));

        let mut missing = response.clone();
        missing
            .pointer_mut("/output/0")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("arguments");
        assert!(matches!(
            from_codex_responses_response(&missing),
            Err(KernelError::Model(message))
                if message.contains("memory_recall") && message.contains("missing string arguments")
        ));

        let nameless = json!({
            "output": [{
                "type": "function_call",
                "call_id": "no-name",
                "arguments": "{}"
            }]
        });
        assert!(matches!(
            from_codex_responses_response(&nameless),
            Err(KernelError::Model(message)) if message.contains("missing function name")
        ));

        let malformed_completed = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"output\":[{\"type\":\"function_call\",\"call_id\":\"completed-bad\",\"name\":\"memory_recall\"}]}}\n\n";
        assert!(matches!(
            from_codex_responses_sse(malformed_completed),
            Err(KernelError::Model(message))
                if message.contains("memory_recall") && message.contains("missing string arguments")
        ));

        let wrong_container = json!({"output": {"bad": true}, "output_text": "safe text"});
        assert!(matches!(
            from_codex_responses_response(&wrong_container),
            Err(KernelError::Model(message)) if message.contains("output must be an array")
        ));

        let malformed_completed_after_item = "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",\"call_id\":\"valid-item\",\"name\":\"memory_recall\",\"arguments\":\"{}\"}}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"output\":[{\"type\":\"function_call\",\"call_id\":\"completed-bad\",\"name\":\"web_search\"}]}}\n\n";
        assert!(matches!(
            from_codex_responses_sse(malformed_completed_after_item),
            Err(KernelError::Model(message))
                if message.contains("web_search") && message.contains("missing string arguments")
        ));

        assert!(matches!(
            from_codex_responses_sse("data: {not-json\n\n"),
            Err(KernelError::Model(message)) if message.contains("SSE event has invalid JSON")
        ));

        let no_space_data = "data:{not-json}\n\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"safe text\"}\n\n";
        assert!(matches!(
            from_codex_responses_sse(no_space_data),
            Err(KernelError::Model(message)) if message.contains("SSE event has invalid JSON")
        ));
    }

    #[test]
    fn jwt_expiring_false_for_opaque() {
        assert!(!jwt_expiring("not-a-jwt", 120));
    }
}
