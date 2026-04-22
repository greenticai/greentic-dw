use greentic_dw_context::{BuildContextRequest, ContextError, ContextPackage, ContextProvider};
use greentic_dw_delegation::{
    DelegationDecision, DelegationError, DelegationHandle, DelegationMergeResult, DelegationMode,
    DelegationProvider, DelegationRequest, MergePolicy, MergeSubtaskResultRequest,
    StartSubtaskRequest,
};
use greentic_dw_engine::{EngineDecision, StaticEngine};
use greentic_dw_planning::{
    CompletionCheckRequest, CompletionState, NextActionsRequest, PlanDocument, PlanRevision,
    PlanStatus, PlanStep, PlanStepKind, PlanStepStatus, PlannedAction, PlanningError,
    PlanningProvider, RevisePlanRequest, StepResultRequest,
};
use greentic_dw_reflection::{
    ReflectionError, ReflectionProvider, ReviewFinalRequest, ReviewOutcome, ReviewPlanRequest,
    ReviewStepRequest, ReviewVerdict,
};
use greentic_dw_runtime::{DeepLoopCoordinator, DeepLoopStatus, DwRuntime};
use greentic_dw_workspace::{
    ArtifactContent, ArtifactKind, ArtifactMetadata, ArtifactRef, ArtifactSummary, ArtifactVersion,
    CreateArtifactRequest, LinkArtifactsRequest, ListArtifactsRequest, ReadArtifactRequest,
    UpdateArtifactRequest, WorkspaceError, WorkspaceProvider, WorkspaceScope,
};
use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::default_fixture;

fn harness_plan() -> PlanDocument {
    PlanDocument {
        plan_id: "plan-harness".to_string(),
        goal: "Run a deterministic deep loop".to_string(),
        status: PlanStatus::Active,
        revision: 1,
        assumptions: vec![],
        constraints: vec![],
        success_criteria: vec!["complete".to_string()],
        steps: vec![
            PlanStep {
                step_id: "step-1".to_string(),
                title: "Execute step".to_string(),
                kind: PlanStepKind::ToolCall,
                status: PlanStepStatus::Ready,
                depends_on: vec![],
                assigned_agent: None,
                inputs_schema_ref: None,
                output_schema_ref: Some("schema://step".to_string()),
                retry_count: 0,
            },
            PlanStep {
                step_id: "step-2".to_string(),
                title: "Review final".to_string(),
                kind: PlanStepKind::Review,
                status: PlanStepStatus::Pending,
                depends_on: vec!["step-1".to_string()],
                assigned_agent: Some("reviewer".to_string()),
                inputs_schema_ref: Some("schema://step".to_string()),
                output_schema_ref: Some("schema://review".to_string()),
                retry_count: 0,
            },
        ],
        edges: vec![],
        metadata: BTreeMap::new(),
    }
}

struct HarnessPlanner {
    completed: Mutex<Vec<String>>,
}

impl HarnessPlanner {
    fn new() -> Self {
        Self {
            completed: Mutex::new(Vec::new()),
        }
    }
}

impl PlanningProvider for HarnessPlanner {
    fn create_plan(
        &self,
        _req: greentic_dw_planning::CreatePlanRequest,
    ) -> Result<PlanDocument, PlanningError> {
        Ok(harness_plan())
    }

    fn revise_plan(&self, _req: RevisePlanRequest) -> Result<PlanRevision, PlanningError> {
        Ok(PlanRevision {
            revision: 2,
            reason: "revise".to_string(),
            changed_step_ids: vec!["step-1".to_string()],
            metadata: BTreeMap::new(),
        })
    }

    fn next_actions(&self, _req: NextActionsRequest) -> Result<Vec<PlannedAction>, PlanningError> {
        let completed = self.completed.lock().expect("lock");
        if !completed.iter().any(|step| step == "step-1") {
            return Ok(vec![PlannedAction {
                step_id: "step-1".to_string(),
                action: "execute".to_string(),
            }]);
        }
        if !completed.iter().any(|step| step == "step-2") {
            return Ok(vec![PlannedAction {
                step_id: "step-2".to_string(),
                action: "execute".to_string(),
            }]);
        }
        Ok(vec![])
    }

    fn record_step_result(&self, req: StepResultRequest) -> Result<PlanDocument, PlanningError> {
        self.completed
            .lock()
            .expect("lock")
            .push(req.step_id.clone());
        let mut plan = req.plan;
        if let Some(step) = plan
            .steps
            .iter_mut()
            .find(|step| step.step_id == req.step_id)
        {
            step.status = req.status;
        }
        if req.step_id == "step-1"
            && let Some(step) = plan.steps.iter_mut().find(|step| step.step_id == "step-2")
        {
            step.status = PlanStepStatus::Ready;
        }
        Ok(plan)
    }

    fn evaluate_completion(
        &self,
        _req: CompletionCheckRequest,
    ) -> Result<CompletionState, PlanningError> {
        let completed = self.completed.lock().expect("lock");
        if completed.iter().any(|step| step == "step-1")
            && completed.iter().any(|step| step == "step-2")
        {
            Ok(CompletionState::Satisfied)
        } else {
            Ok(CompletionState::Incomplete)
        }
    }
}

