use greentic_dw_context::{ContextPackage, validate_context_package};
use greentic_dw_delegation::{SubtaskEnvelope, validate_subtask_envelope};
use greentic_dw_manifest::DigitalWorkerManifest;
use greentic_dw_planning::{PlanDocument, validate_plan};
use greentic_dw_reflection::ReviewOutcome;
use greentic_dw_types::{
    DwApplication, DwApplicationPackSpec, DwBundlePlan, DwCompositionDocument,
};
use greentic_dw_workspace::{ArtifactContent, validate_content};
use std::fs;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn example_path(path: &str) -> PathBuf {
    workspace_root().join("examples").join(path)
}

#[test]
fn deep_research_example_assets_validate() {
    let manifest: DigitalWorkerManifest = serde_json::from_str(
        &fs::read_to_string(example_path(
            "deep-research/manifests/deep-research.manifest.json",
        ))
        .expect("read deep research manifest"),
    )
    .expect("parse deep research manifest");
    manifest
        .validate()
        .expect("deep research manifest validates");

    let plan: PlanDocument = serde_json::from_str(
        &fs::read_to_string(example_path("deep-research/fixtures/plan.json"))
            .expect("read research plan"),
    )
    .expect("parse research plan");
    validate_plan(&plan).expect("research plan validates");
    assert_eq!(plan.steps.len(), 3);

    let context: ContextPackage = serde_json::from_str(
        &fs::read_to_string(example_path("deep-research/fixtures/context.json"))
            .expect("read research context"),
    )
    .expect("parse research context");
    validate_context_package(&context).expect("research context validates");

    let notes: ArtifactContent = serde_json::from_str(
        &fs::read_to_string(example_path("deep-research/expected/notes.artifact.json"))
            .expect("read research notes"),
    )
    .expect("parse research notes");
    validate_content(&notes).expect("research notes validate");

    let report: ArtifactContent = serde_json::from_str(
        &fs::read_to_string(example_path("deep-research/expected/report.artifact.json"))
            .expect("read research report"),
    )
    .expect("parse research report");
    validate_content(&report).expect("research report validates");

    let review: ReviewOutcome = serde_json::from_str(
        &fs::read_to_string(example_path("deep-research/expected/review.json"))
            .expect("read research review"),
    )
    .expect("parse research review");
    review.validate().expect("research review validates");
}

#[test]
fn incident_analysis_example_assets_validate() {
    let manifest: DigitalWorkerManifest = serde_json::from_str(
        &fs::read_to_string(example_path(
            "incident-analysis/manifests/incident-analysis.manifest.json",
        ))
        .expect("read incident manifest"),
    )
    .expect("parse incident manifest");
    manifest.validate().expect("incident manifest validates");

    let app: DwApplication = serde_json::from_str(
        &fs::read_to_string(example_path("incident-analysis/expected/application.json"))
            .expect("read incident app"),
    )
    .expect("parse incident app");
    assert_eq!(app.agents.len(), 4);
    assert!(app.routing.is_some());

    let log_delegate: SubtaskEnvelope = serde_json::from_str(
        &fs::read_to_string(example_path(
            "incident-analysis/fixtures/delegation.log-analysis.json",
        ))
        .expect("read log delegate"),
    )
    .expect("parse log delegate");
    validate_subtask_envelope(&log_delegate).expect("log delegate validates");

    let correlation_delegate: SubtaskEnvelope = serde_json::from_str(
        &fs::read_to_string(example_path(
            "incident-analysis/fixtures/delegation.change-correlation.json",
        ))
        .expect("read correlation delegate"),
    )
    .expect("parse correlation delegate");
    validate_subtask_envelope(&correlation_delegate).expect("correlation delegate validates");

    let review: ReviewOutcome = serde_json::from_str(
        &fs::read_to_string(example_path("incident-analysis/expected/final-review.json"))
            .expect("read final review"),
    )
    .expect("parse final review");
    review.validate().expect("final review validates");
}

#[test]
fn deep_pack_bundle_example_matches_generated_outputs() {
    let manifest: DigitalWorkerManifest = serde_json::from_str(
        &fs::read_to_string(example_path(
            "deep-pack-bundle/manifests/deep-pack.manifest.json",
        ))
        .expect("read pack manifest"),
    )
    .expect("parse pack manifest");
    manifest.validate().expect("pack manifest validates");

    let composition: DwCompositionDocument = serde_json::from_str(
        &fs::read_to_string(example_path("deep-pack-bundle/fixtures/composition.json"))
            .expect("read composition"),
    )
    .expect("parse composition");

    let expected_pack: DwApplicationPackSpec = serde_json::from_str(
        &fs::read_to_string(example_path(
            "deep-pack-bundle/expected/application-pack.json",
        ))
        .expect("read expected pack"),
    )
    .expect("parse expected pack");

    let expected_bundle: DwBundlePlan = serde_json::from_str(
        &fs::read_to_string(example_path("deep-pack-bundle/expected/bundle-plan.json"))
            .expect("read expected bundle"),
    )
    .expect("parse expected bundle");

    assert_eq!(
        composition
            .to_application_pack_spec()
            .expect("generate pack spec"),
        expected_pack
    );
    assert_eq!(
        composition.to_bundle_plan().expect("generate bundle plan"),
        expected_bundle
    );

    let inspect_value: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(example_path(
            "deep-pack-bundle/expected/inspect-output.json",
        ))
        .expect("read inspect output"),
    )
    .expect("parse inspect output");
    assert!(inspect_value.get("application_id").is_some());
    assert!(inspect_value.get("provider_pack_ids").is_some());
}
