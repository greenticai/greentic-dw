use greentic_dw_context::{
    BuildContextRequest, CompressContextRequest, CompressedContext, ContextBudget, ContextPackage,
    ContextProvider, SummarizeContextRequest, SummaryArtifactRef, context_package_fixture,
    validate_context_package,
};
use greentic_dw_delegation::{
    DelegationDecision, DelegationError, DelegationMode, DelegationProvider, MergePolicy,
    MergeSubtaskResultRequest, StartSubtaskRequest, SubtaskEnvelope, delegation_decision_fixture,
    subtask_envelope_fixture, validate_decision, validate_subtask_envelope,
};
use greentic_dw_planning::{
    CompletionCheckRequest, CompletionState, CreatePlanRequest, NextActionsRequest, PlanDocument,
    PlanRevision, PlannedAction, PlanningError, PlanningProvider, RevisePlanRequest,
    StepResultRequest, basic_plan_fixture, plan_document_schema_json, validate_plan,
};
use greentic_dw_reflection::{
    ReflectionError, ReflectionProvider, ReviewFinalRequest, ReviewOutcome, ReviewPlanRequest,
    ReviewStepRequest, review_outcome_fixture,
};
use greentic_dw_workspace::{
    ArtifactContent, ArtifactRef, ArtifactSummary, ArtifactVersion, CreateArtifactRequest,
    LinkArtifactsRequest, ListArtifactsRequest, ReadArtifactRequest, UpdateArtifactRequest,
    WorkspaceError, WorkspaceProvider, artifact_content_fixture, validate_content,
};
use std::collections::BTreeMap;

#[test]
fn deep_contract_documents_round_trip_through_json() {
    let plan = basic_plan_fixture();
    let context = context_package_fixture();
    let artifact = artifact_content_fixture();
    let review = review_outcome_fixture();
    let delegation = subtask_envelope_fixture();

    let plan_round_trip: PlanDocument =
        serde_json::from_str(&serde_json::to_string(&plan).expect("plan json")).expect("plan rt");
    let context_round_trip: ContextPackage =
        serde_json::from_str(&serde_json::to_string(&context).expect("context json"))
            .expect("context rt");
    let artifact_round_trip: ArtifactContent =
        serde_json::from_str(&serde_json::to_string(&artifact).expect("artifact json"))
            .expect("artifact rt");
    let review_round_trip: ReviewOutcome =
        serde_json::from_str(&serde_json::to_string(&review).expect("review json"))
            .expect("review rt");
    let delegation_round_trip: SubtaskEnvelope =
        serde_json::from_str(&serde_json::to_string(&delegation).expect("delegation json"))
            .expect("delegation rt");

    assert_eq!(plan_round_trip, plan);
    assert_eq!(context_round_trip, context);
    assert_eq!(artifact_round_trip, artifact);
    assert_eq!(review_round_trip, review);
    assert_eq!(delegation_round_trip, delegation);
}

#[test]
fn deep_contract_validators_cover_every_family() {
    validate_plan(&basic_plan_fixture()).expect("plan validates");
    validate_context_package(&context_package_fixture()).expect("context validates");
    validate_content(&artifact_content_fixture()).expect("artifact validates");
    review_outcome_fixture()
        .validate()
        .expect("review validates");
    validate_decision(&delegation_decision_fixture()).expect("decision validates");
    validate_subtask_envelope(&subtask_envelope_fixture()).expect("envelope validates");
    assert!(plan_document_schema_json().contains("PlanDocument"));
}

