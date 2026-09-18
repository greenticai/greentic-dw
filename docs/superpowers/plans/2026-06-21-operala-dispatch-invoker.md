# Production OperalaDispatchInvoker — Implementation Plan (deep-worker brain, slice 5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** New `greentic-dw-operala-invoker` crate: `DeepWorkerInvoker` implementing `greentic_dw_operala_bridge::OperalaDispatchInvoker` — constructs the five LLM-backed/store providers, seeds a plan, runs the `DeepLoopCoordinator` on a blocking thread, and maps the result to an `InvokeOutcome`.

**Architecture:** `invoke` wraps the synchronous deep loop in `tokio::task::spawn_blocking`. Three pure helpers (`extract_goal`, `build_envelope`, `outcome_from_run`) hold the invoker's own logic and are unit-tested; the loop itself is exercised end-to-end with a scripted stub LLM.

**Tech Stack:** Rust edition 2024; `greentic-dw-operala-bridge`, `greentic-dw-runtime`, `greentic-dw-{planning,reflection,delegation,context,workspace}-llm`/`-mem`, `greentic-dw-types`, `greentic-dw-engine`, `greentic-dw-core`, `greentic-dw-planning`, `greentic-llm` (git tag v1.2.6-research), tokio, async-trait, serde_json.

## Global Constraints

- Edition 2024. **No `.unwrap()`/`.expect()` in non-test code.**
- `greentic-llm` git tag `v1.2.6-research` (no path/patch).
- `create_plan`/`run` errors propagate via `?` into `anyhow::Result`; join errors map to `anyhow`.
- Conventional commits; **NO Claude/AI co-author or attribution**.
- Worktree `.worktrees/operala-invoker` (greentic-dw), branch `feat/operala-invoker`. greentic-dw pushes via **SSH**.
- ALWAYS prefix cargo with `CARGO_NET_GIT_FETCH_WITH_CLI=true`. Scoped: `cargo build/test/clippy -p greentic-dw-operala-invoker`. On "No space left on device": STOP, report BLOCKED.

## Reference (read first)

- Seam: `crates/greentic-dw-operala-bridge/src/lib.rs` (`OperalaDispatchInvoker`, `InvokeOutcome`).
- Loop: `crates/greentic-dw-runtime/src/deep_loop.rs` (`DeepLoopCoordinator`, `DeepLoopRun`, `DeepLoopStatus`, `run`).
- Construction reference: `crates/greentic-dw-testing/src/deep_loop_harness_tests.rs` (how runtime+coordinator are built) and `crates/greentic-dw-runtime/src/deep_loop.rs` `sample_envelope()` (TaskEnvelope literal).
- StubLlm pattern: `crates/greentic-dw-reflection-llm/src/lib.rs` test module.
- Provider constructors: the five `*-llm` / `*-mem` crates' `lib.rs`.

---

## Task 1: `greentic-dw-operala-invoker` crate

**Files:**
- Create: `crates/greentic-dw-operala-invoker/{Cargo.toml, src/lib.rs}`
- Modify: root `Cargo.toml` (`[workspace] members`)

**Interfaces:**
- Produces: `DeepWorkerInvoker::new(llm: Arc<dyn greentic_llm::LlmProvider>) -> Self` implementing `greentic_dw_operala_bridge::OperalaDispatchInvoker`.

- [ ] **Step 1: Read the references**

Read the seam (`greentic-dw-operala-bridge/src/lib.rs`), the loop (`greentic-dw-runtime/src/deep_loop.rs` — confirm `DeepLoopStatus` variants, `DeepLoopRun` fields, and read `LlmPlanningProvider::evaluate_completion` in `crates/greentic-dw-planning-llm/src/lib.rs` to learn its empty-steps verdict), and the `StubLlm` test double in `greentic-dw-reflection-llm/src/lib.rs`.

- [ ] **Step 2: Cargo.toml + workspace member**

