//! Small argument parsers shared by CLI subcommands.
//!
//! Split out of `main.rs`, the largest entry in
//! `docs/architecture/module-size-baseline.json`, which may only shrink.

use optimus_runtime::PolicyMode;
use optimus_skills::Permission;

pub fn parse_policy_mode(policy: &str) -> Result<PolicyMode, Box<dyn std::error::Error>> {
    match policy.to_ascii_lowercase().as_str() {
        "smart_deny" | "smartdeny" | "deny" => Ok(PolicyMode::SmartDeny),
        // `open` used to mean this and no longer does (#118). The word that
        // turns off every effect check has to sound like it: the error below
        // never advertised `open`, so nothing documented is losing a spelling.
        "unrestricted" => Ok(PolicyMode::Unrestricted),
        other => Err(format!("unknown policy {other}; use smart_deny or unrestricted").into()),
    }
}

pub fn parse_perms(s: &str) -> Result<Vec<Permission>, String> {
    let mut out = Vec::new();
    for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        out.push(match part {
            "fs" | "fs_workspace" => Permission::FsWorkspace,
            "terminal" => Permission::Terminal,
            "net" => Permission::Net,
            "browser" => Permission::Browser,
            "memory_write" => Permission::MemoryWrite,
            other => return Err(format!("unknown permission: {other}")),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_unrestricted_spells_unrestricted() {
        assert!(matches!(
            parse_policy_mode("unrestricted"),
            Ok(PolicyMode::Unrestricted)
        ));
        // Words that read as ordinary product language must not turn off
        // every effect check (#118). They are errors, not aliases.
        for raw in ["open", "full", "all", "yes", "on", "auto"] {
            assert!(parse_policy_mode(raw).is_err(), "{raw} must not parse");
        }
        for raw in ["smart_deny", "smartdeny", "deny", "SMART_DENY"] {
            assert!(
                matches!(parse_policy_mode(raw), Ok(PolicyMode::SmartDeny)),
                "{raw}"
            );
        }
    }
}
