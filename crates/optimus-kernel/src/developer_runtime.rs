use std::path::PathBuf;

use optimus_graph::CommandFsEnvelope;
use optimus_policy::{DeveloperAccessGrant, DeveloperScope};

use crate::{KernelConfig, ProductSettings};

pub(crate) struct Context {
    pub workspace: PathBuf,
    pub project_roots: Vec<PathBuf>,
    pub command_fs_envelope: CommandFsEnvelope,
    pub developer_access: Option<DeveloperAccessGrant>,
    pub developer_roots: Vec<PathBuf>,
}

pub(crate) fn resolve(
    mut workspace: PathBuf,
    mut project_roots: Vec<PathBuf>,
    config: &KernelConfig,
    settings: &ProductSettings,
) -> Context {
    let developer_access = config.developer_access.clone().or_else(|| {
        settings
            .developer_access
            .enabled
            .then_some(settings.developer_access.clone())
    });
    if let Some(grant) = developer_access
        .as_ref()
        .filter(|grant| grant.enabled && grant.capabilities.workspace_files)
    {
        project_roots = match &grant.scope {
            DeveloperScope::SelectedRepository { root, .. } => vec![PathBuf::from(root)],
            DeveloperScope::SelectedDirectories { roots } => {
                roots.iter().map(PathBuf::from).collect()
            }
            DeveloperScope::EntireLocalMachine => vec![machine_root()],
        };
        match &grant.scope {
            DeveloperScope::SelectedRepository { root, .. } => workspace = PathBuf::from(root),
            DeveloperScope::SelectedDirectories { roots } => {
                if let Some(root) = roots.first() {
                    // Relative file paths use the first selected root; other
                    // roots remain available through absolute paths.
                    workspace = PathBuf::from(root);
                }
            }
            DeveloperScope::EntireLocalMachine => {}
        }
    }
    let command_fs_envelope = match developer_access.as_ref() {
        Some(grant) if grant.enabled && grant.capabilities.terminal_execution => {
            match (&grant.scope, grant.capabilities.network_access) {
                (DeveloperScope::EntireLocalMachine, true) => CommandFsEnvelope::UnrestrictedHost,
                (DeveloperScope::EntireLocalMachine, false) => CommandFsEnvelope::ConfinedNoNetwork,
                (_, true) => CommandFsEnvelope::Confined,
                (_, false) => CommandFsEnvelope::ConfinedNoNetwork,
            }
        }
        _ => config
            .command_fs_envelope
            .unwrap_or_else(|| settings.work_isolation.command_fs_envelope()),
    };
    let developer_roots = developer_access
        .as_ref()
        .filter(|grant| grant.enabled && grant.capabilities.workspace_files)
        .map(|grant| match &grant.scope {
            DeveloperScope::EntireLocalMachine => vec![machine_root()],
            scope => scope.roots(),
        })
        .unwrap_or_default();
    Context {
        workspace,
        project_roots,
        command_fs_envelope,
        developer_access,
        developer_roots,
    }
}

fn machine_root() -> PathBuf {
    PathBuf::from(std::path::MAIN_SEPARATOR.to_string())
}