Create `crates/greentic-dw-operala-invoker/Cargo.toml`. Declare the internal deps the SAME way `greentic-dw-reflection-llm` declares its contract dep (`{ workspace = true }` if present in root `[workspace.dependencies]`, else `{ path = "../<crate>" }`). The `*-llm`/`*-mem` and runtime/types/engine/core/planning crates are path siblings — use `{ path = "../<crate>" }` for any not in `[workspace.dependencies]`.
```toml
[package]
name = "greentic-dw-operala-invoker"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
description = "Production OperalaDispatchInvoker wiring the deep-worker providers into the DeepLoopCoordinator."
publish = false

[dependencies]
greentic-dw-operala-bridge = { path = "../greentic-dw-operala-bridge" }
greentic-dw-runtime        = { path = "../greentic-dw-runtime" }
greentic-dw-engine         = { path = "../greentic-dw-engine" }
greentic-dw-core           = { path = "../greentic-dw-core" }
greentic-dw-types          = { path = "../greentic-dw-types" }
greentic-dw-planning       = { path = "../greentic-dw-planning" }
greentic-dw-workspace      = { path = "../greentic-dw-workspace" }
greentic-dw-planning-llm   = { path = "../greentic-dw-planning-llm" }
greentic-dw-reflection-llm = { path = "../greentic-dw-reflection-llm" }
greentic-dw-delegation-llm = { path = "../greentic-dw-delegation-llm" }
greentic-dw-context-llm    = { path = "../greentic-dw-context-llm" }
greentic-dw-workspace-mem  = { path = "../greentic-dw-workspace-mem" }
greentic-llm = { git = "https://github.com/greenticai/greentic-llm", tag = "v1.2.6-research" }
anyhow = "1"
async-trait = "0.1"
serde_json = { workspace = true }
tokio = { version = "1", features = ["rt", "rt-multi-thread"] }

[dev-dependencies]
futures-util = "0.3"
greentic-dw-planning   = { path = "../greentic-dw-planning" }
greentic-dw-reflection = { path = "../greentic-dw-reflection" }
tokio = { version = "1", features = ["rt", "rt-multi-thread", "macros"] }
```
(If `greentic-dw-planning` is already a normal dependency, it does not need repeating under dev-dependencies — keep whichever the build accepts.) Add `"crates/greentic-dw-operala-invoker",` to root `Cargo.toml` `[workspace] members`.

- [ ] **Step 3: Write the failing tests (TDD — RED)**

Create `crates/greentic-dw-operala-invoker/src/lib.rs` with a `#[cfg(test)] mod tests`. Copy the `StubLlm` shape from `greentic-dw-reflection-llm` and add a `ScriptedLlm` whose `chat` pops the next canned response from a `Mutex<VecDeque<String>>`.

```rust
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
    struct ScriptedLlm { responses: Mutex<VecDeque<String>> }
    impl ScriptedLlm {
        fn new(responses: Vec<String>) -> Self { Self { responses: Mutex::new(responses.into()) } }
    }
    #[async_trait]
    impl LlmProvider for ScriptedLlm {
        fn capabilities(&self) -> Capabilities { Capabilities { chat: true, tools: false, streaming: false, vision: false, system_prompt: true } }
        fn provider_name(&self) -> &'static str { "scripted" }
        fn model(&self) -> &str { "scripted-model" }
        async fn chat(&self, _req: ChatRequest) -> Result<ChatResponse, LlmError> {
            let content = self.responses.lock().expect("lock").pop_front().unwrap_or_default();
            Ok(ChatResponse { content, tool_calls: vec![], finish_reason: FinishReason::Stop })
        }
        async fn chat_stream(&self, _req: ChatRequest) -> Result<ChatStream, LlmError> {
            use futures_util::StreamExt;
            Ok(stream::iter(vec![Ok(StreamEvent::Done { finish_reason: FinishReason::Stop })]).boxed())
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
                plan_id: "p".into(), goal: "g".into(), status: PlanStatus::Active, revision: 1,
                assumptions: vec![], constraints: vec![], success_criteria: vec![], steps: vec![],
                edges: vec![], metadata: BTreeMap::new(),
            },
            status, emitted_subtasks: vec![], output_artifact_ids: ids,
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
        let plan = PlanDocument {
            plan_id: "p".into(), goal: "g".into(), status: PlanStatus::Active, revision: 1,
            assumptions: vec![], constraints: vec![], success_criteria: vec![], steps: vec![],
            edges: vec![], metadata: BTreeMap::new(),
        };
        let review = ReviewOutcome { verdict: ReviewVerdict::Accept, score: Some(1.0), findings: vec![], suggested_actions: vec![], binding: false };
        let plan_json = serde_json::to_string(&plan).expect("plan json");
        let review_json = serde_json::to_string(&review).expect("review json");
        // call order: create_plan (invoker) -> next_actions ([]) -> review_final.
        let llm = std::sync::Arc::new(ScriptedLlm::new(vec![plan_json, "[]".into(), review_json]));
        let invoker = DeepWorkerInvoker::new(llm);
        let outcome = invoker
            .invoke("acme", "default", "researcher", "", json!({"goal":"do it"}), Some("run-1"))
            .await
            .expect("invoke ok");
        assert!(outcome.output.get("status").is_some());
        assert!(outcome.ok, "empty-steps plan should complete; status was {:?}", outcome.output["status"]);
    }
}
```
NOTE (implementer): if reading `evaluate_completion` shows an empty-steps plan does NOT yield `Satisfied`, adjust `run_with`/the seed plan in `invoke_runs_loop_to_terminal_status` so completion is deterministic (e.g. give the plan one step already `PlanStepStatus::Done`), keep the 3-response script aligned to the resulting LLM call count, and assert the actual terminal status. Confirm exact field names of `ReviewOutcome`/`PlanDocument` from their crates (the literals above mirror the harness + reflection tests).

