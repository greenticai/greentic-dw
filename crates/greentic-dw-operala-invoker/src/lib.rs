//! Production [`OperalaDispatchInvoker`]: wires the five deep-worker providers
//! into a [`DeepLoopCoordinator`], runs it on a blocking thread, and writes a
//! prose `reply`. Honours the designer's `deep_worker` settings (see `settings`).

mod guards;
mod reply;
mod settings;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use greentic_dw_context_llm::LlmContextProvider;
use greentic_dw_core::RuntimeOperation;
use greentic_dw_delegation::DelegationProvider;
use greentic_dw_delegation_llm::LlmDelegationProvider;
use greentic_dw_engine::{EngineDecision, StaticEngine};
use greentic_dw_operala_bridge::{InvokeOutcome, OperalaDispatchInvoker};
use greentic_dw_planning::CreatePlanRequest;
use greentic_dw_planning::PlanningProvider;
use greentic_dw_planning_llm::LlmPlanningProvider;
use greentic_dw_reflection::ReflectionProvider;
use greentic_dw_reflection_llm::LlmReflectionProvider;
use greentic_dw_runtime::{DeepLoopCoordinator, DeepLoopRun, DeepLoopStatus, DwRuntime};
use greentic_dw_types::{
    LocaleContext, LocalePropagation, OutputLocaleGuidance, TaskEnvelope, TaskLifecycleState,
    TenantScope, WorkerLocalePolicy,
};
use greentic_dw_workspace::WorkspaceScope;
use greentic_dw_workspace_mem::InMemoryWorkspaceProvider;
use greentic_llm::LlmProvider;

use crate::guards::{
    AcceptAllReflection, NO_DELEGATION_CONSTRAINT, NoDelegateVerdict, RefuseDelegation,
    strip_delegate_steps,
};
use crate::reply::synthesize_reply;
use crate::settings::{ResolvedSettings, settings_from_input};

const DEFAULT_GOAL: &str = "Execute the requested task";
const FALLBACK_TASK_ID: &str = "task-unknown";

/// Production invoker. Each dispatch builds fresh providers + an in-memory
/// workspace and runs one deep loop.
pub struct DeepWorkerInvoker {
    llm: Arc<dyn LlmProvider>,
}

impl DeepWorkerInvoker {
    /// Create an invoker over the configured LLM.
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self { llm }
    }
}

/// Pick the goal from the dispatch input: `goal`, then `user_text`, then a default.
fn extract_goal(input: &Value) -> String {
    input
        .get("goal")
        .and_then(Value::as_str)
        .or_else(|| input.get("user_text").and_then(Value::as_str))
        .unwrap_or(DEFAULT_GOAL)
        .to_string()
}

/// Build a `Created` task envelope from dispatch metadata.
fn build_envelope(tenant: &str, target: &str, task_id: &str) -> TaskEnvelope {
    TaskEnvelope {
        task_id: task_id.to_string(),
        worker_id: target.to_string(),
        state: TaskLifecycleState::Created,
        scope: TenantScope {
            tenant: tenant.to_string(),
            team: None,
        },
        locale: LocaleContext {
            worker_default_locale: "en-US".to_string(),
            requested_locale: None,
            human_locale: None,
            policy: WorkerLocalePolicy::PreferRequested,
            propagation: LocalePropagation::PropagateToDelegates,
            output: OutputLocaleGuidance::MatchRequested,
        },
    }
}

/// Validate the dispatch operation. Empty or "run" (case-insensitive) selects
/// the default deep loop; anything else is rejected so callers get feedback
/// instead of a silently-ignored operation.
fn validate_operation(operation: &str) -> anyhow::Result<()> {
    let op = operation.trim();
    if op.is_empty() || op.eq_ignore_ascii_case("run") {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "unsupported operala operation: {operation:?} (use \"\" or \"run\")"
        ))
    }
}

/// The plan request. With delegation off the planner is told not to delegate;
/// with it on (or no settings at all) the request is exactly what it always was.
fn plan_request(goal: String, delegation: bool) -> CreatePlanRequest {
    let constraints = if delegation {
        vec![]
    } else {
        vec![NO_DELEGATION_CONSTRAINT.to_string()]
    };
    CreatePlanRequest {
        goal,
        assumptions: vec![],
        constraints,
        success_criteria: vec!["task completed".to_string()],
    }
}

