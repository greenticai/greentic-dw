use greentic_dw_context::context_package_fixture;
use greentic_dw_delegation::{delegation_decision_fixture, subtask_envelope_fixture};
use greentic_dw_planning::basic_plan_fixture;
use greentic_dw_reflection::review_outcome_fixture;
use greentic_dw_workspace::artifact_content_fixture;
use std::fs;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn snapshot_path(name: &str) -> PathBuf {
    workspace_root().join("fixtures/deep/snapshots").join(name)
}

#[test]
fn golden_snapshot_plan_matches_fixture() {
    let actual = serde_json::to_string_pretty(&basic_plan_fixture()).expect("serialize plan");
    let expected = fs::read_to_string(snapshot_path("plan-document.json")).expect("read snapshot");
    assert_eq!(actual.trim_end(), expected.trim_end());
}

#[test]
fn golden_snapshot_context_matches_fixture() {
    let actual =
        serde_json::to_string_pretty(&context_package_fixture()).expect("serialize context");
    let expected =
        fs::read_to_string(snapshot_path("context-package.json")).expect("read snapshot");
    assert_eq!(actual.trim_end(), expected.trim_end());
}

#[test]
fn golden_snapshot_artifact_matches_fixture() {
    let actual =
        serde_json::to_string_pretty(&artifact_content_fixture()).expect("serialize artifact");
    let expected =
        fs::read_to_string(snapshot_path("workspace-artifact.json")).expect("read snapshot");
    assert_eq!(actual.trim_end(), expected.trim_end());
}

#[test]
fn golden_snapshot_review_matches_fixture() {
    let actual = serde_json::to_string_pretty(&review_outcome_fixture()).expect("serialize review");
    let expected = fs::read_to_string(snapshot_path("review-outcome.json")).expect("read snapshot");
    assert_eq!(actual.trim_end(), expected.trim_end());
}

#[test]
fn golden_snapshot_delegation_matches_fixture() {
    let actual =
        serde_json::to_string_pretty(&delegation_decision_fixture()).expect("serialize decision");
    let expected =
        fs::read_to_string(snapshot_path("delegation-decision.json")).expect("read snapshot");
    assert_eq!(actual.trim_end(), expected.trim_end());

    let envelope_actual =
        serde_json::to_string_pretty(&subtask_envelope_fixture()).expect("serialize envelope");
    let envelope_expected =
        fs::read_to_string(snapshot_path("subtask-envelope.json")).expect("read snapshot");
    assert_eq!(envelope_actual.trim_end(), envelope_expected.trim_end());
}
