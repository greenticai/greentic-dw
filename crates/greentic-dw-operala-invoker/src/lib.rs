//! Production [`OperalaDispatchInvoker`]: wires the five deep-worker providers
//! into a [`DeepLoopCoordinator`], runs it on a blocking thread, and writes a
//! prose `reply`. Honours the designer's `deep_worker` settings (see `settings`).
//!
//! A host may inject tools ([`DeepWorkerInvoker::with_tools`]). When it does,
//! and the model supports tool calling, every non-delegate step is carried out
//! by [`executor::LlmToolStepExecutor`] — a bounded tool-calling loop — and the
//! reply is written from the steps' real outputs. Without tools the invoker
//! behaves exactly as before.

mod executor;
mod guards;
mod reply;
mod settings;
mod tools;

pub use tools::{DeepWorkerTools, ToolSpec};

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
use greentic_dw_workspace::{ReadArtifactRequest, WorkspaceProvider, WorkspaceScope};
use greentic_dw_workspace_mem::InMemoryWorkspaceProvider;
use greentic_llm::LlmProvider;

use crate::executor::LlmToolStepExecutor;
use crate::guards::{
    AcceptAllReflection, NO_DELEGATION_CONSTRAINT, NoDelegateVerdict, RefuseDelegation,
    strip_delegate_steps,
};
use crate::reply::{StepOutput, synthesize_reply};
use crate::settings::{ResolvedSettings, settings_from_input};

const DEFAULT_GOAL: &str = "Execute the requested task";
const FALLBACK_TASK_ID: &str = "task-unknown";

/// Production invoker. Each dispatch builds fresh providers + an in-memory
/// workspace and runs one deep loop.
pub struct DeepWorkerInvoker {
    llm: Arc<dyn LlmProvider>,
    tools: Option<Arc<dyn DeepWorkerTools>>,
}

impl DeepWorkerInvoker {
    /// Create an invoker over the configured LLM, with no tools.
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self { llm, tools: None }
    }

    /// Give the deep worker host tools. They are used only when the list is
    /// non-empty and the LLM reports tool support; otherwise the worker runs
    /// exactly as a tool-less one. `delegation: false` does not hide them.
    pub fn with_tools(mut self, tools: Option<Arc<dyn DeepWorkerTools>>) -> Self {
        self.tools = tools;
        self
    }

    /// The tools to wire for one run, or `None` for today's tool-less loop.
    fn active_tools(&self) -> Option<(Arc<dyn DeepWorkerTools>, Vec<ToolSpec>)> {
        let tools = self.tools.as_ref()?;
        if !self.llm.capabilities().tools {
            tracing::debug!(
                provider = self.llm.provider_name(),
                "deep-worker tools ignored: the model does not support tool calling"
            );
            return None;
        }
        let specs = tools.list();
        if specs.is_empty() {
            return None;
        }
        Some((Arc::clone(tools), specs))
    }
}

/// Planner instruction naming the tools a step may use.
fn tools_constraint(specs: &[ToolSpec]) -> String {
    let listed = specs
        .iter()
        .map(|spec| format!("{} — {}", spec.name, spec.description))
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "Available tools: {listed}. A step may call these tools while it is carried out; \
         plan the steps around them where they help."
    )
}

