use greentic_dw_context::{ContextPackage, validate_context_package};
use greentic_dw_delegation::{SubtaskEnvelope, validate_subtask_envelope};
use greentic_dw_planning::{PlanDocument, validate_plan};
use greentic_dw_reflection::ReviewOutcome;
use greentic_dw_workspace::{ArtifactContent, validate_content};
use std::fs;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn fixture_path(name: &str) -> PathBuf {
    workspace_root().join("fixtures/deep").join(name)
}

#[test]
fn plan_fixture_round_trips_from_json_and_cbor() {
    let json = fs::read_to_string(fixture_path("plan.basic.json")).expect("read json fixture");
    let plan_from_json: PlanDocument = serde_json::from_str(&json).expect("parse plan json");
    validate_plan(&plan_from_json).expect("plan json fixture should validate");

    let cbor = fs::read(fixture_path("plan.basic.cbor")).expect("read cbor fixture");
    let plan_from_cbor: PlanDocument = serde_cbor::from_slice(&cbor).expect("parse plan cbor");
    validate_plan(&plan_from_cbor).expect("plan cbor fixture should validate");

    assert_eq!(plan_from_json, plan_from_cbor);
}

#[test]
fn context_artifact_review_and_delegation_fixtures_validate() {
    let context: ContextPackage = serde_json::from_str(
        &fs::read_to_string(fixture_path("context.basic.json")).expect("read context fixture"),
    )
    .expect("parse context fixture");
    validate_context_package(&context).expect("context fixture should validate");

    let artifact: ArtifactContent = serde_json::from_str(
        &fs::read_to_string(fixture_path("artifact.note.json")).expect("read artifact fixture"),
    )
    .expect("parse artifact fixture");
    validate_content(&artifact).expect("artifact fixture should validate");

    let review: ReviewOutcome = serde_json::from_str(
        &fs::read_to_string(fixture_path("review.accept.json")).expect("read review fixture"),
    )
    .expect("parse review fixture");
    review.validate().expect("review fixture should validate");

    let delegation: SubtaskEnvelope = serde_json::from_str(
        &fs::read_to_string(fixture_path("delegation.single.json"))
            .expect("read delegation fixture"),
    )
    .expect("parse delegation fixture");
    validate_subtask_envelope(&delegation).expect("delegation fixture should validate");
}
