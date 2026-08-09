//! Stable identity types for the pack catalog: `PackId` and `ToolId`.
//!
//! Split out of `lib.rs` under the module-size ratchet (ADR-0049).

use serde::{Deserialize, Serialize};
use std::fmt;

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

    /// Whether the pack is loaded on demand rather than always-on in the waist.
    /// Every pack other than `Core` is an edge pack loaded via `activate_pack`.
    pub fn is_on_demand(self) -> bool {
        !self.is_core()
    }
}

impl fmt::Display for PackId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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

impl fmt::Display for ToolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ToolId {
    fn as_ref(&self) -> &str {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_id_parse_as_str_round_trip() {
        for id in [
            PackId::Core,
            PackId::Browser,
            PackId::Desktop,
            PackId::Media,
            PackId::Devex,
            PackId::Social,
            PackId::Collaboration,
            PackId::Children,
            PackId::Home,
            PackId::Office,
        ] {
            assert_eq!(PackId::parse(id.as_str()), Some(id));
            assert_eq!(PackId::parse(&id.to_string()), Some(id));
        }
        assert_eq!(PackId::parse("unknown"), None);
        assert_eq!(PackId::parse(""), None);
    }

    #[test]
    fn pack_id_display_matches_as_str() {
        assert_eq!(PackId::Core.to_string(), "core");
        assert_eq!(PackId::Browser.to_string(), "browser");
        assert_eq!(PackId::Home.to_string(), "home");
    }

    #[test]
    fn pack_id_is_core_and_is_on_demand_are_complementary() {
        assert!(PackId::Core.is_core());
        assert!(!PackId::Core.is_on_demand());

        for id in [
            PackId::Browser,
            PackId::Desktop,
            PackId::Media,
            PackId::Devex,
            PackId::Social,
            PackId::Collaboration,
            PackId::Children,
            PackId::Home,
            PackId::Office,
        ] {
            assert!(!id.is_core());
            assert!(id.is_on_demand());
        }
    }

    #[test]
    fn tool_id_display_matches_as_str() {
        let id = ToolId::new("terminal");
        assert_eq!(id.to_string(), "terminal");
        assert_eq!(ToolId::from("read_file").to_string(), "read_file");
    }

    #[test]
    fn tool_id_as_ref_str_borrows_backing_string() {
        let id = ToolId::new("search_content");
        let borrowed: &str = id.as_ref();
        assert_eq!(borrowed, "search_content");
    }
}
