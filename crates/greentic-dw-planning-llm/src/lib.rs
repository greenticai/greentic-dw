//! LLM-backed [`PlanningProvider`] implementation for greentic-dw.
//!
//! [`LlmPlanningProvider`] wraps any [`greentic_llm::LlmProvider`] and exposes
//! it as a synchronous [`PlanningProvider`] via the [`bridge::block_on`]
//! helper.
//!
//! # Method status
//!
//! | Method                | Status                     |
//! |-----------------------|----------------------------|
//! | `record_step_result`  | Implemented (deterministic) |
//! | `evaluate_completion` | Implemented (deterministic) |
//! | `create_plan`         | Stub — Task 2              |
//! | `revise_plan`         | Stub — Task 2              |
//! | `next_actions`        | Stub — Task 2              |

pub mod bridge;

use std::sync::Arc;

use greentic_dw_planning::{
    CompletionCheckRequest, CompletionState, CreatePlanRequest, NextActionsRequest, PlanDocument,
    PlanRevision, PlanStepStatus, PlannedAction, PlanningError, PlanningProvider,
    RevisePlanRequest, StepResultRequest,
};
use greentic_llm::LlmProvider;

/// An [`LlmPlanningProvider`] backed by a [`LlmProvider`].
///
/// The two deterministic methods (`record_step_result`, `evaluate_completion`)
/// run purely in memory.  The LLM-driven methods (`create_plan`, `revise_plan`,
/// `next_actions`) will be wired up in Task 2.
pub struct LlmPlanningProvider {
    /// The underlying LLM provider used for generative plan operations.
    llm: Arc<dyn LlmProvider>,
}

impl LlmPlanningProvider {
    /// Create a new provider wrapping the given `llm`.
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self { llm }
    }

    /// Return a reference to the underlying LLM provider.
    pub fn llm(&self) -> &dyn LlmProvider {
        self.llm.as_ref()
    }
}

/// Returns `true` when a step status is terminal (no further transitions
/// expected in normal flow).
fn is_terminal(status: &PlanStepStatus) -> bool {
    matches!(
        status,
        PlanStepStatus::Completed | PlanStepStatus::Skipped | PlanStepStatus::Failed
    )
}

/// Returns `true` when a step status satisfies a dependency (allows downstream
/// steps to become `Ready`).
fn satisfies_dependency(status: &PlanStepStatus) -> bool {
    matches!(status, PlanStepStatus::Completed | PlanStepStatus::Skipped)
}

impl PlanningProvider for LlmPlanningProvider {
    /// Update a single step's status, then promote any `Pending` steps whose
    /// entire `depends_on` set is now satisfied to `Ready`.
    ///
    /// The input plan is cloned; the original is not mutated.
    fn record_step_result(&self, req: StepResultRequest) -> Result<PlanDocument, PlanningError> {
        let mut updated_plan = req.plan.clone();

        // Set the target step's status.
        for step in &mut updated_plan.steps {
            if step.step_id == req.step_id {
                step.status = req.status.clone();
                break;
            }
        }

        // Build a status lookup map so we can check dependencies without
        // multiple nested borrows.
        let status_by_id: std::collections::HashMap<String, PlanStepStatus> = updated_plan
            .steps
            .iter()
            .map(|step| (step.step_id.clone(), step.status.clone()))
            .collect();

        // Promote Pending steps whose every dependency is now satisfied.
        for step in &mut updated_plan.steps {
            if step.status != PlanStepStatus::Pending {
                continue;
            }
            let all_dependencies_satisfied = step.depends_on.iter().all(|dependency_id| {
                status_by_id
                    .get(dependency_id)
                    .map(satisfies_dependency)
                    .unwrap_or(false)
            });
            if all_dependencies_satisfied {
                step.status = PlanStepStatus::Ready;
            }
        }

        Ok(updated_plan)
    }

    /// Determine whether the plan's goal has been met.
    ///
    /// - `Failed` on any step → [`CompletionState::Unsatisfied`]
    /// - All steps terminal (Completed or Skipped) → [`CompletionState::Satisfied`]
    /// - Otherwise → [`CompletionState::Incomplete`]
    fn evaluate_completion(
        &self,
        req: CompletionCheckRequest,
    ) -> Result<CompletionState, PlanningError> {
        let steps = &req.plan.steps;

        if steps
            .iter()
            .any(|step| step.status == PlanStepStatus::Failed)
        {
            return Ok(CompletionState::Unsatisfied);
        }

        if steps.iter().all(|step| is_terminal(&step.status)) {
            return Ok(CompletionState::Satisfied);
        }

        Ok(CompletionState::Incomplete)
    }

    /// Not yet implemented — will call the LLM to generate a structured plan.
    fn create_plan(&self, _req: CreatePlanRequest) -> Result<PlanDocument, PlanningError> {
        Err(PlanningError::Provider("not implemented".into()))
    }

