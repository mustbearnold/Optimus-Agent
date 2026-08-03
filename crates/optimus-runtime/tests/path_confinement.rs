//! Security regressions for runtime workspace confinement.

use std::fs;
use std::path::Path;

use optimus_graph::{Effect, JobSpec, NodeSpec};
use optimus_policy::{DeveloperAccessGrant, DeveloperScope, DEVELOPER_ACCESS_CONFIRMATION_VERSION};
use optimus_runtime::{ApprovalGrant, Runtime, RuntimeError};
use tempfile::tempdir;

fn grant_if_needed(rt: &Runtime, job: optimus_graph::JobId) {
    if let Err(RuntimeError::NeedsApproval { .. }) = rt.run_next(job) {
        rt.grant_approval(ApprovalGrant::for_job(job))
            .expect("grant");
        rt.run_next(job).expect("run after grant");
    }
}

fn create_single_effect_job(rt: &Runtime, label: &str, effect: Effect) -> optimus_graph::JobId {
    rt.create_job(JobSpec {
        label: label.into(),
        budget: Default::default(),
        nodes: vec![NodeSpec {
            label: label.into(),
            effect,
        }],
    })
    .expect("create job")
}

#[cfg(unix)]
fn link_directory(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).expect("create directory symlink");
}

#[cfg(windows)]
fn link_directory(target: &Path, link: &Path) {
    if std::os::windows::fs::symlink_dir(target, link).is_ok() {
        return;
    }

    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .status()
        .expect("run mklink junction fallback");
    assert!(status.success(), "create directory symlink or junction");
}

#[test]
fn write_rejects_missing_parent_below_linked_ancestor() {
    let root = tempdir().expect("root tempdir");
    let workspace = root.path().join("workspace");
    let external = root.path().join("external");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&external).expect("external");
    link_directory(&external, &workspace.join("linked"));

    let rt = Runtime::open(&root.path().join("optimus.db"), &workspace).expect("runtime");
    let job = create_single_effect_job(
        &rt,
        "reject-linked-ancestor",
        Effect::WriteFile {
            relative_path: "linked/new/escaped.txt".into(),
            contents: "must not escape".into(),
        },
    );

    // Illegal path shape is not always preflightable (symlink escape is execute-time).
    // Grant, then expect PathEscape on execution.
    let first = rt.run_next(job);
    assert!(
        matches!(first, Err(RuntimeError::NeedsApproval { .. })),
        "write is high-risk under SmartDeny: {first:?}"
    );
    rt.grant_approval(ApprovalGrant::for_job(job))
        .expect("grant");
    let error = rt
        .run_next(job)
        .expect_err("linked ancestor must be denied");
    assert!(matches!(error, RuntimeError::PathEscape(_)), "{error:?}");
    assert!(
        !external.join("new").exists(),
        "confinement failure must not create directories outside the workspace"
    );
}

#[test]
fn nested_write_remains_available_inside_workspace() {
    let root = tempdir().expect("root tempdir");
    let workspace = root.path().join("workspace");
    let rt = Runtime::open(&root.path().join("optimus.db"), &workspace).expect("runtime");
    let job = create_single_effect_job(
        &rt,
        "nested-write",
        Effect::WriteFile {
            relative_path: "nested/inside.txt".into(),
            contents: "inside".into(),
        },
    );

    grant_if_needed(&rt, job);
    assert_eq!(
        fs::read_to_string(workspace.join("nested/inside.txt")).unwrap(),
        "inside"
    );
}

