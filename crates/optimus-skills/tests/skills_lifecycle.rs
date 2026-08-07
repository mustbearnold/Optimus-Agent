//! Skills 2.0 lifecycle and permission tests.

use optimus_skills::{
    Outcome, Permission, PromotePolicy, SkillDraft, SkillError, SkillRegistry, SkillStatus,
};
use tempfile::tempdir;

fn open() -> (tempfile::TempDir, SkillRegistry) {
    let dir = tempdir().unwrap();
    let reg = SkillRegistry::open_with_policy(
        dir.path().join("skills.db"),
        PromotePolicy {
            min_uses: 3,
            min_success_rate: 0.8,
        },
    )
    .unwrap();
    (dir, reg)
}

#[test]
fn create_starts_as_candidate() {
    let (_d, reg) = open();
    let id = reg
        .create(SkillDraft {
            name: "win-lnk1104".into(),
            body: "set TEMP/TMP to Local\\Temp".into(),
            permissions: vec![Permission::Terminal, Permission::FsWorkspace],
            pin: false,
        })
        .unwrap();
    let s = reg.get(id).unwrap();
    assert_eq!(s.status, SkillStatus::Candidate);
    assert_eq!(s.version, 1);
    assert_eq!(s.permissions.len(), 2);
}

#[test]
fn promote_requires_min_uses_and_success_rate() {
    let (_d, reg) = open();
    let id = reg
        .create(SkillDraft {
            name: "build".into(),
            body: "cargo test".into(),
            permissions: vec![Permission::Terminal],
            pin: false,
        })
        .unwrap();

    assert!(matches!(
        reg.try_promote(id).unwrap_err(),
        SkillError::NotEligible(_)
    ));

    reg.record_outcome(
        id,
        Outcome {
            success: true,
            token_cost: 100,
        },
    )
    .unwrap();
    reg.record_outcome(
        id,
        Outcome {
            success: true,
            token_cost: 100,
        },
    )
    .unwrap();
    // only 2 uses
    assert!(matches!(
        reg.try_promote(id).unwrap_err(),
        SkillError::NotEligible(_)
    ));

    reg.record_outcome(
        id,
        Outcome {
            success: false,
            token_cost: 50,
        },
    )
    .unwrap();
    // 2/3 ~= 0.666 < 0.8
    assert!(matches!(
        reg.try_promote(id).unwrap_err(),
        SkillError::NotEligible(_)
    ));

    reg.record_outcome(
        id,
        Outcome {
            success: true,
            token_cost: 80,
        },
    )
    .unwrap();
    reg.record_outcome(
        id,
        Outcome {
            success: true,
            token_cost: 80,
        },
    )
    .unwrap();
    // 4/5 = 0.8
    let status = reg.try_promote(id).unwrap();
    assert_eq!(status, SkillStatus::Proven);
    assert_eq!(reg.get(id).unwrap().status, SkillStatus::Proven);
}

#[test]
fn authorize_rejects_undeclared_permissions() {
    let (_d, reg) = open();
    let id = reg
        .create(SkillDraft {
            name: "files-only".into(),
            body: "write file".into(),
            permissions: vec![Permission::FsWorkspace],
            pin: false,
        })
        .unwrap();

    reg.authorize(id, &[Permission::FsWorkspace]).unwrap();
    let err = reg
        .authorize(id, &[Permission::FsWorkspace, Permission::Net])
        .unwrap_err();
    match err {
        SkillError::PermissionDenied { missing } => {
            assert_eq!(missing, vec![Permission::Net]);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn update_cannot_expand_permissions() {
    let (_d, reg) = open();
    let id = reg
        .create(SkillDraft {
            name: "narrow".into(),
            body: "v1".into(),
            permissions: vec![Permission::FsWorkspace],
            pin: false,
        })
        .unwrap();

    let err = reg
        .update_body(
            id,
            Some("v2".into()),
            Some(vec![Permission::FsWorkspace, Permission::Terminal]),
        )
        .unwrap_err();
    assert!(matches!(err, SkillError::PermissionDenied { .. }));

    // shrinking is ok
    let s = reg
        .update_body(id, Some("v2".into()), Some(vec![]))
        .unwrap();
    assert!(s.permissions.is_empty());
    assert_eq!(s.body, "v2");
}

#[test]
fn resolve_prefers_pinned_then_proven() {
    let (_d, reg) = open();
    let c = reg
        .create(SkillDraft {
            name: "x".into(),
            body: "candidate".into(),
            permissions: vec![Permission::Terminal],
            pin: false,
        })
        .unwrap();
    // force proven via outcomes
    for _ in 0..3 {
        reg.record_outcome(
            c,
            Outcome {
                success: true,
                token_cost: 1,
            },
        )
        .unwrap();
    }
    reg.try_promote(c).unwrap();

    let p = reg
        .create(SkillDraft {
            name: "x".into(),
            body: "pinned".into(),
            permissions: vec![Permission::Terminal],
            pin: true,
        })
        .unwrap();

    let resolved = reg.resolve("x").unwrap().unwrap();
    assert_eq!(resolved.id, p);
    assert_eq!(resolved.status, SkillStatus::Pinned);
}

#[test]
fn update_body_rejects_empty_body() {
    let (_d, reg) = open();
    let id = reg
        .create(SkillDraft {
            name: "keep-body".into(),
            body: "v1".into(),
            permissions: vec![],
            pin: false,
        })
        .unwrap();

    // Same invariant as create(): whitespace-only bodies must be rejected.
    let err = reg.update_body(id, Some("   ".into()), None).unwrap_err();
    assert!(matches!(err, SkillError::Invariant(_)));

    // And a failed update must not mutate the stored skill.
    let s = reg.get(id).unwrap();
    assert_eq!(s.body, "v1");
    assert_eq!(s.version, 1);
}

#[test]
fn deprecated_excluded_from_default_list_and_resolve() {
    let (_d, reg) = open();
    let id = reg
        .create(SkillDraft {
            name: "old".into(),
            body: "gone".into(),
            permissions: vec![],
            pin: false,
        })
        .unwrap();
    reg.deprecate(id).unwrap();
    assert!(reg.list(false).unwrap().is_empty());
    assert!(reg.resolve("old").unwrap().is_none());
    assert_eq!(reg.list(true).unwrap().len(), 1);
}