    /// Not yet implemented — will call the LLM to revise an existing plan.
    fn revise_plan(&self, _req: RevisePlanRequest) -> Result<PlanRevision, PlanningError> {
        Err(PlanningError::Provider("not implemented".into()))
    }

    /// Not yet implemented — will call the LLM to select the next actions.
    fn next_actions(&self, _req: NextActionsRequest) -> Result<Vec<PlannedAction>, PlanningError> {
        Err(PlanningError::Provider("not implemented".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures_util::stream;
    use greentic_dw_planning::{PlanStatus, PlanStep, PlanStepKind};
    use greentic_llm::{
        Capabilities, ChatRequest, ChatResponse, ChatStream, FinishReason, LlmError, StreamEvent,
    };

    // ---------------------------------------------------------------------------
    // StubLlm — minimal LlmProvider for unit tests
    // ---------------------------------------------------------------------------

    struct StubLlm {
        canned_response: ChatResponse,
    }

    impl StubLlm {
        fn with_response(content: impl Into<String>) -> Self {
            Self {
                canned_response: ChatResponse {
                    content: content.into(),
                    tool_calls: vec![],
                    finish_reason: FinishReason::Stop,
                },
            }
        }
    }

    #[async_trait]
    impl LlmProvider for StubLlm {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                chat: true,
                tools: false,
                streaming: false,
                vision: false,
                system_prompt: true,
            }
        }

        fn provider_name(&self) -> &'static str {
            "stub"
        }

        fn model(&self) -> &str {
            "stub-model"
        }

        async fn chat(&self, _req: ChatRequest) -> Result<ChatResponse, LlmError> {
            Ok(ChatResponse {
                content: self.canned_response.content.clone(),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
            })
        }

        async fn chat_stream(&self, _req: ChatRequest) -> Result<ChatStream, LlmError> {
            use futures_util::StreamExt;
            let events: Vec<Result<StreamEvent, LlmError>> = vec![
                Ok(StreamEvent::TextChunk(self.canned_response.content.clone())),
                Ok(StreamEvent::Done {
                    finish_reason: FinishReason::Stop,
                }),
            ];
            Ok(stream::iter(events).boxed())
        }
    }

    // ---------------------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------------------

    fn stub_provider() -> LlmPlanningProvider {
        LlmPlanningProvider::new(Arc::new(StubLlm::with_response("ok")))
    }

    fn minimal_plan(steps: Vec<PlanStep>) -> PlanDocument {
        PlanDocument {
            plan_id: "plan-1".into(),
            goal: "test goal".into(),
            status: PlanStatus::Active,
            revision: 1,
            assumptions: vec![],
            constraints: vec![],
            success_criteria: vec!["done".into()],
            steps,
            edges: vec![],
            metadata: Default::default(),
        }
    }

    fn step(step_id: &str, status: PlanStepStatus, depends_on: Vec<&str>) -> PlanStep {
        PlanStep {
            step_id: step_id.into(),
            title: format!("Step {step_id}"),
            kind: PlanStepKind::ToolCall,
            status,
            depends_on: depends_on.into_iter().map(String::from).collect(),
            assigned_agent: None,
            inputs_schema_ref: None,
            output_schema_ref: None,
            retry_count: 0,
        }
    }

    // ---------------------------------------------------------------------------
    // record_step_result tests
    // ---------------------------------------------------------------------------

    #[test]
    fn record_step_result_updates_target_status() {
        let provider = stub_provider();
        let plan = minimal_plan(vec![
            step("s1", PlanStepStatus::Running, vec![]),
            step("s2", PlanStepStatus::Pending, vec![]),
        ]);
        let req = StepResultRequest {
            plan,
            step_id: "s1".into(),
            status: PlanStepStatus::Completed,
        };

        let updated = provider.record_step_result(req).expect("should succeed");
        let s1 = updated.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert_eq!(s1.status, PlanStepStatus::Completed);
    }

    #[test]
    fn record_step_result_promotes_dependent_to_ready() {
        let provider = stub_provider();
        // s2 depends on s1; completing s1 should make s2 Ready.
        let plan = minimal_plan(vec![
            step("s1", PlanStepStatus::Running, vec![]),
            step("s2", PlanStepStatus::Pending, vec!["s1"]),
        ]);
        let req = StepResultRequest {
            plan,
            step_id: "s1".into(),
            status: PlanStepStatus::Completed,
        };

        let updated = provider.record_step_result(req).expect("should succeed");
        let s2 = updated.steps.iter().find(|s| s.step_id == "s2").unwrap();
        assert_eq!(
            s2.status,
            PlanStepStatus::Ready,
            "s2 must be promoted to Ready after s1 completes"
        );
    }

