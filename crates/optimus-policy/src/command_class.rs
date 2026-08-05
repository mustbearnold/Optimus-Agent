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
//! 3. **Project vs remote.** Direct remote clients and git remote operations
//!    cannot ride on project execution authority. They reuse the existing
//!    network, git, and external-send capabilities and carry remote-service
//!    externality.
//! 4. **Opaque shell wrappers.** `sh -c` (and equivalent wrappers) can conceal
//!    any host effect in one string. They fall back to `SystemModify` at the
//!    host boundary rather than inheriting project execution authority.
//! 5. **Everything else** keeps the legacy `ProcessProjectExecute`
//!    classification. That fallback is not proof that an arbitrary binary
//!    lacks remote effects; containment remains a separate authority layer.
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
    /// Pushes commits or refs to a git remote.
    GitRemotePush,
    /// Reads from a git remote or a public network endpoint.
    RemoteRead,
    /// Uses a remote shell or transfers data to or from another host.
    RemoteTransfer,
    /// Operates on GitHub through its remote API.
    GitHubRemote,
    /// Hides an arbitrary command string behind a shell interpreter.
    OpaqueShell,
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
            Self::GitRemotePush => CapabilityId::GitRemotePush,
            Self::RemoteRead => CapabilityId::NetworkPublicRead,
            Self::RemoteTransfer => CapabilityId::ExternalSend,
            Self::GitHubRemote => CapabilityId::GitRemotePullRequest,
            Self::OpaqueShell => CapabilityId::SystemModify,
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

    /// Where the command's effects occur.
    #[must_use]
    pub fn externality(self) -> crate::Externality {
        match self {
            Self::HostInstall | Self::OpaqueShell => crate::Externality::HostSystem,
            Self::GitRemotePush | Self::RemoteRead | Self::RemoteTransfer | Self::GitHubRemote => {
                crate::Externality::RemoteService
            }
            Self::PackageSync | Self::PackageAdd => crate::Externality::PublicNetwork,
            Self::ProjectExecute => crate::Externality::ProjectLocal,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PackageSync => "package_sync",
            Self::PackageAdd => "package_add",
            Self::HostInstall => "host_install",
            Self::GitRemotePush => "git_remote_push",
            Self::RemoteRead => "remote_read",
            Self::RemoteTransfer => "remote_transfer",
            Self::GitHubRemote => "github_remote",
            Self::OpaqueShell => "opaque_shell",
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
    let tool = normalized_tool_name(program);
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

    match tool.as_str() {
        "git" if git_sets_inline_alias(args) => CommandClass::OpaqueShell,
        "git" => match git_subcommand(args) {
            "push" | "send-pack" | "http-push" => CommandClass::GitRemotePush,
            "fetch" | "pull" | "clone" | "ls-remote" | "fetch-pack" | "http-fetch"
            | "remote-http" | "remote-https" | "remote-ftp" | "remote-ftps" => {
                CommandClass::RemoteRead
            }
            _ => CommandClass::ProjectExecute,
        },

        "curl" | "wget" => CommandClass::RemoteRead,
        "ssh" | "scp" => CommandClass::RemoteTransfer,
        "rsync" if args.iter().any(|arg| is_rsync_remote_endpoint(arg)) => {
            CommandClass::RemoteTransfer
        }
        "gh" => CommandClass::GitHubRemote,

        "sh" | "bash" | "dash" | "zsh" | "ksh"
            if flags.iter().any(|flag| is_shell_command_flag(flag)) =>
        {
            CommandClass::OpaqueShell
        }
        "fish"
            if flags
                .iter()
                .any(|flag| is_shell_command_flag(flag) || is_fish_init_command_flag(flag)) =>
        {
            CommandClass::OpaqueShell
        }
        "cmd" if args.iter().any(|arg| is_cmd_command_flag(arg)) => CommandClass::OpaqueShell,
        "powershell" | "pwsh" if args.iter().any(|arg| is_powershell_command_flag(arg)) => {
            CommandClass::OpaqueShell
        }

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
            // `uv pip ...` subcommands draw the same sync/add split pip draws.
            "pip" => uv_pip_class(args),
            "add" | "remove" => CommandClass::PackageAdd,
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
    requirements_only_iter(args.iter().map(String::as_str))
}

fn requirements_only_iter<'a>(mut iter: impl Iterator<Item = &'a str>) -> bool {
    let mut saw_requirement = false;
    while let Some(arg) = iter.next() {
        match arg {
            "-r" | "--requirement" => {
                saw_requirement = true;
                iter.next();
            }
            "install" => {}
            // Attached forms: `-rrequirements.txt` and
            // `--requirement=requirements.txt` also name a file.
            other if is_attached_requirement_flag(other) => {
                saw_requirement = true;
            }
            other if other.starts_with('-') => {}
            // A bare positional after `install` is a package name.
            _ => return false,
        }
    }
    saw_requirement
}

/// True for the attached-value forms of the requirement flag — e.g.
/// `-rrequirements.txt` or `--requirement=requirements.txt` — which pip and uv
/// accept exactly like `-r requirements.txt`.
fn is_attached_requirement_flag(arg: &str) -> bool {
    arg.starts_with("--requirement=")
        || arg
            .strip_prefix("-r")
            .is_some_and(|rest| !rest.is_empty() && !rest.starts_with('-'))
}

/// Classify the `uv pip` subcommand family with the same sync/add split pip
/// draws. The `pip` subcommand is itself a bare positional, so it (and any
/// leading global flags) is skipped first; anything unrecognized falls back to
/// `PackageAdd`, never a mis-grant.
fn uv_pip_class(args: &[String]) -> CommandClass {
    let mut iter = args
        .iter()
        .map(String::as_str)
        .skip_while(|arg| arg.starts_with('-'));
    if iter.next() != Some("pip") {
        return CommandClass::PackageAdd;
    }
    match iter.next() {
        // `uv pip sync` reproduces an environment from an existing file.
        Some("sync") => CommandClass::PackageSync,
        // `uv pip install -r requirements.txt` names a file already in the
        // repository, not a new package choice.
        Some("install") if requirements_only_iter(iter) => CommandClass::PackageSync,
        _ => CommandClass::PackageAdd,
    }
}

/// Find a git subcommand after global options such as `-C <path>`.
fn git_subcommand(args: &[String]) -> &str {
    let mut iter = args.iter().map(String::as_str);
    while let Some(arg) = iter.next() {
        if matches!(
            arg,
            "-C" | "-c"
                | "--git-dir"
                | "--work-tree"
                | "--namespace"
                | "--super-prefix"
                | "--exec-path"
        ) {
            iter.next();
        } else if !arg.starts_with('-') {
            return arg;
        }
    }
    ""
}

fn git_sets_inline_alias(args: &[String]) -> bool {
    args.windows(2).any(|pair| {
        pair[0] == "-c"
            && pair[1]
                .get(.."alias.".len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("alias."))
    })
}

fn is_shell_command_flag(flag: &str) -> bool {
    flag == "--command"
        || flag.starts_with("--command=")
        || flag
            .strip_prefix('-')
            .is_some_and(|short_flags| !short_flags.starts_with('-') && short_flags.contains('c'))
}

fn is_fish_init_command_flag(flag: &str) -> bool {
    flag == "-C" || flag == "--init-command" || flag.starts_with("--init-command=")
}

fn is_cmd_command_flag(flag: &str) -> bool {
    flag.eq_ignore_ascii_case("/c") || flag.eq_ignore_ascii_case("/k")
}

fn is_powershell_command_flag(flag: &str) -> bool {
    let flag = flag.to_ascii_lowercase();
    matches!(flag.as_str(), "-c" | "-command")
        || (flag.len() >= 2 && "-encodedcommand".starts_with(flag.as_str()))
}

/// Rsync treats `host:path`, `host::module`, and `rsync://host/module` as
/// remote endpoints. Plain project paths remain local project execution.
fn is_rsync_remote_endpoint(arg: &str) -> bool {
    if arg.starts_with('-') {
        return false;
    }
    if arg
        .get(.."rsync://".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("rsync://"))
    {
        return true;
    }

    let Some((host, path)) = arg.split_once(':') else {
        return false;
    };
    if host.is_empty() || host.contains(['/', '\\']) {
        return false;
    }

    // A Windows drive path is local even though it contains a colon.
    !(host.len() == 1
        && host.as_bytes()[0].is_ascii_alphabetic()
        && (path.starts_with('/') || path.starts_with('\\')))
}

fn normalized_tool_name(program: &str) -> String {
    program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(program)
        .to_ascii_lowercase()
        .trim_end_matches(".exe")
        .trim_end_matches(".cmd")
        .to_string()
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
    fn attached_requirement_flag_forms_are_still_a_sync() {
        // Regression: pip and uv accept `-rrequirements.txt` and
        // `--requirement=requirements.txt` (attached value) exactly like the
        // separate-value form. They name a file already in the repository, so
        // they must not be recorded as a new dependency choice.
        assert_eq!(
            class("pip", &["install", "-rrequirements.txt"]),
            CommandClass::PackageSync
        );
        assert_eq!(
            class("pip", &["install", "--requirement=requirements.txt"]),
            CommandClass::PackageSync
        );
        assert_eq!(
            class("uv", &["pip", "install", "-rrequirements.txt"]),
            CommandClass::PackageSync
        );
        assert_eq!(
            class("uv", &["pip", "install", "--requirement=requirements.txt"]),
            CommandClass::PackageSync
        );
        // Unrelated flags that merely contain `-r` must not be confused with
        // the requirement flag.
        assert_eq!(
            class("pip", &["install", "--root=/tmp/sandbox", "requests"]),
            CommandClass::PackageAdd
        );
        assert_eq!(
            class(
                "pip",
                &["install", "--require-hashes", "-r", "requirements.txt"]
            ),
            CommandClass::PackageSync
        );
    }

    #[test]
    fn uv_pip_requirements_files_are_a_sync_but_a_named_package_is_not() {
        // `uv pip install -r` is the same act as `pip install -r`: it names a
        // file already in the repository, so it must not be recorded as a new
        // dependency choice.
        assert_eq!(
            class("uv", &["pip", "install", "-r", "requirements.txt"]),
            CommandClass::PackageSync
        );
        assert_eq!(
            class(
                "uv",
                &["pip", "install", "--requirement", "requirements.txt"]
            ),
            CommandClass::PackageSync
        );
        assert_eq!(
            class(
                "uv",
                &["--quiet", "pip", "install", "-r", "requirements.txt"]
            ),
            CommandClass::PackageSync,
            "a global flag before the pip subcommand must not change the act"
        );
        assert_eq!(
            class("uv", &["pip", "install", "requests"]),
            CommandClass::PackageAdd
        );
        assert_eq!(
            class("uv", &["pip", "install"]),
            CommandClass::PackageAdd,
            "a bare uv pip install chooses nothing new either way"
        );
        assert_eq!(
            class(
                "uv",
                &["pip", "install", "requests", "-r", "requirements.txt"]
            ),
            CommandClass::PackageAdd,
            "a named package alongside the file is still an add"
        );
        assert_eq!(
            class("uv", &["pip", "sync", "requirements.txt"]),
            CommandClass::PackageSync
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
    fn direct_remote_commands_leave_the_project_lane() {
        assert_eq!(class("git", &["push"]), CommandClass::GitRemotePush);
        assert_eq!(class("git", &["fetch", "origin"]), CommandClass::RemoteRead);
        assert_eq!(
            class("git", &["-C", "project", "fetch", "origin"]),
            CommandClass::RemoteRead
        );
        for subcommand in ["send-pack", "http-push"] {
            assert_eq!(
                class("git", &[subcommand, "origin"]),
                CommandClass::GitRemotePush,
                "{subcommand}"
            );
        }
        for subcommand in ["fetch-pack", "http-fetch", "remote-http", "remote-https"] {
            assert_eq!(
                class("git", &[subcommand, "origin"]),
                CommandClass::RemoteRead,
                "{subcommand}"
            );
        }
        for tool in ["curl", "wget"] {
            assert_eq!(
                class(tool, &["https://example.test"]),
                CommandClass::RemoteRead
            );
        }
        for tool in ["ssh", "scp"] {
            assert_eq!(class(tool, &["host.example"]), CommandClass::RemoteTransfer);
        }
        assert_eq!(
            class("rsync", &["src/", "host.example:path"]),
            CommandClass::RemoteTransfer
        );
        assert_eq!(
            class("rsync", &["src/", "rsync://host.example/module"]),
            CommandClass::RemoteTransfer
        );
        assert_eq!(
            class("rsync", &["-a", "src/", "target/"]),
            CommandClass::ProjectExecute
        );
        assert_eq!(
            class("rsync", &["C:\\source", "D:\\target"]),
            CommandClass::ProjectExecute
        );
        assert_eq!(class("gh", &["pr", "create"]), CommandClass::GitHubRemote);
        assert_eq!(
            CommandClass::GitRemotePush.externality(),
            crate::Externality::RemoteService
        );
    }

    #[test]
    fn command_string_shell_wrappers_are_opaque_host_commands() {
        assert_eq!(
            class("sh", &["-c", "cargo test"]),
            CommandClass::OpaqueShell
        );
        assert_eq!(
            class("bash", &["--command", "cargo test"]),
            CommandClass::OpaqueShell
        );
        assert_eq!(
            class("bash", &["-lc", "cargo test"]),
            CommandClass::OpaqueShell
        );
        for flag in [
            "-C",
            "--command=curl https://example.test",
            "--init-command",
            "--init-command=curl https://example.test",
        ] {
            assert_eq!(
                class("fish", &[flag, "curl https://example.test"]),
                CommandClass::OpaqueShell,
                "fish {flag}"
            );
        }
        for flag in ["/C", "/c", "/K", "/k"] {
            assert_eq!(
                class("cmd", &[flag, "cargo test"]),
                CommandClass::OpaqueShell,
                "cmd {flag}"
            );
        }
        assert_eq!(
            CommandClass::OpaqueShell.capability(),
            CapabilityId::SystemModify
        );
        assert_eq!(
            CommandClass::OpaqueShell.externality(),
            crate::Externality::HostSystem
        );
        assert_eq!(
            class("sh", &["scripts/check.sh"]),
            CommandClass::ProjectExecute,
            "a visible project script is not an opaque command string"
        );
        assert_eq!(
            class(
                "git",
                &["-c", "alias.ship=!curl https://example.test", "ship"]
            ),
            CommandClass::OpaqueShell,
            "an inline git alias can conceal arbitrary shell execution"
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
        assert_eq!(
            class("C:\\Program Files\\Git\\GIT.EXE", &["push"]),
            CommandClass::GitRemotePush
        );
        assert_eq!(
            class("CURL.EXE", &["https://example.test"]),
            CommandClass::RemoteRead
        );
        assert_eq!(
            class("CMD.EXE", &["/C", "cargo test"]),
            CommandClass::OpaqueShell
        );
        for flag in ["-c", "-enc", "-Command", "-EncodedCommand"] {
            assert_eq!(
                class("PowerShell.EXE", &[flag, "cargo test"]),
                CommandClass::OpaqueShell,
                "{flag}"
            );
        }
        for flag in ["-e", "-en", "-enco", "-encoded"] {
            assert_eq!(
                class("PowerShell.EXE", &[flag, "Y2FyZ28gdGVzdA=="]),
                CommandClass::OpaqueShell,
                "{flag}"
            );
        }
        assert_eq!(
            class("PowerShell.EXE", &["-File", "scripts/check.ps1"]),
            CommandClass::ProjectExecute
        );
        assert_eq!(
            class("fish", &["scripts/check.fish"]),
            CommandClass::ProjectExecute
        );
    }

    #[test]
    fn an_unknown_program_keeps_the_legacy_project_execution_class() {
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
        assert!(!CommandClass::RemoteRead.reaches_registry());
        assert!(!CommandClass::ProjectExecute.reaches_registry());
    }
}
