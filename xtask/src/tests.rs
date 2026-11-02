use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    RELEASE_WORKFLOW_TIMEOUT, RELEASE_WORKFLOWS, RemoteTagState, WorkflowRun,
    ensure_clean_worktree, parse_remote_tag_sha, parse_workflow_run, stage_changed_files,
    update_workflow_progress, verify_local_tag, verify_release_branch, verify_release_head,
    verify_remote_tag, workflow_run_list_args, workspace_manifests_from_metadata,
};

struct TempRepo(PathBuf);

impl TempRepo {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mirui-xtask-{label}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        let status = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&path)
            .status()
            .unwrap();
        assert!(status.success());
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn completed(conclusion: &str) -> Option<WorkflowRun> {
    Some(WorkflowRun {
        status: "completed".to_string(),
        conclusion: conclusion.to_string(),
    })
}

fn pending() -> Option<WorkflowRun> {
    Some(WorkflowRun {
        status: "in_progress".to_string(),
        conclusion: String::new(),
    })
}

#[test]
fn bump_targets_only_workspace_members() {
    let metadata = br#"{
        "workspace_members": ["root 0.42.0", "member 0.42.0"],
        "packages": [
            {
                "id": "root 0.42.0",
                "manifest_path": "/repo/Cargo.toml"
            },
            {
                "id": "member 0.42.0",
                "manifest_path": "/repo/member/Cargo.toml"
            },
            {
                "id": "local 9.9.9",
                "manifest_path": "/repo/.local/vendor/Cargo.toml"
            }
        ]
    }"#;

    let manifests = workspace_manifests_from_metadata(metadata).unwrap();
    assert_eq!(manifests, ["/repo/Cargo.toml", "/repo/member/Cargo.toml"]);
}

#[test]
fn release_workflow_timeout_is_twenty_minutes() {
    assert_eq!(RELEASE_WORKFLOW_TIMEOUT.as_secs(), 20 * 60);
}

#[test]
fn release_waits_for_all_expected_workflows() {
    assert_eq!(
        RELEASE_WORKFLOWS.map(|workflow| workflow.file),
        ["ci.yml", "pages.yml", "nuttx-check.yml"]
    );
    assert_eq!(
        RELEASE_WORKFLOWS.map(|workflow| workflow.required_run),
        [true, true, false]
    );
}

#[test]
fn release_requires_main_branch() {
    assert_eq!(verify_release_branch("main"), Ok(()));
    assert_eq!(
        verify_release_branch("feature/release").unwrap_err(),
        "release must run from main; current branch is feature/release"
    );
    assert_eq!(
        verify_release_branch("").unwrap_err(),
        "release must run from main; current branch is detached HEAD"
    );
}

#[test]
fn release_head_must_remain_stable_while_workflows_run() {
    assert_eq!(verify_release_head("head", "head"), Ok(()));
    assert_eq!(
        verify_release_head("head", "new-head").unwrap_err(),
        "HEAD changed during release workflow wait: expected head, found new-head"
    );
}