#[test]
fn developer_scope_allows_direct_mutation_in_a_selected_secondary_root() {
    let root = tempdir().expect("root tempdir");
    let workspace = root.path().join("workspace");
    let secondary = root.path().join("secondary");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&secondary).expect("secondary");
    let grant = DeveloperAccessGrant {
        enabled: true,
        confirmation_version: DEVELOPER_ACCESS_CONFIRMATION_VERSION,
        issued_unix: 1,
        scope: DeveloperScope::SelectedDirectories {
            roots: vec![
                workspace.display().to_string(),
                secondary.display().to_string(),
            ],
        },
        pause_before_destructive: false,
        ..Default::default()
    };
    let rt = Runtime::open_with_developer_access(
        &root.path().join("optimus.db"),
        &workspace,
        optimus_graph::RuntimeConfig {
            autonomy_profile: optimus_graph::AutonomyProfile::DeveloperFullAccess,
            ..Default::default()
        },
        Some(grant),
        vec![workspace.clone(), secondary.clone()],
    )
    .expect("developer runtime");
    let target = secondary.join("direct.txt");
    let job = create_single_effect_job(
        &rt,
        "developer-secondary-write",
        Effect::ProjectWriteFile {
            workspace_sha256: rt.workspace_sha256(),
            relative_path: target.display().to_string(),
            contents: "developer scope".into(),
        },
    );

    rt.run_next(job)
        .expect("developer grant should allow the exact write");
    assert_eq!(fs::read_to_string(target).unwrap(), "developer scope");
}

#[test]
fn assert_file_rejects_linked_ancestor() {
    let root = tempdir().expect("root tempdir");
    let workspace = root.path().join("workspace");
    let external = root.path().join("external");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&external).expect("external");
    fs::write(external.join("secret.txt"), "outside").expect("external fixture");
    link_directory(&external, &workspace.join("linked"));

    let rt = Runtime::open(&root.path().join("optimus.db"), &workspace).expect("runtime");
    let job = create_single_effect_job(
        &rt,
        "reject-linked-read",
        Effect::AssertFileEquals {
            relative_path: "linked/secret.txt".into(),
            expected: "outside".into(),
        },
    );

    let error = rt.run_next(job).expect_err("linked read must be denied");
    assert!(matches!(error, RuntimeError::PathEscape(_)), "{error:?}");
}

#[test]
fn parent_traversal_is_rejected() {
    let root = tempdir().expect("root tempdir");
    let workspace = root.path().join("workspace");
    let rt = Runtime::open(&root.path().join("optimus.db"), &workspace).expect("runtime");
    let job = create_single_effect_job(
        &rt,
        "reject-parent",
        Effect::WriteFile {
            relative_path: "../escaped.txt".into(),
            contents: "must not escape".into(),
        },
    );

    let error = rt
        .run_next(job)
        .expect_err("parent traversal must be denied");
    assert!(matches!(error, RuntimeError::PathEscape(_)), "{error:?}");
    assert!(!root.path().join("escaped.txt").exists());
}

#[test]
fn leading_current_directory_is_rejected_for_write_and_assert() {
    let root = tempdir().expect("root tempdir");
    let workspace = root.path().join("workspace");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::write(workspace.join("existing.txt"), "inside").expect("fixture");
    let rt = Runtime::open(&root.path().join("optimus.db"), &workspace).expect("runtime");

    let write = create_single_effect_job(
        &rt,
        "reject-current-write",
        Effect::WriteFile {
            relative_path: "./nested/file.txt".into(),
            contents: "must be rejected".into(),
        },
    );
    let write_error = rt
        .run_next(write)
        .expect_err("leading current directory write must be denied");
    assert!(
        matches!(write_error, RuntimeError::PathEscape(_)),
        "{write_error:?}"
    );
    assert!(!workspace.join("nested/file.txt").exists());

    let assertion = create_single_effect_job(
        &rt,
        "reject-current-assert",
        Effect::AssertFileEquals {
            relative_path: "./existing.txt".into(),
            expected: "inside".into(),
        },
    );
    let assert_error = rt
        .run_next(assertion)
        .expect_err("leading current directory assertion must be denied");
    assert!(
        matches!(assert_error, RuntimeError::PathEscape(_)),
        "{assert_error:?}"
    );
}

