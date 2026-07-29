//! What a command is, as far as authority is concerned.
//!
//! Every `RunCommand` effect maps to `ProcessProjectExecute` today, which makes
//! `cargo test` and `cargo add some-crate` the same request. They are not.
//! One runs code already in the tree; the other reaches a public registry,
//! chooses a new dependency, and rewrites a lockfile. A broker that cannot see
//! the difference cannot decide differently, and the approval prompt cannot say
//! what it is approving.
//!
//! Three distinctions this draws, in rising order of consequence:
//!
//! 1. **Sync vs add.** `npm ci`, `cargo fetch`, `uv sync` reproduce a lockfile
//!    that is already in the repository — a human already chose those versions.
//!    `npm install lodash`, `cargo add serde` choose something new.
//!    `npm install` with no package named is a sync; with one, an add. Same
//!    verb, different act.
//! 2. **Project vs host.** `cargo install`, `npm install -g`, `pip install
//!    --user` write outside the project entirely. Classifying those as project
//!    execution is how a project-scoped grant quietly becomes a host-scoped
//!    one, so they map to `SystemModify` and leave the project lane.
//! 3. **Everything else** stays `ProcessProjectExecute`. Guessing wide is safe;
//!    guessing narrow is not.
//!
//! Pure and dependency-free: string in, capability out. The classifier makes
//! no network or filesystem calls, so it cannot be fooled by state.

use crate::CapabilityId;

/// What a command does, coarse enough to decide authority on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandClass {
    /// Reproduces an existing lockfile. No new dependency is chosen.
    PackageSync,
    /// Introduces or upgrades a dependency, reaching a package registry.
    PackageAdd,
    /// Installs outside the project — a host change wearing project clothes.
    HostInstall,
    /// Runs what is already there: build, test, lint, format, a script.
    ProjectExecute,
}

impl CommandClass {
    #[must_use]
    pub fn capability(self) -> CapabilityId {
        match self {
            Self::PackageSync => CapabilityId::PackageSync,
            Self::PackageAdd => CapabilityId::PackageAdd,
            Self::HostInstall => CapabilityId::SystemModify,
            Self::ProjectExecute => CapabilityId::ProcessProjectExecute,
        }
    }

    /// Whether the command reaches a package registry over the network.
    #[must_use]
    pub fn reaches_registry(self) -> bool {
        matches!(
            self,
            Self::PackageSync | Self::PackageAdd | Self::HostInstall
        )
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PackageSync => "package_sync",
            Self::PackageAdd => "package_add",
            Self::HostInstall => "host_install",
            Self::ProjectExecute => "project_execute",
        }
    }
}

/// Flags that turn a project-local install into a host-wide one.
const HOST_INSTALL_FLAGS: &[&str] = &["-g", "--global", "--user", "--system"];

/// Decide what a command is from its program and arguments.
///
/// `program` may be a path (`/usr/bin/cargo`); only the file name is
/// considered, because `./node_modules/.bin/npm` is still npm.
#[must_use]
pub fn classify_command(program: &str, args: &[String]) -> CommandClass {
    let tool = base_name(program);
    let positional: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|a| !a.starts_with('-'))
        .collect();
    let flags: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|a| a.starts_with('-'))
        .collect();
    let sub = positional.first().copied().unwrap_or("");
    let named_packages = positional.len() > 1;
    let global = flags.iter().any(|f| HOST_INSTALL_FLAGS.contains(f));

    match tool {
        "cargo" => match sub {
            // `cargo install` puts a binary on PATH, outside the project.
            "install" | "uninstall" => CommandClass::HostInstall,
            "add" | "remove" | "rm" | "update" | "upgrade" => CommandClass::PackageAdd,
            "fetch" | "vendor" | "generate-lockfile" => CommandClass::PackageSync,
            _ => CommandClass::ProjectExecute,
        },

        "npm" | "pnpm" | "yarn" | "bun" => match sub {
            "install" | "i" | "add" | "ci" | "update" | "upgrade" if global => {
                CommandClass::HostInstall
            }
            // `npm ci` is lockfile-exact by definition and takes no package.
            "ci" => CommandClass::PackageSync,
            // Bare `install` reproduces the lockfile; `install <pkg>` does not.
            "install" | "i" | "add" | "update" | "upgrade" => {
                if named_packages {
                    CommandClass::PackageAdd
                } else if flags
                    .iter()
                    .any(|f| matches!(*f, "--frozen-lockfile" | "--immutable" | "--no-save"))
                {
                    CommandClass::PackageSync
                } else if sub == "add" {
                    // `yarn add` with no package is an error, not a sync.
                    CommandClass::PackageAdd
                } else {
                    CommandClass::PackageSync
                }
            }
            "uninstall" | "remove" | "rm" => CommandClass::PackageAdd,
            _ => CommandClass::ProjectExecute,
        },

        "pip" | "pip3" => match sub {
            "install" | "uninstall" if global => CommandClass::HostInstall,
            // `-r requirements.txt` names a file already in the repository.
            "install" if requirements_only(args) => CommandClass::PackageSync,
            "install" | "uninstall" => CommandClass::PackageAdd,
            _ => CommandClass::ProjectExecute,
        },

        "uv" => match sub {
            "tool" => CommandClass::HostInstall,
            "sync" | "lock" => CommandClass::PackageSync,
            "add" | "remove" | "pip" => CommandClass::PackageAdd,
            _ => CommandClass::ProjectExecute,
        },

        "poetry" => match sub {
            "install" | "lock" => CommandClass::PackageSync,
            "add" | "remove" | "update" => CommandClass::PackageAdd,
            _ => CommandClass::ProjectExecute,
        },

        // System package managers are never project-scoped.
        "apt" | "apt-get" | "dnf" | "yum" | "pacman" | "brew" | "apk" | "snap" | "winget"
        | "choco" => CommandClass::HostInstall,

        _ => CommandClass::ProjectExecute,
    }
}