#[test]
fn parses_workflow_run_states() {
    let run = parse_workflow_run(br#"[{"status":"completed","conclusion":"success"}]"#)
        .unwrap()
        .unwrap();
    assert_eq!(run.status, "completed");
    assert_eq!(run.conclusion, "success");
    assert_eq!(parse_workflow_run(b"[]").unwrap(), None);
}

#[test]
fn workflow_query_targets_main_push_for_head() {
    assert_eq!(
        workflow_run_list_args("head-sha", "ci.yml"),
        [
            "run",
            "list",
            "--workflow",
            "ci.yml",
            "--branch",
            "main",
            "--event",
            "push",
            "--commit",
            "head-sha",
            "--limit",
            "1",
            "--json",
            "status,conclusion",
        ]
    );
}

#[test]
fn missing_required_release_workflow_never_completes() {
    let mut progress = vec![Default::default(); RELEASE_WORKFLOWS.len()];
    let observations = vec![completed("success"), None, None];

    for _ in 0..10 {
        assert!(
            !update_workflow_progress(&RELEASE_WORKFLOWS, &mut progress, &observations).unwrap()
        );
    }
    assert!(!progress[1].complete);
    assert!(progress[2].complete);
}

#[test]
fn missing_path_filtered_workflow_does_not_block_release() {
    let mut progress = vec![Default::default(); RELEASE_WORKFLOWS.len()];
    let observations = vec![completed("success"), completed("success"), None];

    assert!(update_workflow_progress(&RELEASE_WORKFLOWS, &mut progress, &observations).unwrap());
}

#[test]
fn all_release_workflows_must_complete() {
    let mut progress = vec![Default::default(); RELEASE_WORKFLOWS.len()];
    let pending_observations = vec![completed("success"), completed("success"), pending()];
    assert!(
        !update_workflow_progress(&RELEASE_WORKFLOWS, &mut progress, &pending_observations)
            .unwrap()
    );

    let complete_observations = vec![
        completed("success"),
        completed("success"),
        completed("success"),
    ];
    assert!(
        update_workflow_progress(&RELEASE_WORKFLOWS, &mut progress, &complete_observations)
            .unwrap()
    );
}

#[test]
fn a_newer_pending_observation_resumes_waiting() {
    let mut progress = vec![Default::default(); RELEASE_WORKFLOWS.len()];
    let first_observations = vec![completed("success"), pending(), pending()];
    assert!(
        !update_workflow_progress(&RELEASE_WORKFLOWS, &mut progress, &first_observations).unwrap()
    );
    assert!(progress[0].complete);

    let second_observations = vec![pending(), pending(), pending()];
    assert!(
        !update_workflow_progress(&RELEASE_WORKFLOWS, &mut progress, &second_observations).unwrap()
    );
    assert!(!progress[0].complete);
}

#[test]
fn failed_release_workflow_stops_the_wait() {
    let mut progress = vec![Default::default(); RELEASE_WORKFLOWS.len()];
    let observations = vec![completed("failure"), completed("success"), None];
    let error =
        update_workflow_progress(&RELEASE_WORKFLOWS, &mut progress, &observations).unwrap_err();
    assert_eq!(error, "CI failed: failure");
}

#[test]
fn failed_nuttx_workflow_stops_the_wait() {
    let mut progress = vec![Default::default(); RELEASE_WORKFLOWS.len()];
    let observations = vec![
        completed("success"),
        completed("success"),
        completed("failure"),
    ];
    let error =
        update_workflow_progress(&RELEASE_WORKFLOWS, &mut progress, &observations).unwrap_err();
    assert_eq!(error, "NuttX target check failed: failure");
}

#[test]
fn remote_tag_parser_prefers_peeled_commit() {
    let direct = "refs/tags/v1.2.3";
    let peeled = "refs/tags/v1.2.3^{}";
    let output = b"tag-object\trefs/tags/v1.2.3\ncommit-sha\trefs/tags/v1.2.3^{}\n";
    assert_eq!(
        parse_remote_tag_sha(output, direct, peeled).as_deref(),
        Some("commit-sha")
    );
    assert_eq!(
        parse_remote_tag_sha(b"commit-sha\trefs/tags/v1.2.3\n", direct, peeled).as_deref(),
        Some("commit-sha")
    );
    assert_eq!(parse_remote_tag_sha(b"", direct, peeled), None);
}

#[test]
fn remote_tag_must_resolve_to_head() {
    assert_eq!(
        verify_remote_tag("v1.2.3", "head", None).unwrap(),
        RemoteTagState::Missing
    );
    assert_eq!(
        verify_remote_tag("v1.2.3", "head", Some("head")).unwrap(),
        RemoteTagState::AtHead
    );
    assert_eq!(
        verify_remote_tag("v1.2.3", "head", Some("other")).unwrap_err(),
        "remote tag v1.2.3 points at other, not HEAD head"
    );
}

#[test]
fn local_tag_must_be_missing_or_resolve_to_release_head() {
    assert!(!verify_local_tag("v1.2.3", "head", "").unwrap());
    assert!(verify_local_tag("v1.2.3", "head", "head").unwrap());
    assert_eq!(
        verify_local_tag("v1.2.3", "head", "other").unwrap_err(),
        "local tag v1.2.3 points at other, not release HEAD head"
    );
}

#[test]
fn templates_bump_rejects_a_dirty_repository() {
    let repo = TempRepo::new("dirty");
    ensure_clean_worktree(repo.path()).unwrap();
    std::fs::write(repo.path().join("unrelated.txt"), "dirty\n").unwrap();
    let error = ensure_clean_worktree(repo.path()).unwrap_err().to_string();
    assert!(error.contains("working tree not clean"));
}

#[test]
fn templates_bump_stages_only_changed_template_files() {
    let repo = TempRepo::new("stage");
    let template = repo.path().join("templates/desktop/cargo-generate.toml");
    let unrelated = repo.path().join("unrelated.txt");
    std::fs::create_dir_all(template.parent().unwrap()).unwrap();
    std::fs::write(&template, "default = \"0.47\"\n").unwrap();
    std::fs::write(&unrelated, "leave unstaged\n").unwrap();

    stage_changed_files(repo.path(), &[template.to_string_lossy().into_owned()]).unwrap();

    let staged = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(staged.status.success());
    assert_eq!(
        String::from_utf8(staged.stdout).unwrap().trim(),
        "templates/desktop/cargo-generate.toml"
    );
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(
        String::from_utf8(status.stdout)
            .unwrap()
            .contains("?? unrelated.txt")
    );
}