/// Map a finished deep-loop run to the bridge's `InvokeOutcome`. `ok` is true
/// only for `Completed`; `reply` is always prose (see [`reply`]).
fn outcome_from_run(run: &DeepLoopRun, operation: &str, reply: String) -> InvokeOutcome {
    InvokeOutcome {
        ok: matches!(run.status, DeepLoopStatus::Completed),
        output: json!({
            "status": run.status.to_string(),
            "operation": operation,
            "artifact_ids": run.output_artifact_ids,
            "reply": reply,
        }),
        events: vec![],
    }
}

#[async_trait]
impl OperalaDispatchInvoker for DeepWorkerInvoker {
    async fn invoke(
        &self,
        tenant: &str,
        _env: &str,
        target: &str,
        operation: &str,
        input: Value,
        idempotency_key: Option<&str>,
    ) -> Result<InvokeOutcome> {
        validate_operation(operation)?;
        let settings = settings_from_input(&input)?;
        let resolved = ResolvedSettings::resolve(settings.as_ref());

        let llm = Arc::clone(&self.llm);
        let tenant = tenant.to_string();
        let target = target.to_string();
        let task_id = idempotency_key.unwrap_or(FALLBACK_TASK_ID).to_string();
        let goal = extract_goal(&input);
        let loop_goal = goal.clone();
        let operation = operation.to_string();

        let run = tokio::task::spawn_blocking(move || -> Result<DeepLoopRun> {
            let workspace: Arc<InMemoryWorkspaceProvider> =
                Arc::new(InMemoryWorkspaceProvider::new());
            let scope = WorkspaceScope {
                tenant: tenant.clone(),
                team: None,
                session: task_id.clone(),
                agent: Some(target.clone()),
                run: task_id.clone(),
            };

            let planner = LlmPlanningProvider::new(Arc::clone(&llm));
            let llm_reflector = LlmReflectionProvider::new(Arc::clone(&llm));
            let llm_delegator = LlmDelegationProvider::new(Arc::clone(&llm));
            let ws_dyn: Arc<dyn greentic_dw_workspace::WorkspaceProvider> = workspace.clone();
            let context = LlmContextProvider::new(Arc::clone(&llm), ws_dyn, scope);

            let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Operation(
                RuntimeOperation::Step,
            )));

            let mut plan = planner.create_plan(plan_request(loop_goal, resolved.delegation))?;
            if !resolved.delegation {
                strip_delegate_steps(&mut plan);
            }

            let mut envelope = build_envelope(&tenant, &target, &task_id);

            let no_delegate_verdicts = NoDelegateVerdict {
                inner: &llm_reflector,
            };
            let reflector: &dyn ReflectionProvider =
                match (resolved.reflection, resolved.delegation) {
                    (false, _) => &AcceptAllReflection,
                    (true, true) => &llm_reflector,
                    (true, false) => &no_delegate_verdicts,
                };
            let delegator: &dyn DelegationProvider = if resolved.delegation {
                &llm_delegator
            } else {
                &RefuseDelegation
            };

            let coordinator = DeepLoopCoordinator {
                runtime: &runtime,
                planner: &planner,
                context: &context,
                workspace: workspace.as_ref(),
                reflector,
                delegator,
                max_iterations: resolved.max_iterations,
            };

            Ok(coordinator.run(&mut envelope, plan)?)
        })
        .await
        .map_err(|join_error| anyhow::anyhow!("spawn_blocking join error: {join_error}"))??;

        // The loop runs on a blocking thread; the reply is one ordinary async
        // call on the same LLM, after it, so it cannot perturb the loop's calls.
        let reply = synthesize_reply(self.llm.as_ref(), &goal, &run).await;
        Ok(outcome_from_run(&run, &operation, reply))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures_util::stream;
    use greentic_dw_runtime::{DeepLoopRun, DeepLoopStatus};
    use greentic_llm::{
        Capabilities, ChatRequest, ChatResponse, ChatStream, FinishReason, LlmError, LlmProvider,
        StreamEvent,
    };
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    /// Queued reply that makes `chat` return an error instead of content.
    const SCRIPTED_ERROR: &str = "!error";

    // Scripted stub: returns queued responses in order, one per chat() call, and
    // records every request's message text so tests can assert on prompts.
    struct ScriptedLlm {
        responses: Mutex<VecDeque<String>>,
        prompts: Mutex<Vec<String>>,
    }

    impl ScriptedLlm {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                prompts: Mutex::new(Vec::new()),
            }
        }

        fn prompts(&self) -> Vec<String> {
            self.prompts.lock().expect("lock").clone()
        }
    }

    #[async_trait]
    impl LlmProvider for ScriptedLlm {
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
            "scripted"
        }

        fn model(&self) -> &str {
            "scripted-model"
        }

        async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, LlmError> {
            let text = req
                .messages
                .iter()
                .map(|message| message.content.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            self.prompts.lock().expect("lock").push(text);
            let content = self
                .responses
                .lock()
                .expect("lock")
                .pop_front()
                .unwrap_or_default();
            if content == SCRIPTED_ERROR {
                return Err(LlmError::Transport("scripted failure".into()));
            }
            Ok(ChatResponse {
                content,
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
            })
        }

        async fn chat_stream(&self, _req: ChatRequest) -> Result<ChatStream, LlmError> {
            use futures_util::StreamExt;
            Ok(stream::iter(vec![Ok(StreamEvent::Done {
                finish_reason: FinishReason::Stop,
            })])
            .boxed())
        }
    }

    fn plan_step(
        id: &str,
        kind: greentic_dw_planning::PlanStepKind,
        status: greentic_dw_planning::PlanStepStatus,
        depends_on: &[&str],
        agent: Option<&str>,
    ) -> greentic_dw_planning::PlanStep {
        greentic_dw_planning::PlanStep {
            step_id: id.into(),
            title: format!("Step {id}"),
            kind,
            status,
            depends_on: depends_on.iter().map(|d| (*d).to_string()).collect(),
            assigned_agent: agent.map(str::to_string),
            inputs_schema_ref: None,
            output_schema_ref: None,
            retry_count: 0,
        }
    }

    /// Serialise a real `PlanDocument` so the scripted JSON matches the live serde shape.
    fn scripted_plan(steps: Vec<greentic_dw_planning::PlanStep>) -> String {
        use greentic_dw_planning::{PlanDocument, PlanStatus};
        serde_json::to_string(&PlanDocument {
            plan_id: "p".into(),
            goal: "g".into(),
            status: PlanStatus::Active,
            revision: 1,
            assumptions: vec![],
            constraints: vec![],
            success_criteria: vec!["task completed".into()],
            steps,
            edges: vec![],
            metadata: std::collections::BTreeMap::new(),
        })
        .expect("plan json")
    }

    fn execute(step_id: &str) -> String {
        format!(r#"[{{"step_id":"{step_id}","action":"execute"}}]"#)
    }

    #[test]
    fn extract_goal_prefers_goal_then_user_text_then_default() {
        assert_eq!(extract_goal(&json!({"goal":"X"})), "X");
        assert_eq!(extract_goal(&json!({"user_text":"Y"})), "Y");
        assert_eq!(extract_goal(&json!({})), "Execute the requested task");
    }

    #[test]
    fn build_envelope_populates_fields() {
        let env = build_envelope("acme", "researcher", "run-1");
        assert_eq!(env.scope.tenant, "acme");
        assert_eq!(env.worker_id, "researcher");
        assert_eq!(env.task_id, "run-1");
        assert!(env.scope.team.is_none());
        assert_eq!(env.state, greentic_dw_types::TaskLifecycleState::Created);
    }

    fn run_with(status: DeepLoopStatus, ids: Vec<String>) -> DeepLoopRun {
        use greentic_dw_planning::{PlanDocument, PlanStatus};
        use std::collections::BTreeMap;
        DeepLoopRun {
            plan: PlanDocument {
                plan_id: "p".into(),
                goal: "g".into(),
                status: PlanStatus::Active,
                revision: 1,
                assumptions: vec![],
                constraints: vec![],
                success_criteria: vec![],
                steps: vec![],
                edges: vec![],
                metadata: BTreeMap::new(),
            },
            status,
            emitted_subtasks: vec![],
            output_artifact_ids: ids,
        }
    }

    #[test]
    fn outcome_from_run_maps_completed_failed_and_budget_exhausted() {
        let ok = outcome_from_run(
            &run_with(DeepLoopStatus::Completed, vec!["a".into()]),
            "run",
            "Done.".into(),
        );
        assert!(ok.ok);
        assert_eq!(ok.output["status"], "completed");
        assert_eq!(ok.output["operation"], "run");
        assert_eq!(ok.output["artifact_ids"], json!(["a"]));
        assert_eq!(ok.output["reply"], "Done.");
        let bad = outcome_from_run(&run_with(DeepLoopStatus::Failed, vec![]), "", "x".into());
        assert!(!bad.ok);
        assert_eq!(bad.output["status"], "failed");
        let capped = outcome_from_run(
            &run_with(DeepLoopStatus::BudgetExhausted, vec![]),
            "",
            "x".into(),
        );
        assert!(!capped.ok, "only Completed is ok");
        assert_eq!(capped.output["status"], "budget_exhausted");
    }

    #[test]
    fn validate_operation_accepts_empty_and_run() {
        assert!(validate_operation("").is_ok());
        assert!(validate_operation("run").is_ok());
        assert!(validate_operation("RUN").is_ok());
        assert!(validate_operation(" run ").is_ok());
    }

    #[test]
    fn validate_operation_rejects_unknown() {
        assert!(validate_operation("delete").is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn invoke_rejects_unsupported_operation() {
        let llm = std::sync::Arc::new(ScriptedLlm::new(vec![]));
        let invoker = DeepWorkerInvoker::new(llm);
        let err = invoker
            .invoke(
                "acme",
                "default",
                "researcher",
                "delete",
                json!({"goal":"x"}),
                Some("run-1"),
            )
            .await;
        assert!(
            err.is_err(),
            "unsupported operation must error before running the loop"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn invoke_runs_loop_to_terminal_status() {
        use greentic_dw_planning::{PlanDocument, PlanStatus};
        use greentic_dw_reflection::{ReviewOutcome, ReviewVerdict};
        use std::collections::BTreeMap;

        // Build the seed plan + final review by SERIALIZING the real structs so the
        // scripted JSON always matches the live serde shape.
        // NOTE: success_criteria must be non-empty to pass validate_plan.
        let plan = PlanDocument {
            plan_id: "p".into(),
            goal: "g".into(),
            status: PlanStatus::Active,
            revision: 1,
            assumptions: vec![],
            constraints: vec![],
            success_criteria: vec!["task completed".to_string()],
            steps: vec![],
            edges: vec![],
            metadata: BTreeMap::new(),
        };
        let review = ReviewOutcome {
            verdict: ReviewVerdict::Accept,
            score: Some(1.0),
            findings: vec![],
            suggested_actions: vec![],
            binding: false,
        };
        let plan_json = serde_json::to_string(&plan).expect("plan json");
        let review_json = serde_json::to_string(&review).expect("review json");

        // Call order: create_plan (invoker) -> next_actions ([]) -> review_final
        // -> reply synthesis. Empty steps plan: evaluate_completion yields
        // Satisfied (vacuously true), so the loop goes straight to review_final.
        let llm = Arc::new(ScriptedLlm::new(vec![
            plan_json,
            "[]".into(),
            review_json,
            "Nothing needed doing, so the task is complete.".into(),
        ]));
        let invoker = DeepWorkerInvoker::new(llm);
        let outcome = invoker
            .invoke(
                "acme",
                "default",
                "researcher",
                "",
                json!({"goal":"do it"}),
                Some("run-1"),
            )
            .await
            .expect("invoke ok");
        assert!(outcome.output.get("status").is_some());
        assert!(
            outcome.ok,
            "empty-steps plan should complete; status was {:?}",
            outcome.output["status"]
        );
        assert_eq!(
            outcome.output["reply"],
            "Nothing needed doing, so the task is complete."
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn iteration_budget_caps_the_loop_and_still_replies() {
        use greentic_dw_planning::{PlanStepKind, PlanStepStatus};
        let llm = Arc::new(ScriptedLlm::new(vec![
            scripted_plan(vec![
                plan_step(
                    "s1",
                    PlanStepKind::ToolCall,
                    PlanStepStatus::Ready,
                    &[],
                    None,
                ),
                plan_step(
                    "s2",
                    PlanStepKind::ToolCall,
                    PlanStepStatus::Pending,
                    &["s1"],
                    None,
                ),
            ]),
            execute("s1"),
            "I finished the first step before running out of budget.".into(),
        ]));
        let invoker = DeepWorkerInvoker::new(llm.clone());
        let outcome = invoker
            .invoke(
                "acme",
                "default",
                "researcher",
                "",
                json!({
                    "goal": "two steps",
                    "deep_worker": {"iterationBudget": 1, "reflection": false, "delegation": false}
                }),
                Some("run-budget"),
            )
            .await
            .expect("budget exhaustion is an outcome, not an error");
        assert!(!outcome.ok);
        assert_eq!(outcome.output["status"], "budget_exhausted");
        assert_eq!(
            outcome.output["reply"],
            "I finished the first step before running out of budget."
        );
        // plan, one next_actions (budget 1), synthesis — nothing else.
        assert_eq!(llm.prompts().len(), 3);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reflection_off_makes_no_review_calls() {
        use greentic_dw_planning::{PlanStepKind, PlanStepStatus};
        // With reflection ON, review_step would consume "[]" and fail to parse it
        // as a ReviewOutcome; with it OFF the loop never asks.
        let llm = Arc::new(ScriptedLlm::new(vec![
            scripted_plan(vec![plan_step(
                "s1",
                PlanStepKind::ToolCall,
                PlanStepStatus::Ready,
                &[],
                None,
            )]),
            execute("s1"),
            "[]".into(),
            "Step s1 is done.".into(),
        ]));
        let invoker = DeepWorkerInvoker::new(llm.clone());
        let outcome = invoker
            .invoke(
                "acme",
                "default",
                "researcher",
                "run",
                json!({"goal": "one step", "deep_worker": {"iterationBudget": 8, "reflection": false}}),
                Some("run-noreflect"),
            )
            .await
            .expect("invoke ok");
        assert!(outcome.ok, "status was {:?}", outcome.output["status"]);
        assert_eq!(outcome.output["reply"], "Step s1 is done.");
        assert_eq!(llm.prompts().len(), 4);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn delegation_off_instructs_the_planner_and_never_delegates() {
        use greentic_dw_planning::{PlanStepKind, PlanStepStatus};
        // The model ignores the instruction and plans a delegate step anyway;
        // without the rewrite, RefuseDelegation would fail the turn.
        let llm = Arc::new(ScriptedLlm::new(vec![
            scripted_plan(vec![plan_step(
                "s1",
                PlanStepKind::Delegate,
                PlanStepStatus::Ready,
                &[],
                Some("helper"),
            )]),
            execute("s1"),
            "[]".into(),
            "Done without handing anything off.".into(),
        ]));
        let invoker = DeepWorkerInvoker::new(llm.clone());
        let outcome = invoker
            .invoke(
                "acme",
                "default",
                "researcher",
                "",
                json!({"goal": "g", "deep_worker": {"reflection": false, "delegation": false}}),
                Some("run-nodelegate"),
            )
            .await
            .expect("a delegate step must not fail the turn when delegation is off");
        assert!(outcome.ok);
        let prompts = llm.prompts();
        assert!(
            prompts[0].contains(crate::guards::NO_DELEGATION_CONSTRAINT),
            "create_plan prompt must carry the no-delegation instruction"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn synthesis_failure_falls_back_to_a_status_sentence() {
        use greentic_dw_planning::{PlanStepKind, PlanStepStatus};
        let llm = Arc::new(ScriptedLlm::new(vec![
            scripted_plan(vec![plan_step(
                "s1",
                PlanStepKind::ToolCall,
                PlanStepStatus::Ready,
                &[],
                None,
            )]),
            execute("s1"),
            "[]".into(),
            SCRIPTED_ERROR.into(),
        ]));
        let invoker = DeepWorkerInvoker::new(llm);
        let outcome = invoker
            .invoke(
                "acme",
                "default",
                "researcher",
                "",
                json!({"goal": "g", "deep_worker": {"reflection": false}}),
                Some("run-fallback"),
            )
            .await
            .expect("a failed synthesis must not fail the turn");
        assert!(outcome.ok);
        assert_eq!(outcome.output["reply"], "The task completed in 1 step.");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn empty_synthesis_falls_back_too() {
        // Queue runs dry → ScriptedLlm returns "" for the synthesis call.
        use greentic_dw_planning::{PlanDocument, PlanStatus};
        let plan = serde_json::to_string(&PlanDocument {
            plan_id: "p".into(),
            goal: "g".into(),
            status: PlanStatus::Active,
            revision: 1,
            assumptions: vec![],
            constraints: vec![],
            success_criteria: vec!["task completed".into()],
            steps: vec![],
            edges: vec![],
            metadata: std::collections::BTreeMap::new(),
        })
        .expect("plan json");
        let llm = Arc::new(ScriptedLlm::new(vec![plan, "[]".into()]));
        let invoker = DeepWorkerInvoker::new(llm);
        let outcome = invoker
            .invoke(
                "acme",
                "default",
                "researcher",
                "",
                json!({"goal": "g", "deep_worker": {"reflection": false}}),
                Some("run-empty"),
            )
            .await
            .expect("invoke ok");
        assert!(outcome.ok);
        assert_eq!(outcome.output["reply"], "The task completed in 0 steps.");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn malformed_deep_worker_is_rejected_before_any_llm_call() {
        let llm = Arc::new(ScriptedLlm::new(vec![]));
        let invoker = DeepWorkerInvoker::new(llm.clone());
        let result = invoker
            .invoke(
                "acme",
                "default",
                "researcher",
                "",
                json!({"goal": "g", "deep_worker": {"iterationBudget": "eight"}}),
                Some("run-bad"),
            )
            .await;
        assert!(result.is_err());
        assert!(llm.prompts().is_empty());
    }
}
