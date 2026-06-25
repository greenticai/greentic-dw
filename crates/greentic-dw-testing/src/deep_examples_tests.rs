use greentic_dw_context::{ContextPackage, validate_context_package};
use greentic_dw_delegation::{SubtaskEnvelope, validate_subtask_envelope};
use greentic_dw_manifest::DigitalWorkerManifest;
use greentic_dw_planning::{PlanDocument, validate_plan};
use greentic_dw_reflection::ReviewOutcome;
use greentic_dw_types::{
    ApplicationPackLayoutHints, DwApplication, DwApplicationPackSpec, DwBundlePlan,
    DwCompositionDocument,
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
fn agentic_worker_pack_example_reuses_dw_pack_path() {
    let composition: DwCompositionDocument = serde_json::from_str(
        &fs::read_to_string(example_path(
            "agentic-worker-pack/fixtures/composition.json",
        ))
        .expect("read AW composition"),
    )
    .expect("parse AW composition");

    assert_eq!(composition.agents.len(), 3);
    assert!(composition.output_plan.supports_multi_agent_app_pack);
    assert!(
        composition
            .application
            .tags
            .contains(&"coordinator-entrypoint".to_string())
    );
    assert!(
        composition
            .application
            .tags
            .contains(&"specialist-workers-as-tools".to_string())
    );

    let coordinator = composition
        .agents
        .iter()
        .find(|agent| agent.agent_id == "coordinator")
        .expect("coordinator agent");
    assert!(
        coordinator
            .behavior_config
            .enabled_question_block_ids
            .contains(&"aw.telco.public_entrypoint".to_string())
    );
    assert!(
        coordinator
            .behavior_config
            .enabled_question_block_ids
            .contains(&"aw.telco.delegation_policy".to_string())
    );

    for specialist_id in ["traffic-specialist", "bgp-specialist"] {
        let specialist = composition
            .agents
            .iter()
            .find(|agent| agent.agent_id == specialist_id)
            .expect("specialist agent");
        assert!(
            specialist
                .behavior_config
                .enabled_question_block_ids
                .contains(&"aw.telco.specialist_worker_tool".to_string())
        );
    }

    let expected_agents = vec![
        "bgp-specialist".to_string(),
        "coordinator".to_string(),
        "traffic-specialist".to_string(),
    ];

    let routing = composition
        .routing
        .as_ref()
        .expect("agentic routing policy");
    assert_eq!(routing.coordinator_agent_id.as_deref(), Some("coordinator"));
    assert_eq!(routing.finalizer_agent_id.as_deref(), Some("coordinator"));
    assert_eq!(routing.callable_workers.len(), 2);
    assert!(routing.callable_workers.iter().any(|tool| {
        tool.tool_id == "traffic_analysis" && tool.target_agent_id == "traffic-specialist"
    }));
    assert!(routing.callable_workers.iter().any(|tool| {
        tool.tool_id == "bgp_analysis" && tool.target_agent_id == "bgp-specialist"
    }));
    assert!(
        routing
            .routes
            .iter()
            .all(|route| route.from_agent_id == "coordinator")
    );

    let pack_spec = composition
        .to_application_pack_spec()
        .expect("generate AW app pack");
    assert_eq!(
        pack_spec.metadata.pack_id,
        "pack.generated.aw.app.telco-investigation"
    );
    assert_eq!(
        pack_spec.metadata.application_id,
        "aw.app.telco-investigation"
    );
    assert!(pack_spec.metadata.multi_agent);
    assert_eq!(pack_spec.agents.len(), 3);
    assert_eq!(
        pack_spec.layout.layout_hint,
        Some(ApplicationPackLayoutHints::MultiAgentSharedProviders)
    );
    assert_eq!(
        pack_spec.layout.shared_asset_roots,
        vec!["shared".to_string()]
    );
    let pack_routing = pack_spec.routing.as_ref().expect("pack routing policy");
    assert_eq!(pack_routing.callable_workers, routing.callable_workers);

    let application_config = pack_spec
        .generated_configs
        .iter()
        .find(|asset| asset.asset_id == "generated.config.application")
        .expect("shared application config");
    let mut config_agents = application_config.applies_to_agents.clone();
    config_agents.sort();
    assert_eq!(config_agents, expected_agents);

    let observer_dependency = pack_spec
        .dependency_pack_refs
        .iter()
        .find(|dependency| dependency.pack_id == "provider.observer.audit.basic")
        .expect("shared observer dependency");
    let mut observer_agents = observer_dependency.applies_to_agents.clone();
    observer_agents.sort();
    assert_eq!(observer_agents, expected_agents);

    assert!(pack_spec.requirements.iter().any(|requirement| {
        requirement.provider_id.as_deref() == Some("provider.delegation.policy.basic")
            && requirement.applies_to_agents == vec!["coordinator".to_string()]
    }));
    assert!(pack_spec.requirements.iter().any(|requirement| {
        requirement.provider_id.as_deref() == Some("provider.tool.telco.traffic")
            && requirement.applies_to_agents == vec!["traffic-specialist".to_string()]
    }));
    assert!(pack_spec.requirements.iter().any(|requirement| {
        requirement.provider_id.as_deref() == Some("provider.tool.telco.bgp")
            && requirement.applies_to_agents == vec!["bgp-specialist".to_string()]
    }));

    let bundle_plan = composition
        .to_bundle_plan()
        .expect("generate AW bundle plan");
    assert!(bundle_plan.multi_agent);
    assert_eq!(
        bundle_plan.generated_app_pack.pack_id,
        "pack.generated.aw.app.telco-investigation"
    );
    assert!(bundle_plan.provider_packs.iter().any(|pack| {
        pack.provider_id == "provider.delegation.policy.basic"
            && pack.applies_to_agents == vec!["coordinator".to_string()]
    }));
    assert!(bundle_plan.provider_packs.iter().any(|pack| {
        pack.provider_id == "provider.observer.audit.basic" && {
            let mut applies_to_agents = pack.applies_to_agents.clone();
            applies_to_agents.sort();
            applies_to_agents == expected_agents
        }
    }));
    assert!(bundle_plan.support_packs.iter().any(|pack| {
        pack.pack_id == "support.aw.telco.common"
            && pack.rationale.as_deref()
                == Some(
                    "Shared Telco-X prompts, coordinator handoff policy, specialist worker tool schemas, and final response policy",
                )
    }));
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
