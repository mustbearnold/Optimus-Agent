//! Stable identity types for the pack catalog: `PackId` and `ToolId`.
//!
//! Split out of `lib.rs` under the module-size ratchet (ADR-0049).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackId {
    Core,
    Browser,
    Desktop,
    Media,
    Devex,
    Social,
    Collaboration,
    /// Recursive children: child kernel sessions (spec-034).
    Children,
    /// Home-automation / IoT breadth (Track Z.10).
    Home,
    /// Office docs breadth (Track Z.10).
    Office,
}

impl PackId {
    pub fn as_str(self) -> &'static str {
        match self {
            PackId::Core => "core",
            PackId::Browser => "browser",
            PackId::Desktop => "desktop",
            PackId::Media => "media",
            PackId::Devex => "devex",
            PackId::Social => "social",
            PackId::Collaboration => "collaboration",
            PackId::Children => "children",
            PackId::Home => "home",
            PackId::Office => "office",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "core" => Some(Self::Core),
            "browser" => Some(Self::Browser),
            "desktop" => Some(Self::Desktop),
            "media" => Some(Self::Media),
            "devex" => Some(Self::Devex),
            "social" => Some(Self::Social),
            "collaboration" => Some(Self::Collaboration),
            "children" => Some(Self::Children),
            "home" => Some(Self::Home),
            "office" => Some(Self::Office),
            _ => None,
        }
    }

    pub fn is_core(self) -> bool {
        matches!(self, PackId::Core)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct ToolId(String);

impl ToolId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ToolId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ToolId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}
