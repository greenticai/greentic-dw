use greentic_dw_context::{BuildContextRequest, ContextBudget, ContextError, ContextProvider};
use greentic_dw_core::RuntimeEvent;
use greentic_dw_delegation::{
    DelegationDecision, DelegationError, DelegationMode, DelegationProvider, DelegationRequest,
    HandoffContextScope, HandoffReturnPolicy, MergePolicy, StartSubtaskRequest, SubtaskEnvelope,
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

use crate::step_executor::{ExecuteStepRequest, StepExecutionError, StepExecutor, ToolBudget};
use crate::{DwRuntime, RuntimeError};

/// Iteration cap used by callers that do not choose one. Kept at 64 so every
/// caller predating the configurable cap behaves exactly as before.
pub const DEFAULT_MAX_ITERATIONS: usize = 64;

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
    /// The iteration cap was reached before the plan reached a terminal state.
    /// An outcome, not an error: the run carries the partial plan.
    BudgetExhausted,
}

impl std::fmt::Display for DeepLoopStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let token = match self {
            DeepLoopStatus::Idle => "idle",
            DeepLoopStatus::Planning => "planning",
            DeepLoopStatus::Executing => "executing",
            DeepLoopStatus::Reflecting => "reflecting",
            DeepLoopStatus::Revising => "revising",
            DeepLoopStatus::Delegating => "delegating",
            DeepLoopStatus::Completed => "completed",
            DeepLoopStatus::Failed => "failed",
            DeepLoopStatus::BudgetExhausted => "budget_exhausted",
        };
        f.write_str(token)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepLoopRun {
    pub plan: PlanDocument,
    pub status: DeepLoopStatus,
    pub emitted_subtasks: Vec<SubtaskEnvelope>,
    pub output_artifact_ids: Vec<String>,
    /// Tool calls charged to the run's tool budget. Always `0` without a
    /// [`StepExecutor`].
    pub tool_calls_used: usize,
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
    #[error(transparent)]
    Execution(#[from] StepExecutionError),
    #[error("plan step `{step_id}` not found")]
    MissingStep { step_id: String },
    /// No longer produced by [`DeepLoopCoordinator::run`], which reports the cap
    /// as [`DeepLoopStatus::BudgetExhausted`]. Kept so downstream matches compile.
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
    /// Maximum number of planner iterations (one `next_actions` call each).
    /// Use [`DEFAULT_MAX_ITERATIONS`] for the historical behaviour.
    pub max_iterations: usize,
    /// Carries out non-delegate steps. `None` keeps the historical behaviour:
    /// a runtime tick and a placeholder artifact body. When set, the run gets a
    /// tool-call budget of `max_iterations * TOOL_CALLS_PER_ITERATION`, at most
    /// [`crate::PER_STEP_TOOL_CAP`] per step.
    pub executor: Option<&'a dyn StepExecutor>,
}

impl<'a, E: DwEngine> DeepLoopCoordinator<'a, E> {
    /// Build a context package for `query` (deep-worker RAG) and render its
    /// inline-content fragments to a `<knowledge>` block. Returns `None` when no
    /// renderable context is produced (e.g. no knowledge provider, or no hits),
    /// so callers thread `Option<String>` straight into the planning/reflection
    /// request DTOs. A knowledge-aware `ContextProvider` fails retrieval open
    /// (returns an empty package), so context-system errors still propagate here.
    fn rendered_context(
        &self,
        query: &str,
        fragment_ref: &str,
    ) -> Result<Option<String>, DeepLoopError> {
        let package = self.context.build_context(BuildContextRequest {
            fragment_refs: vec![fragment_ref.to_string()],
            query: Some(query.to_string()),
            budget: ContextBudget {
                max_fragments: 8,
                max_bytes: 16_384,
            },
        })?;
        let rendered = greentic_dw_context::render_context(&package);
        Ok((!rendered.is_empty()).then_some(rendered))
    }

    pub fn run(
        &self,
        envelope: &mut TaskEnvelope,
        mut plan: PlanDocument,
    ) -> Result<DeepLoopRun, DeepLoopError> {
        let mut emitted_subtasks = Vec::new();
        let mut output_artifact_ids = Vec::new();
        // Per-run monotonic sequence so a step re-executed across iterations
        // (real planners revisit steps) gets a fresh artifact id instead of
        // colliding on a duplicate `create_artifact`.
        let mut artifact_seq: u32 = 0;
        let mut tool_budget = ToolBudget::for_iterations(self.max_iterations);
        // Bodies of executed steps, handed to later steps as prior outputs.
        let mut step_outputs: Vec<serde_json::Value> = Vec::new();

        if matches!(
            envelope.state,
            greentic_dw_types::TaskLifecycleState::Created
        ) {
            self.runtime.start(envelope)?;
        }

        for _ in 0..self.max_iterations {
            // Plan-level knowledge grounding (deep-worker RAG): retrieve against
            // the plan goal and inject into the planner and final-review prompts.
            let plan_context = self.rendered_context(&plan.goal, &plan.plan_id)?;

            let next_actions = self.planner.next_actions(NextActionsRequest {
                plan: plan.clone(),
                context: plan_context.clone(),
            })?;

            if next_actions.is_empty() {
                if !emitted_subtasks.is_empty() {
                    return Ok(DeepLoopRun {
                        plan,
                        status: DeepLoopStatus::Delegating,
                        emitted_subtasks,
                        output_artifact_ids,
                        tool_calls_used: tool_budget.used(),
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
                            context: plan_context.clone(),
                        })?;
                        outcome.validate()?;
                        if matches!(outcome.verdict, ReviewVerdict::Fail) {
                            self.runtime.fail(envelope, "final review failed")?;
                            return Ok(DeepLoopRun {
                                plan,
                                status: DeepLoopStatus::Failed,
                                emitted_subtasks,
                                output_artifact_ids,
                                tool_calls_used: tool_budget.used(),
                            });
                        }
                        self.runtime.complete(envelope)?;
                        return Ok(DeepLoopRun {
                            plan,
                            status: DeepLoopStatus::Completed,
                            emitted_subtasks,
                            output_artifact_ids,
                            tool_calls_used: tool_budget.used(),
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
                            tool_calls_used: tool_budget.used(),
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

                // Per-step knowledge grounding (deep-worker RAG): retrieve against
                // the step title and inject into the step-review prompt.
                let step_context = self.rendered_context(&step.title, &step.step_id)?;

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
                        let body = match self.executor {
                            None => format!("{{\"step_id\":\"{}\"}}", step.step_id),
                            Some(executor) => {
                                let granted = tool_budget.grant();
                                let execution = executor.execute_step(ExecuteStepRequest {
                                    goal: &plan.goal,
                                    step: &step,
                                    context: step_context.as_deref(),
                                    prior_outputs: &step_outputs,
                                    max_tool_calls: granted,
                                })?;
                                tool_budget.charge(granted, execution.tool_calls_used);
                                let body = execution.body.to_string();
                                step_outputs.push(execution.body);
                                body
                            }
                        };
                        let artifact_ref =
                            self.workspace.create_artifact(CreateArtifactRequest {
                                artifact: ArtifactRef {
                                    artifact_id: format!(
                                        "artifact://{}/{}/{}",
                                        plan.plan_id, step.step_id, artifact_seq
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
                                body,
                            })?;
                        output_artifact_ids.push(artifact_ref.artifact_id.clone());
                        artifact_seq += 1;

                        let review = self.reflector.review_step(ReviewStepRequest {
                            plan_step_id: step.step_id.clone(),
                            output_artifact_ref: artifact_ref.artifact_id,
                            context: step_context.clone(),
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
                                    context: step_context.clone(),
                                })?;
                                plan.revision = revision.revision;
                                return Ok(DeepLoopRun {
                                    plan,
                                    status: DeepLoopStatus::Revising,
                                    emitted_subtasks,
                                    output_artifact_ids,
                                    tool_calls_used: tool_budget.used(),
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
                                    tool_calls_used: tool_budget.used(),
                                });
                            }
                        }
                    }
                }
            }
        }

        // The budget ran out before the plan reached a terminal state. Report it
        // as a status so the caller can still describe what was done. The
        // envelope keeps its lifecycle state on purpose: the work was cut short,
        // not failed.
        Ok(DeepLoopRun {
            plan,
            status: DeepLoopStatus::BudgetExhausted,
            emitted_subtasks,
            output_artifact_ids,
            tool_calls_used: tool_budget.used(),
        })
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
        correlation_id: format!("{}::{}::delegation", envelope.task_id, step.step_id),
        source_agent_id: envelope.worker_id.clone(),
        target_agent,
        tool_id: step
            .assigned_agent
            .as_ref()
            .map(|agent| format!("{}_delegate", agent))
            .unwrap_or_else(|| "delegate".to_string()),
        goal: step.title.clone(),
        context_package_ref: format!("context://{}", step.step_id),
        context_scope: HandoffContextScope::ParentTaskOnly,
        expected_output_schema: step
            .output_schema_ref
            .clone()
            .unwrap_or_else(|| "schema://step-output".to_string()),
        permissions_profile: "restricted".to_string(),
        deadline: "2026-04-16T00:00:00Z".to_string(),
        return_policy: match decision.merge_policy {
            MergePolicy::CollectAll => HandoffReturnPolicy::CollectAll,
            _ => HandoffReturnPolicy::FirstReturn,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_loop_status_display_is_stable_lowercase() {
        assert_eq!(DeepLoopStatus::Completed.to_string(), "completed");
        assert_eq!(DeepLoopStatus::Failed.to_string(), "failed");
        assert_eq!(DeepLoopStatus::Planning.to_string(), "planning");
        assert_eq!(DeepLoopStatus::Idle.to_string(), "idle");
    }
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

    /// A `ContextProvider` that returns one inline knowledge chunk whenever the
    /// request carries a query — the deep-worker RAG analogue used to prove the
    /// rendered context reaches the planning/reflection prompts.
    struct KnowledgeContext;

    impl ContextProvider for KnowledgeContext {
        fn build_context(
            &self,
            req: BuildContextRequest,
        ) -> Result<greentic_dw_context::ContextPackage, ContextError> {
            let fragments = if req.query.is_some() {
                vec![greentic_dw_context::ContextFragment {
                    fragment_id: "k0".to_string(),
                    kind: greentic_dw_context::ContextFragmentKind::KnowledgeChunk,
                    content_ref: String::new(),
                    content: Some("Refunds are processed within 5 business days.".to_string()),
                    provenance: "knowledge".to_string(),
                    ordinal: 0,
                }]
            } else {
                vec![]
            };
            Ok(greentic_dw_context::ContextPackage {
                package_id: req.fragment_refs.join(","),
                fragments,
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

    /// Records the `context` field of every `review_step` request it receives.
    struct CapturingReflector {
        step_contexts: std::sync::Mutex<Vec<Option<String>>>,
    }

    impl ReflectionProvider for CapturingReflector {
        fn review_step(
            &self,
            req: ReviewStepRequest,
        ) -> Result<greentic_dw_reflection::ReviewOutcome, ReflectionError> {
            self.step_contexts
                .lock()
                .expect("lock")
                .push(req.context.clone());
            Ok(greentic_dw_reflection::ReviewOutcome {
                verdict: ReviewVerdict::Accept,
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

    #[test]
    fn knowledge_context_threads_rendered_block_into_step_review() {
        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Operation(
            RuntimeOperation::Step,
        )));
        let reflector = CapturingReflector {
            step_contexts: std::sync::Mutex::new(Vec::new()),
        };
        let coordinator = DeepLoopCoordinator {
            runtime: &runtime,
            planner: &MockPlanner::new(),
            context: &KnowledgeContext,
            workspace: &MockWorkspace,
            reflector: &reflector,
            delegator: &MockDelegator,
            max_iterations: DEFAULT_MAX_ITERATIONS,
            executor: None,
        };
        let mut envelope = sample_envelope();

        coordinator
            .run(&mut envelope, two_step_plan(PlanStepKind::ToolCall))
            .expect("deep loop should succeed");

        let captured = reflector.step_contexts.lock().expect("lock");
        assert!(!captured.is_empty(), "review_step was never called");
        assert!(
            captured.iter().any(|c| {
                c.as_deref().is_some_and(|s| {
                    s.contains("<knowledge>")
                        && s.contains("Refunds are processed within 5 business days.")
                })
            }),
            "review_step should receive the rendered knowledge block, got {captured:?}"
        );
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
            max_iterations: DEFAULT_MAX_ITERATIONS,
            executor: None,
        };
        let mut envelope = sample_envelope();

        let run = coordinator
            .run(&mut envelope, two_step_plan(PlanStepKind::ToolCall))
            .expect("deep loop should succeed");

        assert_eq!(run.status, DeepLoopStatus::Completed);
        assert_eq!(run.output_artifact_ids.len(), 2);
        assert_eq!(envelope.state, TaskLifecycleState::Completed);
    }

    /// Regression: a real LLM plan can revisit the same step across iterations.
    /// The loop must give each execution a distinct artifact id instead of
    /// failing on a duplicate `create_artifact` (live DeepSeek hit
    /// "artifact already exists: artifact://.../step-5"). Uses a workspace that
    /// rejects duplicate ids (like `InMemoryWorkspaceProvider`) and a planner
    /// that emits `step-1` twice before completing.
    #[test]
    fn re_executed_step_gets_distinct_artifact_ids() {
        use std::collections::HashSet;
        use std::sync::Mutex;

        struct DupRejectingWorkspace {
            ids: Mutex<HashSet<String>>,
        }
        impl WorkspaceProvider for DupRejectingWorkspace {
            fn create_artifact(
                &self,
                req: CreateArtifactRequest,
            ) -> Result<ArtifactRef, WorkspaceError> {
                let id = req.artifact.artifact_id.clone();
                if !self.ids.lock().expect("lock").insert(id.clone()) {
                    return Err(WorkspaceError::Provider(format!(
                        "artifact already exists: {id}"
                    )));
                }
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

        struct ReExecutingPlanner {
            executions: Mutex<u32>,
        }
        impl PlanningProvider for ReExecutingPlanner {
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
                unreachable!()
            }
            fn next_actions(
                &self,
                _req: NextActionsRequest,
            ) -> Result<Vec<PlannedAction>, PlanningError> {
                // Emit step-1 for the first two iterations, then stop.
                if *self.executions.lock().expect("lock") < 2 {
                    Ok(vec![PlannedAction {
                        step_id: "step-1".to_string(),
                        action: "execute".to_string(),
                    }])
                } else {
                    Ok(vec![])
                }
            }
            fn record_step_result(
                &self,
                req: StepResultRequest,
            ) -> Result<PlanDocument, PlanningError> {
                *self.executions.lock().expect("lock") += 1;
                Ok(req.plan)
            }
            fn evaluate_completion(
                &self,
                _req: CompletionCheckRequest,
            ) -> Result<CompletionState, PlanningError> {
                if *self.executions.lock().expect("lock") >= 2 {
                    Ok(CompletionState::Satisfied)
                } else {
                    Ok(CompletionState::Incomplete)
                }
            }
        }

        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Operation(
            RuntimeOperation::Step,
        )));
        let workspace = DupRejectingWorkspace {
            ids: Mutex::new(HashSet::new()),
        };
        let coordinator = DeepLoopCoordinator {
            runtime: &runtime,
            planner: &ReExecutingPlanner {
                executions: Mutex::new(0),
            },
            context: &MockContext,
            workspace: &workspace,
            reflector: &MockReflector {
                verdict: ReviewVerdict::Accept,
            },
            delegator: &MockDelegator,
            max_iterations: DEFAULT_MAX_ITERATIONS,
            executor: None,
        };
        let mut envelope = sample_envelope();

        let run = coordinator
            .run(&mut envelope, two_step_plan(PlanStepKind::ToolCall))
            .expect("re-executed step must not collide on artifact id");

        assert_eq!(run.status, DeepLoopStatus::Completed);
        // step-1 executed twice -> two distinct artifact ids, no duplicate-create error.
        assert_eq!(run.output_artifact_ids.len(), 2);
        let unique: HashSet<&String> = run.output_artifact_ids.iter().collect();
        assert_eq!(
            unique.len(),
            2,
            "artifact ids must be distinct per execution"
        );
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
            max_iterations: DEFAULT_MAX_ITERATIONS,
            executor: None,
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
            max_iterations: DEFAULT_MAX_ITERATIONS,
            executor: None,
        };
        let mut envelope = sample_envelope();

        let run = coordinator
            .run(&mut envelope, two_step_plan(PlanStepKind::Delegate))
            .expect("deep loop should delegate");

        assert!(!run.emitted_subtasks.is_empty());
        let emitted = &run.emitted_subtasks[0];
        assert_eq!(emitted.target_agent, "delegate-a");
        assert_eq!(emitted.source_agent_id, envelope.worker_id);
        assert_eq!(emitted.tool_id, "delegate-a_delegate");
        assert!(!emitted.correlation_id.is_empty());
        assert_eq!(run.status, DeepLoopStatus::Delegating);
    }

    #[test]
    fn deep_loop_status_budget_exhausted_displays_snake_case() {
        assert_eq!(
            DeepLoopStatus::BudgetExhausted.to_string(),
            "budget_exhausted"
        );
    }

    #[test]
    fn default_iteration_cap_is_unchanged_for_existing_callers() {
        assert_eq!(DEFAULT_MAX_ITERATIONS, 64);
    }

    /// Reaching the cap is an outcome of the run, not a fault in it: the caller
    /// gets the partial plan back and can still describe what was done.
    #[test]
    fn reaching_the_iteration_cap_returns_budget_exhausted_not_an_error() {
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
            max_iterations: 1,
            executor: None,
        };
        let mut envelope = sample_envelope();

        let run = coordinator
            .run(&mut envelope, two_step_plan(PlanStepKind::ToolCall))
            .expect("budget exhaustion must be Ok(run), not Err");

        assert_eq!(run.status, DeepLoopStatus::BudgetExhausted);
        // MockPlanner offers step-1 in iteration 1 and step-2 only in iteration 2.
        assert_eq!(run.output_artifact_ids.len(), 1);
        let step_1 = run
            .plan
            .steps
            .iter()
            .find(|step| step.step_id == "step-1")
            .expect("step-1 present");
        assert_eq!(step_1.status, PlanStepStatus::Completed);
        assert_ne!(envelope.state, TaskLifecycleState::Completed);
    }

    #[test]
    fn a_zero_iteration_cap_runs_no_step() {
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
            max_iterations: 0,
            executor: None,
        };
        let mut envelope = sample_envelope();

        let run = coordinator
            .run(&mut envelope, two_step_plan(PlanStepKind::ToolCall))
            .expect("a zero cap is still an outcome");

        assert_eq!(run.status, DeepLoopStatus::BudgetExhausted);
        assert!(run.output_artifact_ids.is_empty());
    }

    /// Records every artifact body the loop writes.
    #[derive(Default)]
    struct BodyRecordingWorkspace {
        bodies: Mutex<Vec<String>>,
    }

    impl WorkspaceProvider for BodyRecordingWorkspace {
        fn create_artifact(
            &self,
            req: CreateArtifactRequest,
        ) -> Result<ArtifactRef, WorkspaceError> {
            self.bodies.lock().expect("lock").push(req.body);
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

    /// What a [`RecordingExecutor`] saw for one step.
    #[derive(Debug, Clone, PartialEq)]
    struct SeenRequest {
        step_id: String,
        goal: String,
        prior_outputs: Vec<serde_json::Value>,
        max_tool_calls: usize,
    }

    /// Returns `{"step_id", "answer"}` and reports `reported_calls` tool calls
    /// (or the full grant when `None`).
    struct RecordingExecutor {
        seen: Mutex<Vec<SeenRequest>>,
        reported_calls: Option<usize>,
        fail: bool,
    }

    impl RecordingExecutor {
        fn new(reported_calls: Option<usize>) -> Self {
            Self {
                seen: Mutex::new(Vec::new()),
                reported_calls,
                fail: false,
            }
        }

        fn seen(&self) -> Vec<SeenRequest> {
            self.seen.lock().expect("lock").clone()
        }
    }

    impl StepExecutor for RecordingExecutor {
        fn execute_step(
            &self,
            request: ExecuteStepRequest<'_>,
        ) -> Result<crate::StepExecution, StepExecutionError> {
            self.seen.lock().expect("lock").push(SeenRequest {
                step_id: request.step.step_id.clone(),
                goal: request.goal.to_string(),
                prior_outputs: request.prior_outputs.to_vec(),
                max_tool_calls: request.max_tool_calls,
            });
            if self.fail {
                return Err(StepExecutionError::Backend("model unreachable".into()));
            }
            Ok(crate::StepExecution {
                body: serde_json::json!({
                    "step_id": request.step.step_id,
                    "answer": format!("did {}", request.step.title),
                }),
                tool_calls_used: self.reported_calls.unwrap_or(request.max_tool_calls),
            })
        }
    }

    fn coordinator_with<'a>(
        runtime: &'a DwRuntime<StaticEngine>,
        planner: &'a MockPlanner,
        workspace: &'a dyn WorkspaceProvider,
        max_iterations: usize,
        executor: Option<&'a dyn StepExecutor>,
    ) -> DeepLoopCoordinator<'a, StaticEngine> {
        DeepLoopCoordinator {
            runtime,
            planner,
            context: &MockContext,
            workspace,
            reflector: &MockReflector {
                verdict: ReviewVerdict::Accept,
            },
            delegator: &MockDelegator,
            max_iterations,
            executor,
        }
    }

    fn step_runtime() -> DwRuntime<StaticEngine> {
        DwRuntime::new(StaticEngine::new(EngineDecision::Operation(
            RuntimeOperation::Step,
        )))
    }

    #[test]
    fn executed_bodies_replace_the_placeholder_and_flow_to_later_steps() {
        let runtime = step_runtime();
        let planner = MockPlanner::new();
        let workspace = BodyRecordingWorkspace::default();
        let executor = RecordingExecutor::new(Some(0));
        let coordinator = coordinator_with(
            &runtime,
            &planner,
            &workspace,
            DEFAULT_MAX_ITERATIONS,
            Some(&executor),
        );
        let mut envelope = sample_envelope();

        let run = coordinator
            .run(&mut envelope, two_step_plan(PlanStepKind::ToolCall))
            .expect("deep loop should succeed");

        assert_eq!(run.status, DeepLoopStatus::Completed);
        let first = serde_json::json!({"step_id": "step-1", "answer": "did First"});
        let second = serde_json::json!({"step_id": "step-2", "answer": "did Second"});
        assert_eq!(
            *workspace.bodies.lock().expect("lock"),
            vec![first.to_string(), second.to_string()]
        );
        let seen = executor.seen();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].goal, "Do the work");
        assert!(seen[0].prior_outputs.is_empty());
        assert_eq!(seen[1].step_id, "step-2");
        assert_eq!(seen[1].prior_outputs, vec![first]);
    }

    #[test]
    fn the_total_tool_budget_is_shared_across_steps() {
        let runtime = step_runtime();
        let planner = MockPlanner::new();
        // 3 iterations -> 9 tool calls in total; each step uses its full grant.
        let executor = RecordingExecutor::new(None);
        let coordinator = coordinator_with(&runtime, &planner, &MockWorkspace, 3, Some(&executor));
        let mut envelope = sample_envelope();

        let run = coordinator
            .run(&mut envelope, two_step_plan(PlanStepKind::ToolCall))
            .expect("deep loop should succeed");

        assert_eq!(run.status, DeepLoopStatus::Completed);
        let grants: Vec<usize> = executor.seen().iter().map(|s| s.max_tool_calls).collect();
        assert_eq!(grants, vec![crate::PER_STEP_TOOL_CAP, 3]);
        assert_eq!(run.tool_calls_used, 9);
    }

    #[test]
    fn an_exhausted_tool_budget_grants_zero_and_is_not_an_error() {
        let runtime = step_runtime();
        let planner = MockPlanner::new();
        // 2 iterations -> 6 tool calls: step-1 spends them all, step-2 gets 0.
        let executor = RecordingExecutor::new(Some(usize::MAX));
        let coordinator = coordinator_with(&runtime, &planner, &MockWorkspace, 2, Some(&executor));
        let mut envelope = sample_envelope();

        let run = coordinator
            .run(&mut envelope, two_step_plan(PlanStepKind::ToolCall))
            .expect("an exhausted tool budget is not an error");

        let grants: Vec<usize> = executor.seen().iter().map(|s| s.max_tool_calls).collect();
        assert_eq!(grants, vec![crate::PER_STEP_TOOL_CAP, 0]);
        // Over-reporting is clamped to the grant.
        assert_eq!(run.tool_calls_used, crate::PER_STEP_TOOL_CAP);
        // The iteration rule still ends the run.
        assert_eq!(run.status, DeepLoopStatus::BudgetExhausted);
    }

    #[test]
    fn without_an_executor_the_run_is_unchanged() {
        let runtime = step_runtime();
        let planner = MockPlanner::new();
        let workspace = BodyRecordingWorkspace::default();
        let coordinator =
            coordinator_with(&runtime, &planner, &workspace, DEFAULT_MAX_ITERATIONS, None);
        let mut envelope = sample_envelope();

        let run = coordinator
            .run(&mut envelope, two_step_plan(PlanStepKind::ToolCall))
            .expect("deep loop should succeed");

        assert_eq!(run.status, DeepLoopStatus::Completed);
        assert_eq!(run.tool_calls_used, 0);
        assert_eq!(
            run.output_artifact_ids,
            vec![
                "artifact://plan-1/step-1/0".to_string(),
                "artifact://plan-1/step-2/1".to_string()
            ]
        );
        assert_eq!(
            *workspace.bodies.lock().expect("lock"),
            vec![
                r#"{"step_id":"step-1"}"#.to_string(),
                r#"{"step_id":"step-2"}"#.to_string()
            ]
        );
    }

    #[test]
    fn a_failing_executor_is_an_execution_error() {
        let runtime = step_runtime();
        let planner = MockPlanner::new();
        let mut executor = RecordingExecutor::new(None);
        executor.fail = true;
        let coordinator = coordinator_with(
            &runtime,
            &planner,
            &MockWorkspace,
            DEFAULT_MAX_ITERATIONS,
            Some(&executor),
        );
        let mut envelope = sample_envelope();

        let error = coordinator
            .run(&mut envelope, two_step_plan(PlanStepKind::ToolCall))
            .expect_err("an executor failure is a loop error");

        assert!(matches!(error, DeepLoopError::Execution(_)), "{error:?}");
    }
}