struct HarnessContext;
impl ContextProvider for HarnessContext {
    fn build_context(&self, req: BuildContextRequest) -> Result<ContextPackage, ContextError> {
        Ok(ContextPackage {
            package_id: format!("context-{}", req.fragment_refs.join("-")),
            fragments: vec![],
            budget: req.budget,
        })
    }
    fn compress_context(
        &self,
        _req: greentic_dw_context::CompressContextRequest,
    ) -> Result<greentic_dw_context::CompressedContext, ContextError> {
        unreachable!()
    }
    fn summarize_context(
        &self,
        _req: greentic_dw_context::SummarizeContextRequest,
    ) -> Result<greentic_dw_context::SummaryArtifactRef, ContextError> {
        unreachable!()
    }
}

#[derive(Default)]
struct HarnessWorkspace {
    created: Mutex<Vec<String>>,
}
impl WorkspaceProvider for HarnessWorkspace {
    fn create_artifact(&self, req: CreateArtifactRequest) -> Result<ArtifactRef, WorkspaceError> {
        self.created
            .lock()
            .expect("lock")
            .push(req.artifact.artifact_id.clone());
        Ok(req.artifact)
    }
    fn read_artifact(&self, _req: ReadArtifactRequest) -> Result<ArtifactContent, WorkspaceError> {
        unreachable!()
    }
    fn update_artifact(
        &self,
        _req: UpdateArtifactRequest,
    ) -> Result<ArtifactVersion, WorkspaceError> {
        unreachable!()
    }
    fn list_artifacts(
        &self,
        _req: ListArtifactsRequest,
    ) -> Result<Vec<ArtifactSummary>, WorkspaceError> {
        Ok(vec![])
    }
    fn link_artifacts(&self, _req: LinkArtifactsRequest) -> Result<(), WorkspaceError> {
        Ok(())
    }
}

struct HarnessReflector;
impl ReflectionProvider for HarnessReflector {
    fn review_step(&self, _req: ReviewStepRequest) -> Result<ReviewOutcome, ReflectionError> {
        Ok(ReviewOutcome {
            verdict: ReviewVerdict::Accept,
            score: Some(1.0),
            findings: vec![],
            suggested_actions: vec![],
            binding: false,
        })
    }
    fn review_plan(&self, _req: ReviewPlanRequest) -> Result<ReviewOutcome, ReflectionError> {
        Ok(ReviewOutcome {
            verdict: ReviewVerdict::Accept,
            score: Some(1.0),
            findings: vec![],
            suggested_actions: vec![],
            binding: false,
        })
    }
    fn review_final(&self, _req: ReviewFinalRequest) -> Result<ReviewOutcome, ReflectionError> {
        Ok(ReviewOutcome {
            verdict: ReviewVerdict::Accept,
            score: Some(1.0),
            findings: vec![],
            suggested_actions: vec![],
            binding: false,
        })
    }
}

struct HarnessDelegator;
impl DelegationProvider for HarnessDelegator {
    fn choose_delegate(
        &self,
        _req: DelegationRequest,
    ) -> Result<DelegationDecision, DelegationError> {
        Ok(DelegationDecision {
            mode: DelegationMode::Single,
            target_agents: vec!["delegate".to_string()],
            merge_policy: MergePolicy::FirstSuccess,
            rationale: "stub".to_string(),
        })
    }
    fn start_subtask(&self, req: StartSubtaskRequest) -> Result<DelegationHandle, DelegationError> {
        Ok(DelegationHandle {
            subtask_id: req.envelope.subtask_id,
            target_agent: req.envelope.target_agent,
        })
    }
    fn merge_result(
        &self,
        _req: MergeSubtaskResultRequest,
    ) -> Result<DelegationMergeResult, DelegationError> {
        Ok(DelegationMergeResult {
            accepted_artifact_refs: vec![],
            summary: "merged".to_string(),
        })
    }
}

#[test]
fn deep_loop_harness_runs_full_lifecycle_locally() {
    let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Operation(
        greentic_dw_core::RuntimeOperation::Step,
    )));
    let planner = HarnessPlanner::new();
    let context = HarnessContext;
    let workspace = HarnessWorkspace::default();
    let reflector = HarnessReflector;
    let delegator = HarnessDelegator;
    let coordinator = DeepLoopCoordinator {
        runtime: &runtime,
        planner: &planner,
        context: &context,
        workspace: &workspace,
        reflector: &reflector,
        delegator: &delegator,
    };

    let mut envelope = default_fixture().task_envelope();
    let run = coordinator
        .run(&mut envelope, harness_plan())
        .expect("harness should run");

    assert_eq!(run.status, DeepLoopStatus::Completed);
    assert_eq!(run.output_artifact_ids.len(), 2);
    assert_eq!(workspace.created.lock().expect("lock").len(), 2);
    assert_eq!(
        envelope.state,
        greentic_dw_types::TaskLifecycleState::Completed
    );
}

#[test]
fn deep_loop_harness_uses_deterministic_workspace_scope() {
    let scope = WorkspaceScope {
        tenant: "tenant-a".to_string(),
        team: Some("team-a".to_string()),
        session: "session-1".to_string(),
        agent: Some("worker".to_string()),
        run: "run-1".to_string(),
    };
    let artifact = ArtifactRef {
        artifact_id: "artifact://harness".to_string(),
        kind: ArtifactKind::ToolOutput,
        scope,
    };
    let metadata = ArtifactMetadata {
        title: "Harness artifact".to_string(),
        tags: vec!["deep-loop".to_string()],
        mime_type: Some("application/json".to_string()),
    };
    assert_eq!(artifact.scope.tenant, "tenant-a");
    assert_eq!(metadata.title, "Harness artifact");
}
