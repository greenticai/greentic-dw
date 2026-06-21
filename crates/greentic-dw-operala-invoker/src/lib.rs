//! Production [`OperalaDispatchInvoker`]: wires the five deep-worker providers
//! into a [`DeepLoopCoordinator`] and runs it on a blocking thread.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use greentic_dw_context_llm::LlmContextProvider;
use greentic_dw_core::RuntimeOperation;
use greentic_dw_delegation_llm::LlmDelegationProvider;
use greentic_dw_engine::{EngineDecision, StaticEngine};
use greentic_dw_operala_bridge::{InvokeOutcome, OperalaDispatchInvoker};
use greentic_dw_planning::CreatePlanRequest;
use greentic_dw_planning::PlanningProvider;
use greentic_dw_planning_llm::LlmPlanningProvider;
use greentic_dw_reflection_llm::LlmReflectionProvider;
use greentic_dw_runtime::{DeepLoopCoordinator, DeepLoopRun, DeepLoopStatus, DwRuntime};
use greentic_dw_types::{
    LocaleContext, LocalePropagation, OutputLocaleGuidance, TaskEnvelope, TaskLifecycleState,
    TenantScope, WorkerLocalePolicy,
};
use greentic_dw_workspace::WorkspaceScope;
use greentic_dw_workspace_mem::InMemoryWorkspaceProvider;
use greentic_llm::LlmProvider;

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

/// Map a finished deep-loop run to the bridge's `InvokeOutcome`.
fn outcome_from_run(run: &DeepLoopRun) -> InvokeOutcome {
    InvokeOutcome {
        ok: matches!(run.status, DeepLoopStatus::Completed),
        output: json!({
            "status": format!("{:?}", run.status),
            "artifact_ids": run.output_artifact_ids,
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
        _operation: &str,
        input: Value,
        idempotency_key: Option<&str>,
    ) -> Result<InvokeOutcome> {
        let llm = Arc::clone(&self.llm);
        let tenant = tenant.to_string();
        let target = target.to_string();
        let task_id = idempotency_key.unwrap_or(FALLBACK_TASK_ID).to_string();

        let outcome = tokio::task::spawn_blocking(move || -> Result<InvokeOutcome> {
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
            let reflector = LlmReflectionProvider::new(Arc::clone(&llm));
            let delegator = LlmDelegationProvider::new(Arc::clone(&llm));
            let ws_dyn: Arc<dyn greentic_dw_workspace::WorkspaceProvider> = workspace.clone();
            let context = LlmContextProvider::new(Arc::clone(&llm), ws_dyn, scope);

            let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Operation(
                RuntimeOperation::Step,
            )));

            let plan = planner.create_plan(CreatePlanRequest {
                goal: extract_goal(&input),
                assumptions: vec![],
                constraints: vec![],
                success_criteria: vec!["task completed".to_string()],
            })?;

            let mut envelope = build_envelope(&tenant, &target, &task_id);

            let coordinator = DeepLoopCoordinator {
                runtime: &runtime,
                planner: &planner,
                context: &context,
                workspace: workspace.as_ref(),
                reflector: &reflector,
                delegator: &delegator,
            };

            let run = coordinator.run(&mut envelope, plan)?;
            Ok(outcome_from_run(&run))
        })
        .await
        .map_err(|join_error| anyhow::anyhow!("spawn_blocking join error: {join_error}"))??;

        Ok(outcome)
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

    // Scripted stub: returns queued responses in order, one per chat() call.
    struct ScriptedLlm {
        responses: Mutex<VecDeque<String>>,
    }

    impl ScriptedLlm {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
            }
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

        async fn chat(&self, _req: ChatRequest) -> Result<ChatResponse, LlmError> {
            let content = self
                .responses
                .lock()
                .expect("lock")
                .pop_front()
                .unwrap_or_default();
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
    fn outcome_from_run_maps_completed_and_failed() {
        let ok = outcome_from_run(&run_with(DeepLoopStatus::Completed, vec!["a".into()]));
        assert!(ok.ok);
        assert_eq!(ok.output["status"], "Completed");
        assert_eq!(ok.output["artifact_ids"], json!(["a"]));
        let bad = outcome_from_run(&run_with(DeepLoopStatus::Failed, vec![]));
        assert!(!bad.ok);
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

        // Call order: create_plan (invoker) -> next_actions ([]) -> review_final.
        // Empty steps plan: evaluate_completion yields Satisfied (vacuously true),
        // so the loop goes straight to review_final after next_actions returns [].
        let llm = Arc::new(ScriptedLlm::new(vec![plan_json, "[]".into(), review_json]));
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
    }
}