#[test]
fn secret_basenames_are_rejected_for_write_and_assert() {
    let root = tempdir().expect("root tempdir");
    let workspace = root.path().join("workspace");
    fs::create_dir_all(workspace.join("nested")).expect("workspace");
    fs::write(workspace.join(".env"), "fixture only").expect("secret fixture");
    let rt = Runtime::open(&root.path().join("optimus.db"), &workspace).expect("runtime");

    for (index, relative_path) in [
        ".env",
        "nested/Auth.JSON",
        "nested/key.pem",
        "id_rsa",
        ".netrc",
    ]
    .into_iter()
    .enumerate()
    {
        let job = create_single_effect_job(
            &rt,
            &format!("reject-secret-{index}"),
            Effect::WriteFile {
                relative_path: relative_path.into(),
                contents: "must not be written".into(),
            },
        );
        let error = rt
            .run_next(job)
            .expect_err("secret basename write must be denied");
        assert!(
            matches!(error, RuntimeError::PathEscape(_)),
            "{relative_path}: {error:?}"
        );
        if relative_path != ".env" {
            assert!(!workspace.join(relative_path).exists(), "{relative_path}");
        }
    }

    let assertion = create_single_effect_job(
        &rt,
        "reject-secret-assert",
        Effect::AssertFileEquals {
            relative_path: ".env".into(),
            expected: "fixture only".into(),
        },
    );
    let error = rt
        .run_next(assertion)
        .expect_err("secret basename assertion must be denied");
    assert!(matches!(error, RuntimeError::PathEscape(_)), "{error:?}");
}

#[test]
fn workspace_root_replacement_cannot_redirect_effects() {
    let root = tempdir().expect("root tempdir");
    let workspace = root.path().join("workspace");
    let original = root.path().join("workspace-original");
    fs::create_dir_all(&workspace).expect("workspace");
    let rt = Runtime::open(&root.path().join("optimus.db"), &workspace).expect("runtime");

    let replacement_succeeded = fs::rename(&workspace, &original).is_ok();
    if replacement_succeeded {
        fs::create_dir_all(&workspace).expect("replacement workspace");
    }

    let job = create_single_effect_job(
        &rt,
        "root-replacement",
        Effect::WriteFile {
            relative_path: "pinned.txt".into(),
            contents: "original capability".into(),
        },
    );
    grant_if_needed(&rt, job);

    if replacement_succeeded {
        assert_eq!(
            fs::read_to_string(original.join("pinned.txt")).expect("original target"),
            "original capability"
        );
        assert!(
            !workspace.join("pinned.txt").exists(),
            "replacement workspace received the effect"
        );
    } else {
        assert_eq!(
            fs::read_to_string(workspace.join("pinned.txt")).expect("locked original target"),
            "original capability"
        );
    }
}

#[cfg(windows)]
#[test]
fn drive_relative_path_is_rejected() {
    let root = tempdir().expect("root tempdir");
    let workspace = root.path().join("workspace");
    let rt = Runtime::open(&root.path().join("optimus.db"), &workspace).expect("runtime");
    let job = create_single_effect_job(
        &rt,
        "reject-drive-relative",
        Effect::WriteFile {
            relative_path: "C:escaped.txt".into(),
            contents: "must not escape".into(),
        },
    );

    let error = rt
        .run_next(job)
        .expect_err("drive-relative path must be denied");
    assert!(matches!(error, RuntimeError::PathEscape(_)), "{error:?}");
}