    #[test]
    fn record_step_result_does_not_promote_when_dependency_unmet() {
        let provider = stub_provider();
        // s3 depends on both s1 and s2; only s1 completes here.
        let plan = minimal_plan(vec![
            step("s1", PlanStepStatus::Running, vec![]),
            step("s2", PlanStepStatus::Pending, vec![]),
            step("s3", PlanStepStatus::Pending, vec!["s1", "s2"]),
        ]);
        let req = StepResultRequest {
            plan,
            step_id: "s1".into(),
            status: PlanStepStatus::Completed,
        };

        let updated = provider.record_step_result(req).expect("should succeed");
        let s3 = updated.steps.iter().find(|s| s.step_id == "s3").unwrap();
        assert_eq!(
            s3.status,
            PlanStepStatus::Pending,
            "s3 must remain Pending while s2 is not yet done"
        );
    }

    #[test]
    fn record_step_result_does_not_mutate_original_plan() {
        let provider = stub_provider();
        let plan = minimal_plan(vec![step("s1", PlanStepStatus::Running, vec![])]);
        let original_status = plan.steps[0].status.clone();
        let req = StepResultRequest {
            plan: plan.clone(),
            step_id: "s1".into(),
            status: PlanStepStatus::Completed,
        };

        let _ = provider.record_step_result(req).expect("should succeed");
        // plan is consumed into req, so the clone above represents the original
        assert_eq!(plan.steps[0].status, original_status);
    }

    #[test]
    fn record_step_result_promotes_with_skipped_dependency() {
        let provider = stub_provider();
        // Skipped is also a terminal satisfying status.
        let plan = minimal_plan(vec![
            step("s1", PlanStepStatus::Running, vec![]),
            step("s2", PlanStepStatus::Pending, vec!["s1"]),
        ]);
        let req = StepResultRequest {
            plan,
            step_id: "s1".into(),
            status: PlanStepStatus::Skipped,
        };

        let updated = provider.record_step_result(req).expect("should succeed");
        let s2 = updated.steps.iter().find(|s| s.step_id == "s2").unwrap();
        assert_eq!(
            s2.status,
            PlanStepStatus::Ready,
            "s2 must be promoted to Ready after s1 is skipped"
        );
    }

    // ---------------------------------------------------------------------------
    // evaluate_completion tests
    // ---------------------------------------------------------------------------

    #[test]
    fn evaluate_completion_satisfied_when_all_terminal() {
        let provider = stub_provider();
        let plan = minimal_plan(vec![
            step("s1", PlanStepStatus::Completed, vec![]),
            step("s2", PlanStepStatus::Skipped, vec![]),
        ]);
        let state = provider
            .evaluate_completion(CompletionCheckRequest { plan })
            .expect("should succeed");
        assert_eq!(state, CompletionState::Satisfied);
    }

    #[test]
    fn evaluate_completion_incomplete_when_steps_still_running() {
        let provider = stub_provider();
        let plan = minimal_plan(vec![
            step("s1", PlanStepStatus::Completed, vec![]),
            step("s2", PlanStepStatus::Running, vec![]),
        ]);
        let state = provider
            .evaluate_completion(CompletionCheckRequest { plan })
            .expect("should succeed");
        assert_eq!(state, CompletionState::Incomplete);
    }

    #[test]
    fn evaluate_completion_unsatisfied_when_any_step_failed() {
        let provider = stub_provider();
        let plan = minimal_plan(vec![
            step("s1", PlanStepStatus::Completed, vec![]),
            step("s2", PlanStepStatus::Failed, vec![]),
        ]);
        let state = provider
            .evaluate_completion(CompletionCheckRequest { plan })
            .expect("should succeed");
        assert_eq!(state, CompletionState::Unsatisfied);
    }

    #[test]
    fn evaluate_completion_unsatisfied_takes_precedence_over_incomplete() {
        let provider = stub_provider();
        // One step is Failed AND another is still Running — Failed wins.
        let plan = minimal_plan(vec![
            step("s1", PlanStepStatus::Failed, vec![]),
            step("s2", PlanStepStatus::Running, vec![]),
        ]);
        let state = provider
            .evaluate_completion(CompletionCheckRequest { plan })
            .expect("should succeed");
        assert_eq!(state, CompletionState::Unsatisfied);
    }

    #[test]
    fn evaluate_completion_satisfied_with_empty_steps() {
        let provider = stub_provider();
        let plan = minimal_plan(vec![]);
        let state = provider
            .evaluate_completion(CompletionCheckRequest { plan })
            .expect("should succeed");
        // All-of-empty is vacuously true.
        assert_eq!(state, CompletionState::Satisfied);
    }

    // ---------------------------------------------------------------------------
    // LLM-stub method stubs return Provider error
    // ---------------------------------------------------------------------------

    #[test]
    fn create_plan_returns_not_implemented() {
        let provider = stub_provider();
        let err = provider
            .create_plan(CreatePlanRequest {
                goal: "test".into(),
                assumptions: vec![],
                constraints: vec![],
                success_criteria: vec![],
            })
            .unwrap_err();
        assert!(matches!(err, PlanningError::Provider(_)));
    }
}
