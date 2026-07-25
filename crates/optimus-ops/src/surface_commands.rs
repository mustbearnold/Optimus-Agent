//! Unified slash-command registry shared by CLI and desktop (program P26).
//!
//! This is a **surface catalog**, not a second tool list. Runtime tools remain
//! `optimus-packs::ToolDesc` only.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandSurface {
    Cli,
    Desktop,
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceCommand {
    pub id: String,
    pub name: String,
    pub description: String,
    pub surface: CommandSurface,
    /// Optional pack id when the command surfaces pack tooling (not a tool grant).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack_id: Option<String>,
}

/// Canonical slash-command / palette registry. CLI and desktop must call this.
pub fn builtin_surface_commands() -> Vec<SurfaceCommand> {
    vec![
        cmd("help", "help", "Show available surface commands", CommandSurface::Both, None),
        cmd("doctor", "doctor", "Run doctor diagnostics", CommandSurface::Both, None),
        cmd("sessions", "sessions", "List durable sessions", CommandSurface::Both, None),
        cmd("new", "new", "Start a new session", CommandSurface::Desktop, None),
        cmd("skills", "skills", "Open skills console", CommandSurface::Desktop, None),
        cmd("memory", "memory", "Open memory explorer", CommandSurface::Desktop, None),
        cmd("packs", "packs", "Open packs console", CommandSurface::Desktop, None),
        cmd("logs", "logs", "Open redacted logs drawer", CommandSurface::Desktop, None),
        cmd("mail", "mail", "Open messaging inbox/outbox", CommandSurface::Desktop, None),
        cmd("cron", "cron", "Open schedules workbench", CommandSurface::Desktop, None),
        cmd("artifacts", "artifacts", "Open artifacts gallery", CommandSurface::Desktop, None),
        cmd("capabilities", "capabilities", "Open runtime capabilities", CommandSurface::Desktop, None),
        cmd(
            "packs.list",
            "packs list",
            "List capability packs (CLI)",
            CommandSurface::Cli,
            None,
        ),
        cmd(
            "skills.list",
            "skills list",
            "List procedural skills (CLI)",
            CommandSurface::Cli,
            None,
        ),
        cmd(
            "memory.recall",
            "memory recall",
            "Recall memory evidence (data only, never ActionAuthorize)",
            CommandSurface::Both,
            None,
        ),
    ]
}

fn cmd(
    id: &str,
    name: &str,
    description: &str,
    surface: CommandSurface,
    pack_id: Option<&str>,
) -> SurfaceCommand {
    SurfaceCommand {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        surface,
        pack_id: pack_id.map(|s| s.into()),
    }
}

pub fn commands_for_surface(surface: CommandSurface) -> Vec<SurfaceCommand> {
    builtin_surface_commands()
        .into_iter()
        .filter(|c| match (c.surface, surface) {
            (CommandSurface::Both, _) => true,
            (a, b) => a == b,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_nonempty_and_cli_desktop_share_help() {
        let all = builtin_surface_commands();
        assert!(all.len() >= 8);
        let ids: Vec<_> = all.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"help"));
        assert!(ids.contains(&"doctor"));
        let desktop = commands_for_surface(CommandSurface::Desktop);
        assert!(desktop.iter().any(|c| c.id == "skills"));
        assert!(desktop.iter().any(|c| c.id == "capabilities"));
        let cli = commands_for_surface(CommandSurface::Cli);
        assert!(cli.iter().any(|c| c.id == "packs.list"));
        // No tool ids disguised as commands for inventing a second tool catalog.
        assert!(!all.iter().any(|c| c.id.contains("browser_navigate")));
    }
}