#[test]
fn provider_stub_matrix_covers_all_deep_agent_families() {
    struct StubPlanner;
    impl PlanningProvider for StubPlanner {
        fn create_plan(&self, _req: CreatePlanRequest) -> Result<PlanDocument, PlanningError> {
            Ok(basic_plan_fixture())
        }
        fn revise_plan(&self, _req: RevisePlanRequest) -> Result<PlanRevision, PlanningError> {
            Ok(PlanRevision {
                revision: 2,
                reason: "stub".to_string(),
                changed_step_ids: vec!["research".to_string()],
                metadata: BTreeMap::new(),
            })
        }
        fn next_actions(
            &self,
            _req: NextActionsRequest,
        ) -> Result<Vec<PlannedAction>, PlanningError> {
            Ok(vec![PlannedAction {
                step_id: "research".to_string(),
                action: "execute".to_string(),
            }])
        }
        fn record_step_result(
            &self,
            req: StepResultRequest,
        ) -> Result<PlanDocument, PlanningError> {
            Ok(req.plan)
        }
        fn evaluate_completion(
            &self,
            _req: CompletionCheckRequest,
        ) -> Result<CompletionState, PlanningError> {
            Ok(CompletionState::Incomplete)
        }
    }

    struct StubContext;
    impl ContextProvider for StubContext {
        fn build_context(
            &self,
            req: BuildContextRequest,
        ) -> Result<ContextPackage, greentic_dw_context::ContextError> {
            Ok(ContextPackage {
                package_id: "context-stub".to_string(),
                fragments: vec![],
                budget: req.budget,
            })
        }
        fn compress_context(
            &self,
            _req: CompressContextRequest,
        ) -> Result<CompressedContext, greentic_dw_context::ContextError> {
            Ok(CompressedContext {
                source_package_id: "context-stub".to_string(),
                compressed_artifact_ref: "artifact://compressed".to_string(),
                fragment_count: 0,
            })
        }
        fn summarize_context(
            &self,
            _req: SummarizeContextRequest,
        ) -> Result<SummaryArtifactRef, greentic_dw_context::ContextError> {
            Ok(SummaryArtifactRef {
                artifact_ref: "artifact://summary".to_string(),
            })
        }
    }

    struct StubWorkspace;
    impl WorkspaceProvider for StubWorkspace {
        fn create_artifact(
            &self,
            req: CreateArtifactRequest,
        ) -> Result<ArtifactRef, WorkspaceError> {
            Ok(req.artifact)
        }
        fn read_artifact(
            &self,
            _req: ReadArtifactRequest,
        ) -> Result<ArtifactContent, WorkspaceError> {
            Ok(artifact_content_fixture())
        }
        fn update_artifact(
            &self,
            _req: UpdateArtifactRequest,
        ) -> Result<ArtifactVersion, WorkspaceError> {
            Ok(artifact_content_fixture().version)
        }
        fn list_artifacts(
            &self,
            _req: ListArtifactsRequest,
        ) -> Result<Vec<ArtifactSummary>, WorkspaceError> {
            let content = artifact_content_fixture();
            Ok(vec![ArtifactSummary {
                artifact: content.artifact,
                latest_version: content.version,
                metadata: content.metadata,
            }])
        }
        fn link_artifacts(&self, _req: LinkArtifactsRequest) -> Result<(), WorkspaceError> {
            Ok(())
        }
    }

    struct StubReflector;
    impl ReflectionProvider for StubReflector {
        fn review_step(&self, _req: ReviewStepRequest) -> Result<ReviewOutcome, ReflectionError> {
            Ok(review_outcome_fixture())
        }
        fn review_plan(&self, _req: ReviewPlanRequest) -> Result<ReviewOutcome, ReflectionError> {
            Ok(review_outcome_fixture())
        }
        fn review_final(&self, _req: ReviewFinalRequest) -> Result<ReviewOutcome, ReflectionError> {
            Ok(review_outcome_fixture())
        }
    }

    struct StubDelegator;
    impl DelegationProvider for StubDelegator {
        fn choose_delegate(
            &self,
            _req: greentic_dw_delegation::DelegationRequest,
        ) -> Result<DelegationDecision, DelegationError> {
            Ok(DelegationDecision {
                mode: DelegationMode::Single,
                target_agents: vec!["delegate".to_string()],
                merge_policy: MergePolicy::FirstSuccess,
                rationale: "stub".to_string(),
            })
        }
        fn start_subtask(
            &self,
            req: StartSubtaskRequest,
        ) -> Result<greentic_dw_delegation::DelegationHandle, DelegationError> {
            Ok(greentic_dw_delegation::DelegationHandle {
                subtask_id: req.envelope.subtask_id,
                target_agent: req.envelope.target_agent,
            })
        }
        fn merge_result(
            &self,
            _req: MergeSubtaskResultRequest,
        ) -> Result<greentic_dw_delegation::DelegationMergeResult, DelegationError> {
            Ok(greentic_dw_delegation::DelegationMergeResult {
                accepted_artifact_refs: vec!["artifact://summary".to_string()],
                summary: "merged".to_string(),
            })
        }
    }

    let planner = StubPlanner;
    let context = StubContext;
    let workspace = StubWorkspace;
    let reflector = StubReflector;
    let delegator = StubDelegator;

    assert_eq!(
        planner
            .create_plan(CreatePlanRequest {
                goal: "goal".to_string(),
                assumptions: vec![],
                constraints: vec![],
                success_criteria: vec!["done".to_string()],
            })
            .expect("plan")
            .plan_id,
        "plan-basic"
    );
    assert_eq!(
        context
            .compress_context(CompressContextRequest {
                package: ContextPackage {
                    package_id: "p".to_string(),
                    fragments: vec![],
                    budget: ContextBudget {
                        max_fragments: 1,
                        max_bytes: 1,
                    },
                },
            })
            .expect("compress")
            .compressed_artifact_ref,
        "artifact://compressed"
    );
    assert_eq!(
        workspace
            .list_artifacts(ListArtifactsRequest {
                scope: artifact_content_fixture().artifact.scope,
            })
            .expect("list")
            .len(),
        1
    );
    assert!(
        reflector
            .review_step(ReviewStepRequest {
                plan_step_id: "s".to_string(),
                output_artifact_ref: "artifact://x".to_string(),
            })
            .expect("review")
            .score
            .is_some()
    );
    assert_eq!(
        delegator
            .choose_delegate(greentic_dw_delegation::DelegationRequest {
                goal: "g".to_string(),
                candidate_agents: vec!["delegate".to_string()],
            })
            .expect("delegate")
            .mode,
        DelegationMode::Single
    );
}