#[test]
fn mkdir_delete_rename_patch_under_smart_deny_and_confinement() {
    let root = tempdir().expect("root");
    let workspace = root.path().join("workspace");
    let rt = Runtime::open(&root.path().join("optimus.db"), &workspace).expect("runtime");

    let mkdir = create_single_effect_job(
        &rt,
        "mkdir",
        Effect::Mkdir {
            relative_path: "nested/dir".into(),
        },
    );
    assert!(matches!(
        rt.run_next(mkdir),
        Err(RuntimeError::NeedsApproval { .. })
    ));
    grant_if_needed(&rt, mkdir);
    assert!(workspace.join("nested/dir").is_dir());

    // seed file for patch/rename/delete
    let write = create_single_effect_job(
        &rt,
        "seed",
        Effect::WriteFile {
            relative_path: "nested/dir/note.txt".into(),
            contents: "hello world".into(),
        },
    );
    grant_if_needed(&rt, write);

    let patch = create_single_effect_job(
        &rt,
        "patch",
        Effect::PatchFile {
            relative_path: "nested/dir/note.txt".into(),
            old_string: "hello".into(),
            new_string: "hola".into(),
        },
    );
    assert!(matches!(
        rt.run_next(patch),
        Err(RuntimeError::NeedsApproval { .. })
    ));
    grant_if_needed(&rt, patch);
    assert_eq!(
        fs::read_to_string(workspace.join("nested/dir/note.txt")).unwrap(),
        "hola world"
    );

    let rename = create_single_effect_job(
        &rt,
        "rename",
        Effect::RenamePath {
            from_relative_path: "nested/dir/note.txt".into(),
            to_relative_path: "nested/dir/renamed.txt".into(),
        },
    );
    grant_if_needed(&rt, rename);
    assert!(workspace.join("nested/dir/renamed.txt").is_file());
    assert!(!workspace.join("nested/dir/note.txt").exists());

    let delete = create_single_effect_job(
        &rt,
        "delete",
        Effect::DeletePath {
            relative_path: "nested/dir/renamed.txt".into(),
        },
    );
    grant_if_needed(&rt, delete);
    assert!(!workspace.join("nested/dir/renamed.txt").exists());
}

#[test]
fn patch_fails_closed_when_old_string_not_unique() {
    let root = tempdir().expect("root");
    let workspace = root.path().join("workspace");
    let rt = Runtime::open(&root.path().join("optimus.db"), &workspace).expect("runtime");
    let write = create_single_effect_job(
        &rt,
        "seed",
        Effect::WriteFile {
            relative_path: "dup.txt".into(),
            contents: "aa aa".into(),
        },
    );
    grant_if_needed(&rt, write);
    let patch = create_single_effect_job(
        &rt,
        "patch-dup",
        Effect::PatchFile {
            relative_path: "dup.txt".into(),
            old_string: "aa".into(),
            new_string: "bb".into(),
        },
    );
    rt.grant_approval(ApprovalGrant::for_job(patch))
        .expect("grant");
    let err = rt.run_next(patch).expect_err("non-unique patch");
    assert!(matches!(err, RuntimeError::Effector(_)), "{err:?}");
    assert_eq!(
        fs::read_to_string(workspace.join("dup.txt")).unwrap(),
        "aa aa"
    );
}

#[test]
fn rename_rejects_parent_traversal_on_either_side() {
    let root = tempdir().expect("root");
    let workspace = root.path().join("workspace");
    let rt = Runtime::open(&root.path().join("optimus.db"), &workspace).expect("runtime");
    let job = create_single_effect_job(
        &rt,
        "bad-rename",
        Effect::RenamePath {
            from_relative_path: "../escape".into(),
            to_relative_path: "ok.txt".into(),
        },
    );
    let err = rt.run_next(job).expect_err("preflight");
    assert!(
        matches!(err, RuntimeError::PathEscape(_))
            || matches!(err, RuntimeError::NeedsApproval { .. }),
        "{err:?}"
    );
    // Even if SmartDeny first, grant then fail path escape
    if matches!(err, RuntimeError::NeedsApproval { .. }) {
        rt.grant_approval(ApprovalGrant::for_job(job)).unwrap();
        let err2 = rt.run_next(job).expect_err("path");
        assert!(matches!(err2, RuntimeError::PathEscape(_)), "{err2:?}");
    }
}