/// The capability a command should be authorized against.
#[must_use]
pub fn capability_for_command(program: &str, args: &[String]) -> CapabilityId {
    classify_command(program, args).capability()
}

/// `pip install -r requirements.txt` names a file, not a package.
fn requirements_only(args: &[String]) -> bool {
    let mut saw_requirement = false;
    let mut iter = args.iter().map(String::as_str).peekable();
    while let Some(arg) = iter.next() {
        match arg {
            "-r" | "--requirement" => {
                saw_requirement = true;
                iter.next();
            }
            "install" => {}
            other if other.starts_with('-') => {}
            // A bare positional after `install` is a package name.
            _ => return false,
        }
    }
    saw_requirement
}

fn base_name(program: &str) -> &str {
    program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(program)
        .trim_end_matches(".exe")
        .trim_end_matches(".cmd")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(program: &str, args: &[&str]) -> CommandClass {
        let args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
        classify_command(program, &args)
    }

    #[test]
    fn building_and_testing_stay_project_execution() {
        assert_eq!(class("cargo", &["build"]), CommandClass::ProjectExecute);
        assert_eq!(
            class("cargo", &["test", "--workspace"]),
            CommandClass::ProjectExecute
        );
        assert_eq!(class("just", &["verify"]), CommandClass::ProjectExecute);
        assert_eq!(
            class("npm", &["run", "build"]),
            CommandClass::ProjectExecute
        );
    }

    #[test]
    fn adding_a_dependency_is_not_the_same_act_as_testing() {
        assert_eq!(class("cargo", &["add", "serde"]), CommandClass::PackageAdd);
        assert_eq!(
            class("cargo", &["remove", "serde"]),
            CommandClass::PackageAdd
        );
        assert_ne!(
            class("cargo", &["add", "serde"]),
            class("cargo", &["test"]),
            "the whole point of the classifier"
        );
    }

    #[test]
    fn a_lockfile_reproduction_is_not_a_new_choice() {
        assert_eq!(class("npm", &["ci"]), CommandClass::PackageSync);
        assert_eq!(class("cargo", &["fetch"]), CommandClass::PackageSync);
        assert_eq!(class("uv", &["sync"]), CommandClass::PackageSync);
        assert_eq!(class("poetry", &["install"]), CommandClass::PackageSync);
        assert_eq!(
            class("pnpm", &["install", "--frozen-lockfile"]),
            CommandClass::PackageSync
        );
    }

    #[test]
    fn bare_install_syncs_but_naming_a_package_adds() {
        assert_eq!(class("npm", &["install"]), CommandClass::PackageSync);
        assert_eq!(
            class("npm", &["install", "lodash"]),
            CommandClass::PackageAdd
        );
        assert_eq!(
            class("yarn", &["add", "left-pad"]),
            CommandClass::PackageAdd
        );
    }

    #[test]
    fn requirements_files_are_a_sync_but_a_named_package_is_not() {
        assert_eq!(
            class("pip", &["install", "-r", "requirements.txt"]),
            CommandClass::PackageSync
        );
        assert_eq!(
            class("pip", &["install", "requests"]),
            CommandClass::PackageAdd
        );
    }

    #[test]
    fn installing_outside_the_project_leaves_the_project_lane() {
        // The escalation this exists to catch: today all four of these are
        // ProcessProjectExecute, so a project-scoped grant covers them.
        assert_eq!(
            class("cargo", &["install", "ripgrep"]),
            CommandClass::HostInstall
        );
        assert_eq!(
            class("npm", &["install", "-g", "tsx"]),
            CommandClass::HostInstall
        );
        assert_eq!(
            class("pip", &["install", "--user", "black"]),
            CommandClass::HostInstall
        );
        assert_eq!(
            class("apt-get", &["install", "curl"]),
            CommandClass::HostInstall
        );
        assert_eq!(
            CommandClass::HostInstall.capability(),
            CapabilityId::SystemModify,
            "a host install must not answer to a project capability"
        );
    }

    #[test]
    fn the_program_path_does_not_disguise_the_tool() {
        assert_eq!(
            class("/usr/local/bin/cargo", &["add", "serde"]),
            CommandClass::PackageAdd
        );
        assert_eq!(
            class("./node_modules/.bin/npm", &["install", "lodash"]),
            CommandClass::PackageAdd
        );
        assert_eq!(class("npm.cmd", &["ci"]), CommandClass::PackageSync);
    }

    #[test]
    fn an_unknown_program_is_guessed_wide_not_narrow() {
        assert_eq!(class("make", &["all"]), CommandClass::ProjectExecute);
        assert_eq!(class("./deploy.sh", &[]), CommandClass::ProjectExecute);
        assert_eq!(
            capability_for_command("whatever", &[]),
            CapabilityId::ProcessProjectExecute
        );
    }

    #[test]
    fn package_work_is_known_to_reach_the_network() {
        assert!(CommandClass::PackageAdd.reaches_registry());
        assert!(CommandClass::PackageSync.reaches_registry());
        assert!(CommandClass::HostInstall.reaches_registry());
        assert!(!CommandClass::ProjectExecute.reaches_registry());
    }
}
