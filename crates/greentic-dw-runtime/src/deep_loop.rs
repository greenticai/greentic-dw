use greentic_dw_context::{BuildContextRequest, ContextBudget, ContextError, ContextProvider};
use greentic_dw_core::RuntimeEvent;
use greentic_dw_delegation::{
    DelegationDecision, DelegationError, DelegationMode, DelegationProvider, DelegationRequest,
    MergePolicy, StartSubtaskRequest, SubtaskEnvelope,
};
use greentic_dw_engine::DwEngine;
use greentic_dw_planning::{
    CompletionCheckRequest, CompletionState, NextActionsRequest, PlanDocument, PlanStepKind,
    PlanStepStatus, PlannedAction, PlanningError, PlanningProvider, RevisePlanRequest,
    StepResultRequest,
};
use greentic_dw_reflection::{
    ReflectionError, ReflectionProvider, ReviewFinalRequest, ReviewStepRequest, ReviewVerdict,
};
use greentic_dw_types::TaskEnvelope;
use greentic_dw_workspace::{
    ArtifactKind, ArtifactMetadata, ArtifactRef, CreateArtifactRequest, WorkspaceError,
    WorkspaceProvider,
};
use thiserror::Error;

use crate::{DwRuntime, RuntimeError};

const DEFAULT_MAX_ITERATIONS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepLoopStatus {
    Idle,
    Planning,
    Executing,
    Reflecting,
    Revising,
    Delegating,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepLoopRun {
    pub plan: PlanDocument,
    pub status: DeepLoopStatus,
    pub emitted_subtasks: Vec<SubtaskEnvelope>,
    pub output_artifact_ids: Vec<String>,
}

#[derive(Debug, Error)]
pub enum DeepLoopError {
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    Planning(#[from] PlanningError),
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Reflection(#[from] ReflectionError),
    #[error(transparent)]
    Delegation(#[from] DelegationError),
    #[error("plan step `{step_id}` not found")]
    MissingStep { step_id: String },
    #[error("deep loop exceeded the maximum iteration count")]
    IterationLimitExceeded,
}

pub struct DeepLoopCoordinator<'a, E: DwEngine> {
    pub runtime: &'a DwRuntime<E>,
    pub planner: &'a dyn PlanningProvider,
    pub context: &'a dyn ContextProvider,
    pub workspace: &'a dyn WorkspaceProvider,
    pub reflector: &'a dyn ReflectionProvider,
    pub delegator: &'a dyn DelegationProvider,
}

impl<'a, E: DwEngine> DeepLoopCoordinator<'a, E> {
    pub fn run(
        &self,
        envelope: &mut TaskEnvelope,
        mut plan: PlanDocument,
    ) -> Result<DeepLoopRun, DeepLoopError> {
        let mut emitted_subtasks = Vec::new();
        let mut output_artifact_ids = Vec::new();

        if matches!(
            envelope.state,
            greentic_dw_types::TaskLifecycleState::Created
        ) {
            self.runtime.start(envelope)?;
        }

        for _ in 0..DEFAULT_MAX_ITERATIONS {
            let next_actions = self
                .planner
                .next_actions(NextActionsRequest { plan: plan.clone() })?;

            if next_actions.is_empty() {
                if !emitted_subtasks.is_empty() {
                    return Ok(DeepLoopRun {
                        plan,
                        status: DeepLoopStatus::Delegating,
                        emitted_subtasks,
                        output_artifact_ids,
                    });
                }

                match self
                    .planner
                    .evaluate_completion(CompletionCheckRequest { plan: plan.clone() })?
                {
                    CompletionState::Satisfied => {
                        let final_ref = output_artifact_ids
                            .last()
                            .cloned()
                            .unwrap_or_else(|| "artifact://deep-loop/final".to_string());
                        let outcome = self.reflector.review_final(ReviewFinalRequest {
                            run_id: envelope.task_id.clone(),
                            output_artifact_ref: final_ref,
                        })?;
                        outcome.validate()?;
                        if matches!(outcome.verdict, ReviewVerdict::Fail) {
                            self.runtime.fail(envelope, "final review failed")?;
                            return Ok(DeepLoopRun {
                                plan,
                                status: DeepLoopStatus::Failed,
                                emitted_subtasks,
                                output_artifact_ids,
                            });
                        }
                        self.runtime.complete(envelope)?;
                        return Ok(DeepLoopRun {
                            plan,
                            status: DeepLoopStatus::Completed,
                            emitted_subtasks,
                            output_artifact_ids,
                        });
                    }
                    CompletionState::Unsatisfied => {
                        self.runtime
                            .fail(envelope, "completion check unsatisfied")?;
                        return Ok(DeepLoopRun {
                            plan,
                            status: DeepLoopStatus::Failed,
                            emitted_subtasks,
                            output_artifact_ids,
                        });
                    }
                    CompletionState::Incomplete => continue,
                }
            }

            for action in next_actions {
                let step = plan
                    .steps
                    .iter()
                    .find(|step| step.step_id == action.step_id)
                    .cloned()
                    .ok_or_else(|| DeepLoopError::MissingStep {
                        step_id: action.step_id.clone(),
                    })?;

                let _context_package = self.context.build_context(BuildContextRequest {
                    fragment_refs: vec![step.step_id.clone()],
                    budget: ContextBudget {
                        max_fragments: 8,
                        max_bytes: 16_384,
                    },
                })?;

                match step.kind {
                    PlanStepKind::Delegate => {
                        let delegation_decision =
                            self.delegator.choose_delegate(DelegationRequest {
                                goal: step.title.clone(),
                                candidate_agents: step.assigned_agent.clone().into_iter().collect(),
                            })?;
                        let envelope_to_emit =
                            build_subtask_envelope(envelope, &step, &delegation_decision);
                        let target_agent = envelope_to_emit.target_agent.clone();
                        self.runtime.delegate(envelope, target_agent)?;
                        self.delegator.start_subtask(StartSubtaskRequest {
                            envelope: envelope_to_emit.clone(),
                        })?;
                        emitted_subtasks.push(envelope_to_emit);
                        plan = self.planner.record_step_result(StepResultRequest {
                            plan: plan.clone(),
                            step_id: step.step_id.clone(),
                            status: PlanStepStatus::Completed,
                        })?;
                    }
                    _ => {
                        let _events = self.execute_action(envelope, &action)?;
                        let artifact_ref =
                            self.workspace.create_artifact(CreateArtifactRequest {
                                artifact: ArtifactRef {
                                    artifact_id: format!(
                                        "artifact://{}/{}",
                                        plan.plan_id, step.step_id
                                    ),
                                    kind: ArtifactKind::ToolOutput,
                                    scope: greentic_dw_workspace::WorkspaceScope {
                                        tenant: envelope.scope.tenant.clone(),
                                        team: envelope.scope.team.clone(),
                                        session: envelope.task_id.clone(),
                                        agent: Some(envelope.worker_id.clone()),
                                        run: plan.plan_id.clone(),
                                    },
                                },
                                metadata: ArtifactMetadata {
                                    title: format!("Output for {}", step.title),
                                    tags: vec![action.action.clone()],
                                    mime_type: Some("application/json".to_string()),
                                },
                                body: format!("{{\"step_id\":\"{}\"}}", step.step_id),
                            })?;
                        output_artifact_ids.push(artifact_ref.artifact_id.clone());

                        let review = self.reflector.review_step(ReviewStepRequest {
                            plan_step_id: step.step_id.clone(),
                            output_artifact_ref: artifact_ref.artifact_id,
                        })?;
                        review.validate()?;

                        match review.verdict {
                            ReviewVerdict::Accept | ReviewVerdict::Retry => {
                                plan = self.planner.record_step_result(StepResultRequest {
                                    plan: plan.clone(),
                                    step_id: step.step_id.clone(),
                                    status: PlanStepStatus::Completed,
                                })?;
                            }
                            ReviewVerdict::Revise => {
                                let revision = self.planner.revise_plan(RevisePlanRequest {
                                    plan: plan.clone(),
                                    reason: format!(
                                        "reflection requested revision for {}",
                                        step.step_id
                                    ),
                                })?;
                                plan.revision = revision.revision;
                                return Ok(DeepLoopRun {
                                    plan,
                                    status: DeepLoopStatus::Revising,
                                    emitted_subtasks,
                                    output_artifact_ids,
                                });
                            }
                            ReviewVerdict::Delegate => {
                                let delegation_decision =
                                    self.delegator.choose_delegate(DelegationRequest {
                                        goal: format!("review {}", step.title),
                                        candidate_agents: step
                                            .assigned_agent
                                            .clone()
                                            .into_iter()
                                            .collect(),
                                    })?;
                                let envelope_to_emit =
                                    build_subtask_envelope(envelope, &step, &delegation_decision);
                                self.delegator.start_subtask(StartSubtaskRequest {
                                    envelope: envelope_to_emit.clone(),
                                })?;
                                emitted_subtasks.push(envelope_to_emit);
                                plan = self.planner.record_step_result(StepResultRequest {
                                    plan: plan.clone(),
                                    step_id: step.step_id.clone(),
                                    status: PlanStepStatus::Completed,
                                })?;
                            }
                            ReviewVerdict::Fail => {
                                self.runtime.fail(
                                    envelope,
                                    format!("reflection failed step {}", step.step_id),
                                )?;
                                return Ok(DeepLoopRun {
                                    plan,
                                    status: DeepLoopStatus::Failed,
                                    emitted_subtasks,
                                    output_artifact_ids,
                                });
                            }
                        }
                    }
                }
            }
        }

        Err(DeepLoopError::IterationLimitExceeded)
    }

    fn execute_action(
        &self,
        envelope: &mut TaskEnvelope,
        _action: &PlannedAction,
    ) -> Result<Vec<RuntimeEvent>, DeepLoopError> {
        self.runtime.tick(envelope).map_err(DeepLoopError::from)
    }
}

fn build_subtask_envelope(
    envelope: &TaskEnvelope,
    step: &greentic_dw_planning::PlanStep,
    decision: &DelegationDecision,
) -> SubtaskEnvelope {
    let target_agent = match decision.mode {
        DelegationMode::None => step
            .assigned_agent
            .clone()
            .unwrap_or_else(|| "delegate".to_string()),
        _ => decision
            .target_agents
            .first()
            .cloned()
            .or_else(|| step.assigned_agent.clone())
            .unwrap_or_else(|| "delegate".to_string()),
    };

    SubtaskEnvelope {
        subtask_id: format!("{}::{}", envelope.task_id, step.step_id),
        parent_run_id: envelope.task_id.clone(),
        target_agent,
        goal: step.title.clone(),
        context_package_ref: format!("context://{}", step.step_id),
        expected_output_schema: step
            .output_schema_ref
            .clone()
            .unwrap_or_else(|| "schema://step-output".to_string()),
        permissions_profile: "restricted".to_string(),
        deadline: "2026-04-16T00:00:00Z".to_string(),
        return_policy: match decision.merge_policy {
            MergePolicy::CollectAll => "collect_all".to_string(),
            _ => "first_return".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_dw_core::RuntimeOperation;
    use greentic_dw_delegation::{
        DelegationHandle, DelegationMergeResult, MergeSubtaskResultRequest,
    };
    use greentic_dw_engine::{EngineDecision, StaticEngine};
    use greentic_dw_types::{
        LocaleContext, LocalePropagation, OutputLocaleGuidance, TaskLifecycleState, TenantScope,
        WorkerLocalePolicy,
    };
    use std::sync::Mutex;

    fn sample_envelope() -> TaskEnvelope {
        TaskEnvelope {
            task_id: "task-1".to_string(),
            worker_id: "worker-1".to_string(),
            state: TaskLifecycleState::Created,
            scope: TenantScope {
                tenant: "tenant-a".to_string(),
                team: Some("team-a".to_string()),
            },
            locale: LocaleContext {
                worker_default_locale: "en-US".to_string(),
                requested_locale: None,
                human_locale: None,
                policy: WorkerLocalePolicy::WorkerDefault,
                propagation: LocalePropagation::CurrentTaskOnly,
                output: OutputLocaleGuidance::WorkerDefault,
            },
        }
    }

    fn two_step_plan(kind: PlanStepKind) -> PlanDocument {
        PlanDocument {
            plan_id: "plan-1".to_string(),
            goal: "Do the work".to_string(),
            status: greentic_dw_planning::PlanStatus::Active,
            revision: 1,
            assumptions: vec![],
            constraints: vec![],
            success_criteria: vec!["done".to_string()],
            steps: vec![
                greentic_dw_planning::PlanStep {
                    step_id: "step-1".to_string(),
                    title: "First".to_string(),
                    kind,
                    status: PlanStepStatus::Ready,
                    depends_on: vec![],
                    assigned_agent: Some("delegate-a".to_string()),
                    inputs_schema_ref: None,
                    output_schema_ref: Some("schema://out".to_string()),
                    retry_count: 0,
                },
                greentic_dw_planning::PlanStep {
                    step_id: "step-2".to_string(),
                    title: "Second".to_string(),
                    kind: PlanStepKind::ToolCall,
                    status: PlanStepStatus::Pending,
                    depends_on: vec!["step-1".to_string()],
                    assigned_agent: None,
                    inputs_schema_ref: None,
                    output_schema_ref: Some("schema://out".to_string()),
                    retry_count: 0,
                },
            ],
            edges: vec![],
            metadata: Default::default(),
        }
    }

    struct MockPlanner {
        completed: Mutex<Vec<String>>,
        revise_called: Mutex<bool>,
    }

    impl MockPlanner {
        fn new() -> Self {
            Self {
                completed: Mutex::new(Vec::new()),
                revise_called: Mutex::new(false),
            }
        }
    }

    impl PlanningProvider for MockPlanner {
        fn create_plan(
            &self,
            _req: greentic_dw_planning::CreatePlanRequest,
        ) -> Result<PlanDocument, PlanningError> {
            unreachable!()
        }

        fn revise_plan(
            &self,
            _req: RevisePlanRequest,
        ) -> Result<greentic_dw_planning::PlanRevision, PlanningError> {
            *self.revise_called.lock().expect("lock") = true;
            Ok(greentic_dw_planning::PlanRevision {
                revision: 2,
                reason: "revise".to_string(),
                changed_step_ids: vec!["step-1".to_string()],
                metadata: Default::default(),
            })
        }

        fn next_actions(
            &self,
            req: NextActionsRequest,
        ) -> Result<Vec<PlannedAction>, PlanningError> {
            let completed = self.completed.lock().expect("lock");
            if !completed.iter().any(|step| step == "step-1") {
                return Ok(vec![PlannedAction {
                    step_id: "step-1".to_string(),
                    action: "execute".to_string(),
                }]);
            }
            if req.plan.steps.iter().any(|step| step.step_id == "step-2")
                && !completed.iter().any(|step| step == "step-2")
            {
                return Ok(vec![PlannedAction {
                    step_id: "step-2".to_string(),
                    action: "execute".to_string(),
                }]);
            }
            Ok(vec![])
        }

        fn record_step_result(
            &self,
            req: StepResultRequest,
        ) -> Result<PlanDocument, PlanningError> {
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
                && let Some(step_2) = plan.steps.iter_mut().find(|step| step.step_id == "step-2")
            {
                step_2.status = PlanStepStatus::Ready;
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

    struct MockContext;

    impl ContextProvider for MockContext {
        fn build_context(
            &self,
            req: BuildContextRequest,
        ) -> Result<greentic_dw_context::ContextPackage, ContextError> {
            Ok(greentic_dw_context::ContextPackage {
                package_id: req.fragment_refs.join(","),
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

    struct MockWorkspace;

    impl WorkspaceProvider for MockWorkspace {
        fn create_artifact(
            &self,
            req: CreateArtifactRequest,
        ) -> Result<ArtifactRef, WorkspaceError> {
            Ok(req.artifact)
        }

        fn read_artifact(
            &self,
            _req: greentic_dw_workspace::ReadArtifactRequest,
        ) -> Result<greentic_dw_workspace::ArtifactContent, WorkspaceError> {
            unreachable!()
        }

        fn update_artifact(
            &self,
            _req: greentic_dw_workspace::UpdateArtifactRequest,
        ) -> Result<greentic_dw_workspace::ArtifactVersion, WorkspaceError> {
            unreachable!()
        }

        fn list_artifacts(
            &self,
            _req: greentic_dw_workspace::ListArtifactsRequest,
        ) -> Result<Vec<greentic_dw_workspace::ArtifactSummary>, WorkspaceError> {
            unreachable!()
        }

        fn link_artifacts(
            &self,
            _req: greentic_dw_workspace::LinkArtifactsRequest,
        ) -> Result<(), WorkspaceError> {
            Ok(())
        }
    }

    struct MockReflector {
        verdict: ReviewVerdict,
    }

    impl ReflectionProvider for MockReflector {
        fn review_step(
            &self,
            _req: ReviewStepRequest,
        ) -> Result<greentic_dw_reflection::ReviewOutcome, ReflectionError> {
            Ok(greentic_dw_reflection::ReviewOutcome {
                verdict: self.verdict.clone(),
                score: Some(1.0),
                findings: vec![],
                suggested_actions: vec![],
                binding: false,
            })
        }

        fn review_plan(
            &self,
            _req: greentic_dw_reflection::ReviewPlanRequest,
        ) -> Result<greentic_dw_reflection::ReviewOutcome, ReflectionError> {
            unreachable!()
        }

        fn review_final(
            &self,
            _req: ReviewFinalRequest,
        ) -> Result<greentic_dw_reflection::ReviewOutcome, ReflectionError> {
            Ok(greentic_dw_reflection::ReviewOutcome {
                verdict: ReviewVerdict::Accept,
                score: Some(1.0),
                findings: vec![],
                suggested_actions: vec![],
                binding: false,
            })
        }
    }

    struct MockDelegator;

    impl DelegationProvider for MockDelegator {
        fn choose_delegate(
            &self,
            req: DelegationRequest,
        ) -> Result<DelegationDecision, DelegationError> {
            Ok(DelegationDecision {
                mode: DelegationMode::Single,
                target_agents: if req.candidate_agents.is_empty() {
                    vec!["delegate-a".to_string()]
                } else {
                    req.candidate_agents
                },
                merge_policy: MergePolicy::FirstSuccess,
                rationale: "delegate".to_string(),
            })
        }

        fn start_subtask(
            &self,
            req: StartSubtaskRequest,
        ) -> Result<DelegationHandle, DelegationError> {
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
                summary: String::new(),
            })
        }
    }

    #[test]
    fn deep_loop_executes_two_steps_deterministically() {
        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Operation(
            RuntimeOperation::Step,
        )));
        let coordinator = DeepLoopCoordinator {
            runtime: &runtime,
            planner: &MockPlanner::new(),
            context: &MockContext,
            workspace: &MockWorkspace,
            reflector: &MockReflector {
                verdict: ReviewVerdict::Accept,
            },
            delegator: &MockDelegator,
        };
        let mut envelope = sample_envelope();

        let run = coordinator
            .run(&mut envelope, two_step_plan(PlanStepKind::ToolCall))
            .expect("deep loop should succeed");

        assert_eq!(run.status, DeepLoopStatus::Completed);
        assert_eq!(run.output_artifact_ids.len(), 2);
        assert_eq!(envelope.state, TaskLifecycleState::Completed);
    }

    #[test]
    fn failed_reflection_causes_revision() {
        let planner = MockPlanner::new();
        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Operation(
            RuntimeOperation::Step,
        )));
        let coordinator = DeepLoopCoordinator {
            runtime: &runtime,
            planner: &planner,
            context: &MockContext,
            workspace: &MockWorkspace,
            reflector: &MockReflector {
                verdict: ReviewVerdict::Revise,
            },
            delegator: &MockDelegator,
        };
        let mut envelope = sample_envelope();

        let run = coordinator
            .run(&mut envelope, two_step_plan(PlanStepKind::ToolCall))
            .expect("deep loop should return revision status");

        assert_eq!(run.status, DeepLoopStatus::Revising);
        assert_eq!(run.plan.revision, 2);
    }

    #[test]
    fn delegation_step_emits_subtask_envelope() {
        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Operation(
            RuntimeOperation::Step,
        )));
        let coordinator = DeepLoopCoordinator {
            runtime: &runtime,
            planner: &MockPlanner::new(),
            context: &MockContext,
            workspace: &MockWorkspace,
            reflector: &MockReflector {
                verdict: ReviewVerdict::Accept,
            },
            delegator: &MockDelegator,
        };
        let mut envelope = sample_envelope();

        let run = coordinator
            .run(&mut envelope, two_step_plan(PlanStepKind::Delegate))
            .expect("deep loop should delegate");

        assert!(!run.emitted_subtasks.is_empty());
        assert_eq!(run.emitted_subtasks[0].target_agent, "delegate-a");
        assert_eq!(run.status, DeepLoopStatus::Delegating);
    }
}
