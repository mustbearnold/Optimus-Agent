//! How a tool is invoked, and the operational contract that follows from it.
//!
//! Split from `lib.rs` under the module-size law. It is a natural seam: every
//! function here is a total match over `ToolInvocation`, so a new tool is added
//! by extending this file and the catalog, and the compiler names each arm that
//! still needs an answer.

use serde::{Deserialize, Serialize};

use crate::{ReplayClass, ToolPolicy};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ToolInvocation {
    ReadFile,
    SearchContent,
    FindFiles,
    ListDir,
    WriteFile,
    Mkdir,
    DeletePath,
    RenamePath,
    PatchFile,
    Terminal,
    WebSearch,
    MemoryRecall,
    SkillResolve,
    ActivatePack,
    BrowserNavigate,
    BrowserSnapshot,
    BrowserClick,
    VisionAnalyze,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolRetry {
    Never,
    Bounded { max_attempts: u8 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolIdempotency {
    None,
    Keyed,
    Convergent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolTimeout {
    CallerBounded,
    FixedMillis(u64),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCancellation {
    Unsupported,
    Cooperative,
    Terminal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolObservability {
    pub call_identity: bool,
    pub trace_span: bool,
    pub effect_provenance: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolOperations {
    pub retry: ToolRetry,
    pub idempotency: ToolIdempotency,
    pub timeout: ToolTimeout,
    pub cancellation: ToolCancellation,
    pub observability: ToolObservability,
}

impl ToolInvocation {
    /// Every invocation the kernel may dispatch (excludes placeholder `Unavailable`).
    ///
    /// Catalog construction and domain-modularity tests must stay closed over this
    /// set: advertised available tools ≡ handlers, no orphan dispatch arms.
    pub const ALL_DISPATCHABLE: &'static [ToolInvocation] = &[
        Self::ReadFile,
        Self::SearchContent,
        Self::FindFiles,
        Self::ListDir,
        Self::WriteFile,
        Self::Mkdir,
        Self::DeletePath,
        Self::RenamePath,
        Self::PatchFile,
        Self::Terminal,
        Self::WebSearch,
        Self::MemoryRecall,
        Self::SkillResolve,
        Self::ActivatePack,
        Self::BrowserNavigate,
        Self::BrowserSnapshot,
        Self::BrowserClick,
        Self::VisionAnalyze,
    ];

    pub fn canonical_id(self) -> Option<&'static str> {
        self.id()
    }

    pub(crate) fn id(self) -> Option<&'static str> {
        match self {
            Self::ReadFile => Some("read_file"),
            Self::SearchContent => Some("search_content"),
            Self::FindFiles => Some("find_files"),
            Self::ListDir => Some("list_dir"),
            Self::WriteFile => Some("write_file"),
            Self::Mkdir => Some("mkdir"),
            Self::DeletePath => Some("delete_path"),
            Self::RenamePath => Some("rename_path"),
            Self::PatchFile => Some("patch_file"),
            Self::Terminal => Some("terminal"),
            Self::WebSearch => Some("web_search"),
            Self::MemoryRecall => Some("memory_recall"),
            Self::SkillResolve => Some("skill_resolve"),
            Self::ActivatePack => Some("activate_pack"),
            Self::BrowserNavigate => Some("browser_navigate"),
            Self::BrowserSnapshot => Some("browser_snapshot"),
            Self::BrowserClick => Some("browser_click"),
            Self::VisionAnalyze => Some("vision_analyze"),
            Self::Unavailable => None,
        }
    }

    pub(crate) fn policy(self) -> Option<ToolPolicy> {
        match self {
            Self::ReadFile => Some(ToolPolicy::WorkspaceRead),
            // Reads, not processes. Shelling out to `rg` would make every
            // lookup a Process-policy call and so an approval prompt; these
            // read the workspace exactly as `read_file` does.
            Self::SearchContent => Some(ToolPolicy::WorkspaceRead),
            Self::FindFiles => Some(ToolPolicy::WorkspaceRead),
            Self::ListDir => Some(ToolPolicy::WorkspaceRead),
            // One arm per variant so Engineering Memory can parse ToolPolicy maps.
            Self::WriteFile => Some(ToolPolicy::WorkspaceWrite),
            Self::Mkdir => Some(ToolPolicy::WorkspaceWrite),
            Self::DeletePath => Some(ToolPolicy::WorkspaceWrite),
            Self::RenamePath => Some(ToolPolicy::WorkspaceWrite),
            Self::PatchFile => Some(ToolPolicy::WorkspaceWrite),
            Self::Terminal => Some(ToolPolicy::Process),
            Self::WebSearch => Some(ToolPolicy::NetworkRead),
            Self::MemoryRecall => Some(ToolPolicy::MemoryRead),
            Self::SkillResolve => Some(ToolPolicy::SkillRead),
            Self::ActivatePack => Some(ToolPolicy::Capability),
            Self::BrowserNavigate => Some(ToolPolicy::Browser),
            Self::BrowserSnapshot => Some(ToolPolicy::Browser),
            Self::BrowserClick => Some(ToolPolicy::Browser),
            Self::VisionAnalyze => Some(ToolPolicy::Media),
            Self::Unavailable => None,
        }
    }

    pub(crate) fn replay(self) -> ReplayClass {
        match self {
            Self::ReadFile
            | Self::SearchContent
            | Self::FindFiles
            | Self::ListDir
            | Self::MemoryRecall
            | Self::SkillResolve
            | Self::ActivatePack
            | Self::BrowserSnapshot
            | Self::Unavailable => ReplayClass::Deterministic,
            Self::WriteFile
            | Self::Mkdir
            | Self::DeletePath
            | Self::RenamePath
            | Self::PatchFile => ReplayClass::Convergent,
            Self::Terminal | Self::WebSearch | Self::BrowserNavigate | Self::BrowserClick => {
                ReplayClass::ExternalNondeterministic
            }
            // A vision sub-call is a model completion: same inputs may phrase
            // the analysis differently, and no external state is mutated.
            Self::VisionAnalyze => ReplayClass::ModelNondeterministic,
        }
    }

    pub(crate) fn operations(self) -> ToolOperations {
        let replay = self.replay();
        ToolOperations {
            retry: ToolRetry::Never,
            idempotency: match replay {
                ReplayClass::Deterministic | ReplayClass::FixtureReplayable => {
                    ToolIdempotency::Keyed
                }
                ReplayClass::Convergent => ToolIdempotency::Convergent,
                _ => ToolIdempotency::None,
            },
            timeout: ToolTimeout::CallerBounded,
            cancellation: match self {
                Self::WriteFile
                | Self::Mkdir
                | Self::DeletePath
                | Self::RenamePath
                | Self::PatchFile
                | Self::Terminal => ToolCancellation::Terminal,
                _ => ToolCancellation::Unsupported,
            },
            observability: ToolObservability {
                call_identity: true,
                trace_span: true,
                effect_provenance: matches!(
                    self,
                    Self::WriteFile
                        | Self::Mkdir
                        | Self::DeletePath
                        | Self::RenamePath
                        | Self::PatchFile
                        | Self::Terminal
                ),
            },
        }
    }
}