- [ ] **Step 4: Run tests — verify they FAIL**

Run: `CARGO_NET_GIT_FETCH_WITH_CLI=true cargo test -p greentic-dw-operala-invoker`
Expected: FAIL (symbols not defined).

- [ ] **Step 5: Implement `DeepWorkerInvoker` + helpers (GREEN)**

In `lib.rs`, above the tests:
```rust
//! Production [`OperalaDispatchInvoker`]: wires the five deep-worker providers
//! into a [`DeepLoopCoordinator`] and runs it on a blocking thread.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use greentic_dw_core::RuntimeOperation;
use greentic_dw_engine::{EngineDecision, StaticEngine};
use greentic_dw_operala_bridge::{InvokeOutcome, OperalaDispatchInvoker};
use greentic_dw_planning::CreatePlanRequest;
use greentic_dw_planning::PlanningProvider;
use greentic_dw_runtime::{DeepLoopCoordinator, DeepLoopRun, DeepLoopStatus, DwRuntime};
use greentic_dw_types::{
    LocaleContext, LocalePropagation, OutputLocaleGuidance, TaskEnvelope, TaskLifecycleState,
    TenantScope, WorkerLocalePolicy,
};
use greentic_dw_workspace::WorkspaceScope;
use greentic_dw_context_llm::LlmContextProvider;
use greentic_dw_delegation_llm::LlmDelegationProvider;
use greentic_dw_planning_llm::LlmPlanningProvider;
use greentic_dw_reflection_llm::LlmReflectionProvider;
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
        scope: TenantScope { tenant: tenant.to_string(), team: None },
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
            let workspace: Arc<InMemoryWorkspaceProvider> = Arc::new(InMemoryWorkspaceProvider::new());
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
            let context = LlmContextProvider::new(Arc::clone(&llm), Arc::clone(&workspace) as Arc<dyn greentic_dw_workspace::WorkspaceProvider>, scope);

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
        .await??;

        Ok(outcome)
    }
}
```
Adjust the `Arc::clone(&workspace) as Arc<dyn ...>` coercion form if the compiler prefers an explicit `let ws_dyn: Arc<dyn WorkspaceProvider> = workspace.clone();` binding. Confirm `DeepLoopStatus`/`DeepLoopRun` are imported from `greentic_dw_runtime` (they are re-exported from the crate root).

- [ ] **Step 6: Run tests — verify they PASS**

Run: `CARGO_NET_GIT_FETCH_WITH_CLI=true cargo test -p greentic-dw-operala-invoker`
Expected: all PASS (3 helper tests + 1 end-to-end invoke).

- [ ] **Step 7: clippy + fmt**

Run: `CARGO_NET_GIT_FETCH_WITH_CLI=true cargo clippy -p greentic-dw-operala-invoker --all-targets -- -D warnings` (clean); `cargo fmt -p greentic-dw-operala-invoker`.

- [ ] **Step 8: Commit**

```bash
git add crates/greentic-dw-operala-invoker Cargo.toml Cargo.lock
git commit -m "feat(operala-invoker): production OperalaDispatchInvoker wiring DeepLoopCoordinator (deep-worker brain slice 5)"
```

---

## Manual verification

`cargo test -p greentic-dw-operala-invoker` green; `DeepWorkerInvoker::new(llm)` usable as an `OperalaDispatchInvoker`; the end-to-end test drives a real `DeepLoopCoordinator` to a terminal status via a scripted LLM.

## Self-Review (during planning)

- **Spec coverage:** §1 contract → Task 1; §2 design → Steps 2/5 (spawn_blocking, 3 helpers, struct-literal coordinator); §4 testing → Step 3 (3 unit + 1 e2e with serialized-struct scripting).
- **Placeholders:** none — full code in Steps 2/3/5. The two "confirm exact" notes (evaluate_completion empty-steps verdict; ReviewOutcome/PlanDocument field names; Arc-coercion form) are explicit with fallbacks.
- **Type consistency:** `TaskEnvelope`/`LocaleContext`/`TenantScope` fields + enum variants match the verified recipe; `DwRuntime::new(StaticEngine::new(EngineDecision::Operation(RuntimeOperation::Step)))`; provider constructors per their crates; `CreatePlanRequest{goal,assumptions,constraints,success_criteria}`; `InvokeOutcome{ok,output,events}` + `DeepLoopRun{status,output_artifact_ids}` from the seam/runtime. `spawn_blocking` closure is `Send + 'static` (captures Arc + Strings + Value).
