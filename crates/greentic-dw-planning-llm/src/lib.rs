//! LLM-backed [`PlanningProvider`] implementation for greentic-dw.
//!
//! [`LlmPlanningProvider`] wraps any [`greentic_llm::LlmProvider`] and exposes
//! it as a synchronous [`PlanningProvider`] via the [`bridge::block_on`]
//! helper.
//!
//! # Method status
//!
//! | Method                | Status                      |
//! |-----------------------|-----------------------------|
//! | `record_step_result`  | Implemented (deterministic) |
//! | `evaluate_completion` | Implemented (deterministic) |
//! | `create_plan`         | Implemented (LLM-backed)    |
//! | `revise_plan`         | Implemented (LLM-backed)    |
//! | `next_actions`        | Implemented (LLM-backed)    |

pub mod bridge;
mod prompt;

use std::sync::Arc;

use greentic_dw_planning::validate_plan;
use greentic_dw_planning::{
    CompletionCheckRequest, CompletionState, CreatePlanRequest, NextActionsRequest, PlanDocument,
    PlanRevision, PlanStepStatus, PlannedAction, PlanningError, PlanningProvider,
    RevisePlanRequest, StepResultRequest,
};
use greentic_llm::{ChatMessage, ChatRequest, LlmProvider};

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

    /// Send a one-shot LLM request and deserialize the JSON response into `T`.
    ///
    /// Builds a minimal `ChatRequest` with a system prompt and a single user
    /// message, calls the provider synchronously via [`bridge::block_on`], then
    /// extracts and parses the JSON from the model reply.
    ///
    /// # Errors
    ///
    /// Returns [`PlanningError::Provider`] when the LLM call fails or when the
    /// response cannot be parsed as valid JSON for `T`.
    fn complete_json<T: serde::de::DeserializeOwned>(
        &self,
        system: &str,
        user: String,
    ) -> Result<T, PlanningError> {
        let request = ChatRequest {
            messages: vec![ChatMessage::system(system), ChatMessage::user(user)],
            tools: vec![],
            tool_choice: None,
            max_tokens: Some(4096),
            temperature: Some(0.2),
        };

        let response = bridge::block_on(self.llm.chat(request))
            .map_err(|llm_error| PlanningError::Provider(llm_error.to_string()))?;

        let raw_json = prompt::extract_json(&response.content);

        serde_json::from_str::<T>(raw_json).map_err(|parse_error| {
            PlanningError::Provider(format!("planner LLM output not valid JSON: {parse_error}"))
        })
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

    /// Call the LLM to generate a structured [`PlanDocument`] for the given goal.
    ///
    /// The response is validated with [`validate_plan`] before being returned so
    /// callers always receive a structurally sound plan.
    fn create_plan(&self, req: CreatePlanRequest) -> Result<PlanDocument, PlanningError> {
        let plan: PlanDocument = self.complete_json(
            &prompt::system_for_create_plan(),
            prompt::user_for_create_plan(&req),
        )?;

        validate_plan(&plan)?;

        Ok(plan)
    }

    /// Call the LLM to produce a [`PlanRevision`] summarising what changed and why.
    fn revise_plan(&self, req: RevisePlanRequest) -> Result<PlanRevision, PlanningError> {
        let revision: PlanRevision = self.complete_json(
            &prompt::system_for_revise_plan(),
            prompt::user_for_revise_plan(&req),
        )?;

        Ok(revision)
    }

    /// Call the LLM to select the next [`PlannedAction`]s from the current plan.
    ///
    /// Each returned action must reference a `step_id` that exists in the plan;
    /// unknown step IDs are rejected with [`PlanningError::Validation`].
    fn next_actions(&self, req: NextActionsRequest) -> Result<Vec<PlannedAction>, PlanningError> {
        let actions: Vec<PlannedAction> = self.complete_json(
            &prompt::system_for_next_actions(),
            prompt::user_for_next_actions(&req),
        )?;

        let known_step_ids: std::collections::HashSet<&str> = req
            .plan
            .steps
            .iter()
            .map(|step| step.step_id.as_str())
            .collect();

        for action in &actions {
            if !known_step_ids.contains(action.step_id.as_str()) {
                return Err(PlanningError::Validation(format!(
                    "planner referenced unknown step_id: {}",
                    action.step_id
                )));
            }
        }

        Ok(actions)
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
    // Helpers for LLM-backed method tests
    // ---------------------------------------------------------------------------

    fn stub_with(content: impl Into<String>) -> LlmPlanningProvider {
        LlmPlanningProvider::new(Arc::new(StubLlm::with_response(content)))
    }

    fn valid_plan_json() -> String {
        serde_json::to_string(&serde_json::json!({
            "plan_id": "plan-42",
            "goal": "achieve something",
            "status": "active",
            "revision": 1,
            "assumptions": [],
            "constraints": [],
            "success_criteria": ["criterion-1"],
            "steps": [{
                "step_id": "s1",
                "title": "Do the thing",
                "kind": "tool_call",
                "status": "pending",
                "depends_on": [],
                "retry_count": 0
            }],
            "edges": [],
            "metadata": {}
        }))
        .unwrap()
    }

    fn create_plan_request() -> CreatePlanRequest {
        CreatePlanRequest {
            goal: "achieve something".into(),
            assumptions: vec![],
            constraints: vec![],
            success_criteria: vec!["criterion-1".into()],
        }
    }

    // ---------------------------------------------------------------------------
    // next_actions tests
    // ---------------------------------------------------------------------------

    #[test]
    fn next_actions_parses_and_validates_known_step() {
        let json = r#"[{"step_id":"s1","action":"execute"}]"#;
        let provider = stub_with(json);
        let plan = minimal_plan(vec![step("s1", PlanStepStatus::Ready, vec![])]);
        let req = NextActionsRequest {
            plan,
            context: None,
        };

        let actions = provider.next_actions(req).expect("should succeed");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].step_id, "s1");
        assert_eq!(actions[0].action, "execute");
    }

    #[test]
    fn next_actions_rejects_unknown_step_id() {
        let json = r#"[{"step_id":"ghost","action":"x"}]"#;
        let provider = stub_with(json);
        let plan = minimal_plan(vec![step("s1", PlanStepStatus::Ready, vec![])]);
        let req = NextActionsRequest {
            plan,
            context: None,
        };

        let err = provider.next_actions(req).unwrap_err();
        assert!(
            matches!(err, PlanningError::Validation(_)),
            "expected Validation, got {err:?}"
        );
    }

    #[test]
    fn next_actions_bad_json_is_provider_error() {
        let provider = stub_with("not json at all");
        let plan = minimal_plan(vec![step("s1", PlanStepStatus::Ready, vec![])]);
        let req = NextActionsRequest {
            plan,
            context: None,
        };

        let err = provider.next_actions(req).unwrap_err();
        assert!(
            matches!(err, PlanningError::Provider(_)),
            "expected Provider error, got {err:?}"
        );
    }

    #[test]
    fn next_actions_returns_empty_list_when_llm_says_so() {
        let provider = stub_with("[]");
        let plan = minimal_plan(vec![step("s1", PlanStepStatus::Completed, vec![])]);
        let req = NextActionsRequest {
            plan,
            context: None,
        };

        let actions = provider.next_actions(req).expect("should succeed");
        assert!(actions.is_empty());
    }

    // ---------------------------------------------------------------------------
    // create_plan tests
    // ---------------------------------------------------------------------------

    #[test]
    fn create_plan_parses_and_validates_valid_document() {
        let provider = stub_with(valid_plan_json());
        let result = provider.create_plan(create_plan_request());
        let plan = result.expect("should succeed");
        assert_eq!(plan.plan_id, "plan-42");
        assert_eq!(plan.steps.len(), 1);
    }

    #[test]
    fn create_plan_invalid_plan_is_validation_error() {
        // success_criteria is empty → validate_plan rejects it
        let bad_plan = serde_json::to_string(&serde_json::json!({
            "plan_id": "plan-bad",
            "goal": "something",
            "status": "active",
            "revision": 1,
            "assumptions": [],
            "constraints": [],
            "success_criteria": [],
            "steps": [{
                "step_id": "s1",
                "title": "A step",
                "kind": "tool_call",
                "status": "pending",
                "depends_on": [],
                "retry_count": 0
            }],
            "edges": [],
            "metadata": {}
        }))
        .unwrap();
        let provider = stub_with(bad_plan);
        let err = provider.create_plan(create_plan_request()).unwrap_err();
        assert!(
            matches!(err, PlanningError::Validation(_)),
            "expected Validation error, got {err:?}"
        );
    }

    #[test]
    fn create_plan_bad_json_is_provider_error() {
        let provider = stub_with("not valid json");
        let err = provider.create_plan(create_plan_request()).unwrap_err();
        assert!(
            matches!(err, PlanningError::Provider(_)),
            "expected Provider error, got {err:?}"
        );
    }

    // ---------------------------------------------------------------------------
    // revise_plan tests
    // ---------------------------------------------------------------------------

    #[test]
    fn revise_plan_parses_valid_revision() {
        let revision_json = serde_json::to_string(&serde_json::json!({
            "revision": 2,
            "reason": "adjusted scope",
            "changed_step_ids": ["s1"],
            "metadata": {}
        }))
        .unwrap();
        let provider = stub_with(revision_json);
        let plan = minimal_plan(vec![step("s1", PlanStepStatus::Ready, vec![])]);
        let req = RevisePlanRequest {
            plan,
            reason: "adjusted scope".into(),
            context: None,
        };

        let revision = provider.revise_plan(req).expect("should succeed");
        assert_eq!(revision.revision, 2);
        assert_eq!(revision.reason, "adjusted scope");
        assert_eq!(revision.changed_step_ids, vec!["s1"]);
    }

    #[test]
    fn revise_plan_bad_json_is_provider_error() {
        let provider = stub_with("not json");
        let plan = minimal_plan(vec![step("s1", PlanStepStatus::Ready, vec![])]);
        let req = RevisePlanRequest {
            plan,
            reason: "reason".into(),
            context: None,
        };

        let err = provider.revise_plan(req).unwrap_err();
        assert!(
            matches!(err, PlanningError::Provider(_)),
            "expected Provider error, got {err:?}"
        );
    }

    // ---------------------------------------------------------------------------
    // extract_json tests (via the public prompt module)
    // ---------------------------------------------------------------------------

    #[test]
    fn next_actions_handles_fenced_json() {
        let fenced = "```json\n[{\"step_id\":\"s1\",\"action\":\"go\"}]\n```";
        let provider = stub_with(fenced);
        let plan = minimal_plan(vec![step("s1", PlanStepStatus::Ready, vec![])]);
        let req = NextActionsRequest {
            plan,
            context: None,
        };
        let actions = provider
            .next_actions(req)
            .expect("should parse fenced JSON");
        assert_eq!(actions[0].step_id, "s1");
    }
}