/// Read each output artifact back so the reply can quote what the steps found.
fn read_step_outputs(workspace: &dyn WorkspaceProvider, run: &DeepLoopRun) -> Vec<StepOutput> {
    run.output_artifact_ids
        .iter()
        .filter_map(|artifact_id| {
            match workspace.read_artifact(ReadArtifactRequest {
                artifact_id: artifact_id.clone(),
            }) {
                Ok(content) => Some(StepOutput {
                    title: content.metadata.title,
                    body: content.body,
                }),
                Err(error) => {
                    tracing::warn!(%artifact_id, %error, "deep-worker step output unreadable; leaving it out of the reply");
                    None
                }
            }
        })
        .collect()
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
fn plan_request(goal: String, delegation: bool, tools: Option<&[ToolSpec]>) -> CreatePlanRequest {
    let mut constraints = if delegation {
        vec![]
    } else {
        vec![NO_DELEGATION_CONSTRAINT.to_string()]
    };
    if let Some(specs) = tools {
        constraints.push(tools_constraint(specs));
    }
    CreatePlanRequest {
        goal,
        assumptions: vec![],
        constraints,
        success_criteria: vec!["task completed".to_string()],
    }
}

/// Map a finished deep-loop run to the bridge's `InvokeOutcome`. `ok` is true
/// only for `Completed`; `reply` is always prose (see [`reply`]).
/// With tools wired, the output also carries `tool_calls_used`; a tool-less
/// run's output is exactly what it always was.
fn outcome_from_run(
    run: &DeepLoopRun,
    operation: &str,
    reply: String,
    tools_wired: bool,
) -> InvokeOutcome {
    let mut output = json!({
        "status": run.status.to_string(),
        "operation": operation,
        "artifact_ids": run.output_artifact_ids,
        "reply": reply,
    });
    if tools_wired && let Some(object) = output.as_object_mut() {
        object.insert("tool_calls_used".to_string(), json!(run.tool_calls_used));
    }
    InvokeOutcome {
        ok: matches!(run.status, DeepLoopStatus::Completed),
        output,
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
        let active_tools = self.active_tools();
        let tools_wired = active_tools.is_some();

        let (run, step_outputs) =
            tokio::task::spawn_blocking(move || -> Result<(DeepLoopRun, Vec<StepOutput>)> {
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

                let mut plan = planner.create_plan(plan_request(
                    loop_goal,
                    resolved.delegation,
                    active_tools.as_ref().map(|(_, specs)| specs.as_slice()),
                ))?;
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

                let executor = active_tools.map(|(tools, specs)| {
                    LlmToolStepExecutor::new(Arc::clone(&llm), tools, &specs)
                });

                let coordinator = DeepLoopCoordinator {
                    runtime: &runtime,
                    planner: &planner,
                    context: &context,
                    workspace: workspace.as_ref(),
                    reflector,
                    delegator,
                    max_iterations: resolved.max_iterations,
                    executor: executor
                        .as_ref()
                        .map(|executor| executor as &dyn greentic_dw_runtime::StepExecutor),
                };

                let run = coordinator.run(&mut envelope, plan)?;
                let step_outputs = if executor.is_some() {
                    read_step_outputs(workspace.as_ref(), &run)
                } else {
                    vec![]
                };
                Ok((run, step_outputs))
            })
            .await
            .map_err(|join_error| anyhow::anyhow!("spawn_blocking join error: {join_error}"))??;

        // The loop runs on a blocking thread; the reply is one ordinary async
        // call on the same LLM, after it, so it cannot perturb the loop's calls.
        let reply = synthesize_reply(self.llm.as_ref(), &goal, &run, &step_outputs).await;
        Ok(outcome_from_run(&run, &operation, reply, tools_wired))
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
    /// Queued reply prefix that makes `chat` return one tool call.
    const SCRIPTED_TOOL: &str = "!tool:";

    // Scripted stub: returns queued responses in order, one per chat() call, and
    // records every request's message text so tests can assert on prompts.
    //
    // A queued reply of the form `!tool:<name>:<json args>` makes the response a
    // single tool call instead of text.
    struct ScriptedLlm {
        responses: Mutex<VecDeque<String>>,
        prompts: Mutex<Vec<String>>,
        requests: Mutex<Vec<RecordedRequest>>,
        tool_support: bool,
    }

    /// The parts of a request the tool tests assert on.
    #[derive(Debug, Clone)]
    struct RecordedRequest {
        messages: Vec<(greentic_llm::MessageRole, String)>,
        tool_names: Vec<String>,
        tool_choice: Option<String>,
    }

    impl RecordedRequest {
        fn tool_messages(&self) -> Vec<String> {
            self.messages
                .iter()
                .filter(|(role, _)| *role == greentic_llm::MessageRole::Tool)
                .map(|(_, content)| content.clone())
                .collect()
        }
    }

    impl ScriptedLlm {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                prompts: Mutex::new(Vec::new()),
                requests: Mutex::new(Vec::new()),
                tool_support: false,
            }
        }

        fn with_tool_support(mut self) -> Self {
            self.tool_support = true;
            self
        }

        fn prompts(&self) -> Vec<String> {
            self.prompts.lock().expect("lock").clone()
        }

        fn requests(&self) -> Vec<RecordedRequest> {
            self.requests.lock().expect("lock").clone()
        }
    }

    #[async_trait]
    impl LlmProvider for ScriptedLlm {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                chat: true,
                tools: self.tool_support,
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
            self.requests.lock().expect("lock").push(RecordedRequest {
                messages: req
                    .messages
                    .iter()
                    .map(|message| (message.role.clone(), message.content.clone()))
                    .collect(),
                tool_names: req.tools.iter().map(|tool| tool.name.clone()).collect(),
                tool_choice: req.tool_choice.clone(),
            });
            let content = self
                .responses
                .lock()
                .expect("lock")
                .pop_front()
                .unwrap_or_default();
            if content == SCRIPTED_ERROR {
                return Err(LlmError::Transport("scripted failure".into()));
            }
            if let Some(call) = content.strip_prefix(SCRIPTED_TOOL) {
                let (name, args) = call.split_once(':').expect("!tool:<name>:<json>");
                let index = self.requests.lock().expect("lock").len();
                return Ok(ChatResponse {
                    content: String::new(),
                    tool_calls: vec![greentic_llm::ToolCall {
                        id: format!("call-{index}"),
                        name: name.to_string(),
                        arguments: serde_json::from_str(args).expect("tool args json"),
                    }],
                    finish_reason: FinishReason::ToolCalls,
                    usage: None,
                });
            }
            Ok(ChatResponse {
                content,
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
                usage: None,
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
            tool_calls_used: 2,
        }
    }

    #[test]
    fn outcome_from_run_maps_completed_failed_and_budget_exhausted() {
        let ok = outcome_from_run(
            &run_with(DeepLoopStatus::Completed, vec!["a".into()]),
            "run",
            "Done.".into(),
            false,
        );
        assert!(ok.ok);
        assert_eq!(ok.output["status"], "completed");
        assert_eq!(ok.output["operation"], "run");
        assert_eq!(ok.output["artifact_ids"], json!(["a"]));
        assert_eq!(ok.output["reply"], "Done.");
        assert!(ok.output.get("tool_calls_used").is_none());
        let bad = outcome_from_run(
            &run_with(DeepLoopStatus::Failed, vec![]),
            "",
            "x".into(),
            false,
        );
        assert!(!bad.ok);
        assert_eq!(bad.output["status"], "failed");
        let capped = outcome_from_run(
            &run_with(DeepLoopStatus::BudgetExhausted, vec![]),
            "",
            "x".into(),
            false,
        );
        assert!(!capped.ok, "only Completed is ok");
        assert_eq!(capped.output["status"], "budget_exhausted");
        let wired = outcome_from_run(
            &run_with(DeepLoopStatus::Completed, vec![]),
            "",
            "x".into(),
            true,
        );
        assert_eq!(wired.output["tool_calls_used"], 2);
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

    /// Host tools double: `lookup` answers `{"status":"paid"}`, `broken` fails
    /// at the infrastructure level, anything else echoes its arguments.
    struct FakeTools {
        specs: Vec<ToolSpec>,
        calls: Mutex<Vec<(String, Value)>>,
    }

    impl FakeTools {
        fn new() -> Self {
            Self {
                specs: vec![ToolSpec {
                    name: "lookup".into(),
                    description: "Look up an order".into(),
                    parameters: json!({"type": "object"}),
                }],
                calls: Mutex::new(Vec::new()),
            }
        }

        fn empty() -> Self {
            Self {
                specs: vec![],
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<(String, Value)> {
            self.calls.lock().expect("lock").clone()
        }
    }

    #[async_trait]
    impl DeepWorkerTools for FakeTools {
        fn list(&self) -> Vec<ToolSpec> {
            self.specs.clone()
        }

        async fn call(&self, name: &str, args: Value) -> anyhow::Result<Value> {
            self.calls
                .lock()
                .expect("lock")
                .push((name.to_string(), args.clone()));
            match name {
                "lookup" => Ok(json!({"status": "paid"})),
                "broken" => Err(anyhow::anyhow!("tool host unreachable")),
                _ => Ok(json!({"echo": args})),
            }
        }
    }

    fn one_step_plan() -> String {
        use greentic_dw_planning::{PlanStepKind, PlanStepStatus};
        scripted_plan(vec![plan_step(
            "s1",
            PlanStepKind::ToolCall,
            PlanStepStatus::Ready,
            &[],
            None,
        )])
    }

    fn tool(name: &str, args: Value) -> String {
        format!("{SCRIPTED_TOOL}{name}:{args}")
    }

    async fn invoke_with(
        llm: Arc<ScriptedLlm>,
        tools: Option<Arc<dyn DeepWorkerTools>>,
        deep_worker: Value,
    ) -> InvokeOutcome {
        DeepWorkerInvoker::new(llm)
            .with_tools(tools)
            .invoke(
                "acme",
                "default",
                "researcher",
                "",
                json!({"goal": "refund order 42", "deep_worker": deep_worker}),
                Some("run-tools"),
            )
            .await
            .expect("invoke ok")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_tool_result_reaches_the_next_request_and_the_reply() {
        let llm = Arc::new(
            ScriptedLlm::new(vec![
                one_step_plan(),
                execute("s1"),
                tool("lookup", json!({"order": 42})),
                "Order 42 is paid.".into(),
                "[]".into(),
                "Order 42 was already paid.".into(),
            ])
            .with_tool_support(),
        );
        let tools = Arc::new(FakeTools::new());
        let outcome = invoke_with(
            llm.clone(),
            Some(tools.clone()),
            json!({"reflection": false}),
        )
        .await;

        assert!(outcome.ok, "status was {:?}", outcome.output["status"]);
        assert_eq!(outcome.output["reply"], "Order 42 was already paid.");
        assert_eq!(outcome.output["tool_calls_used"], 1);
        assert_eq!(
            tools.calls(),
            vec![("lookup".to_string(), json!({"order": 42}))]
        );

        let requests = llm.requests();
        assert_eq!(requests.len(), 6);
        let plan_prompt = &llm.prompts()[0];
        assert!(
            plan_prompt.contains("Available tools: lookup — Look up an order"),
            "the planner must be told about the tools"
        );
        assert_eq!(requests[2].tool_names, vec!["lookup".to_string()]);
        assert_eq!(requests[2].tool_choice.as_deref(), Some("auto"));
        assert_eq!(
            requests[3].tool_messages(),
            vec![r#"{"status":"paid"}"#.to_string()]
        );
        let reply_prompt = &llm.prompts()[5];
        assert!(reply_prompt.contains("Step outputs"), "{reply_prompt}");
        assert!(reply_prompt.contains("Order 42 is paid."), "{reply_prompt}");
        assert!(
            reply_prompt.contains(r#"\"status\":\"paid\""#),
            "{reply_prompt}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_model_that_always_calls_tools_stops_at_the_per_step_cap() {
        let mut script = vec![one_step_plan(), execute("s1")];
        // Six granted calls, then a seventh request the executor must not run.
        for n in 1..=7 {
            script.push(tool("lookup", json!({"n": n})));
        }
        script.push("[]".into());
        script.push("Looked it up six times.".into());
        let llm = Arc::new(ScriptedLlm::new(script).with_tool_support());
        let tools = Arc::new(FakeTools::new());
        // Budget 8 → 24 tool calls in total, so only the per-step cap binds.
        let outcome = invoke_with(
            llm.clone(),
            Some(tools.clone()),
            json!({"iterationBudget": 8, "reflection": false}),
        )
        .await;

        assert_eq!(tools.calls().len(), greentic_dw_runtime::PER_STEP_TOOL_CAP);
        assert_eq!(outcome.output["tool_calls_used"], 6);
        let requests = llm.requests();
        // plan, next_actions, six tool rounds, then the forced text round.
        let final_round = &requests[8];
        assert_eq!(final_round.tool_choice.as_deref(), Some("none"));
        assert_eq!(final_round.tool_messages().len(), 6);
        assert!(outcome.ok, "status was {:?}", outcome.output["status"]);
        assert_eq!(outcome.output["reply"], "Looked it up six times.");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_total_tool_cap_leaves_later_steps_no_tools() {
        use greentic_dw_planning::{PlanStepKind, PlanStepStatus};
        let mut script = vec![
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
                    PlanStepStatus::Ready,
                    &[],
                    None,
                ),
            ]),
            execute("s1"),
        ];
        for n in 1..=6 {
            script.push(tool("lookup", json!({"n": n})));
        }
        script.push("s1 done".into());
        script.push(execute("s2"));
        script.push("s2 answered from what it had".into());
        script.push("Stopped after two steps.".into());
        let llm = Arc::new(ScriptedLlm::new(script).with_tool_support());
        let tools = Arc::new(FakeTools::new());
        // Budget 2 → 6 tool calls in total; s1 spends them all.
        let outcome = invoke_with(
            llm.clone(),
            Some(tools.clone()),
            json!({"iterationBudget": 2, "reflection": false}),
        )
        .await;

        assert_eq!(tools.calls().len(), 6);
        assert_eq!(outcome.output["status"], "budget_exhausted");
        assert_eq!(outcome.output["tool_calls_used"], 6);
        let requests = llm.requests();
        let s2_round = &requests[10];
        assert!(
            s2_round.tool_names.is_empty(),
            "no tools once the budget is spent"
        );
        assert_eq!(s2_round.tool_choice, None);
        assert!(llm.prompts()[10].contains("No tool calls are available"));
        assert_eq!(outcome.output["reply"], "Stopped after two steps.");
    }

    /// The prompts of the plain one-step script used by the tool-less checks.
    async fn tool_less_prompts(
        tools: Option<Arc<dyn DeepWorkerTools>>,
        tool_support: bool,
    ) -> Vec<String> {
        let mut llm = ScriptedLlm::new(vec![
            one_step_plan(),
            execute("s1"),
            "[]".into(),
            "Step s1 is done.".into(),
        ]);
        if tool_support {
            llm = llm.with_tool_support();
        }
        let llm = Arc::new(llm);
        let outcome = invoke_with(llm.clone(), tools, json!({"reflection": false})).await;
        assert!(outcome.ok);
        assert_eq!(outcome.output["reply"], "Step s1 is done.");
        assert!(outcome.output.get("tool_calls_used").is_none());
        llm.prompts()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_model_without_tool_support_skips_the_tool_loop() {
        let tools = Arc::new(FakeTools::new());
        let with_tools = tool_less_prompts(Some(tools.clone()), false).await;
        assert!(tools.calls().is_empty());
        assert_eq!(with_tools.len(), 4, "no execution round");
        assert_eq!(with_tools, tool_less_prompts(None, false).await);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn no_tools_keeps_todays_prompts() {
        let baseline = tool_less_prompts(None, true).await;
        assert_eq!(
            baseline,
            tool_less_prompts(Some(Arc::new(FakeTools::empty())), true).await
        );
        assert!(!baseline[0].contains("Available tools"));
        // Golden: the reply prompt is byte-for-byte what it was before tools.
        assert_eq!(
            baseline[3],
            format!(
                "{}\nGoal: refund order 42\nOutcome: completed\n\nPlan steps:\n1. Step s1 — completed",
                crate::reply::REPLY_SYSTEM_PROMPT
            )
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_re_executed_step_reuses_the_cached_tool_result() {
        let llm = Arc::new(
            ScriptedLlm::new(vec![
                one_step_plan(),
                execute("s1"),
                tool("lookup", json!({"order": 42, "full": true})),
                "first pass".into(),
                // The planner revisits s1; the model repeats the same call with
                // its arguments in a different order.
                execute("s1"),
                tool("lookup", json!({"full": true, "order": 42})),
                "second pass".into(),
                "[]".into(),
                "Done.".into(),
            ])
            .with_tool_support(),
        );
        let tools = Arc::new(FakeTools::new());
        let outcome = invoke_with(
            llm.clone(),
            Some(tools.clone()),
            json!({"reflection": false}),
        )
        .await;

        assert!(outcome.ok, "status was {:?}", outcome.output["status"]);
        assert_eq!(tools.calls().len(), 1, "the side effect must not run twice");
        assert_eq!(outcome.output["tool_calls_used"], 1, "a cached hit is free");
        let requests = llm.requests();
        assert_eq!(
            requests[6].tool_messages(),
            vec![r#"{"status":"paid"}"#.to_string()]
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_infrastructure_tool_failure_is_an_error_body_not_a_failed_run() {
        let llm = Arc::new(
            ScriptedLlm::new(vec![
                one_step_plan(),
                execute("s1"),
                tool("broken", json!({})),
                "I could not reach the order system.".into(),
                "[]".into(),
                "The lookup failed.".into(),
            ])
            .with_tool_support(),
        );
        let tools = Arc::new(FakeTools::new());
        let outcome = invoke_with(
            llm.clone(),
            Some(tools.clone()),
            json!({"reflection": false}),
        )
        .await;

        assert!(outcome.ok, "status was {:?}", outcome.output["status"]);
        let requests = llm.requests();
        let shown = requests[3].tool_messages();
        assert_eq!(shown.len(), 1);
        assert!(
            shown[0].contains("tool call failed: tool host unreachable"),
            "{shown:?}"
        );
        let reply_prompt = &llm.prompts()[5];
        assert!(
            reply_prompt.contains(r#""status":"error""#),
            "{reply_prompt}"
        );
    }
}
