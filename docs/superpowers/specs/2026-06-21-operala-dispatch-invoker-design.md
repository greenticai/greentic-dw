# Production OperalaDispatchInvoker (Design) — deep-worker brain, slice 5

- **Date:** 2026-06-21
- **Status:** Design approved (user chose "OperalaDispatchInvoker (greentic-dw)"), ready for planning
- **Surface:** greentic-dw (new crate `greentic-dw-operala-invoker`).
- **Part of:** SP-3 deep-worker brain, slice 5 — the **integration** that connects the five providers (planning/reflection/delegation/context/workspace, all on `research`) to the `DeepLoopCoordinator` and exposes them through the `OperalaDispatchInvoker` seam (`greentic-dw-operala-bridge`, PR #81). First production crate to combine `greentic-dw-runtime` with the `*-llm` provider crates.

## 1. Contract (verified)

Seam (from `greentic-dw-operala-bridge`):
```rust
#[async_trait]
pub trait OperalaDispatchInvoker: Send + Sync {
    async fn invoke(&self, tenant: &str, env: &str, target: &str, operation: &str,
                    input: serde_json::Value, idempotency_key: Option<&str>) -> anyhow::Result<InvokeOutcome>;
}
pub struct InvokeOutcome { pub ok: bool, pub output: serde_json::Value, pub events: Vec<serde_json::Value> }
```

`DeepLoopCoordinator<'a, E: DwEngine>` (from `greentic-dw-runtime`) is a struct-literal of borrows: `runtime: &DwRuntime<E>`, `planner: &dyn PlanningProvider`, `context: &dyn ContextProvider`, `workspace: &dyn WorkspaceProvider`, `reflector: &dyn ReflectionProvider`, `delegator: &dyn DelegationProvider`. Entry: `run(&self, envelope: &mut TaskEnvelope, plan: PlanDocument) -> Result<DeepLoopRun, DeepLoopError>`. **`run` does NOT call `create_plan`** — the caller seeds the plan and passes it in. `DeepLoopRun{plan, status: DeepLoopStatus, emitted_subtasks, output_artifact_ids: Vec<String>}`.

Construction facts (verified): `TaskId/WorkerId/TenantId/TeamId` are `String` aliases. `DwRuntime::new(StaticEngine::new(EngineDecision::Operation(RuntimeOperation::Step)))`. Provider constructors: `LlmPlanningProvider::new(Arc<dyn LlmProvider>)`, same for reflection/delegation; `LlmContextProvider::new(llm, Arc<dyn WorkspaceProvider>, WorkspaceScope)`; `InMemoryWorkspaceProvider::new()`. `CreatePlanRequest{goal, assumptions, constraints, success_criteria}`.

## 2. Design

New crate `greentic-dw-operala-invoker`:

```rust
pub struct DeepWorkerInvoker { llm: Arc<dyn greentic_llm::LlmProvider> }
impl DeepWorkerInvoker { pub fn new(llm: Arc<dyn greentic_llm::LlmProvider>) -> Self }
```

`invoke` orchestration (async): the deep loop is **synchronous** and the providers bridge to async LLM calls via `bridge::block_on`, so the whole construct-and-run is wrapped in **`tokio::task::spawn_blocking`** (the loop never blocks the async executor; on a blocking thread `block_on` spins a transient current-thread runtime). The closure:
1. `let workspace = Arc::new(InMemoryWorkspaceProvider::new());` (fresh per dispatch).
2. Build the five providers from the cloned `llm` + a `WorkspaceScope{tenant, team:None, session: task_id, agent: Some(target), run: task_id}`.
3. `let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Operation(RuntimeOperation::Step)));`
4. `let plan = planner.create_plan(CreatePlanRequest{ goal: extract_goal(&input), assumptions: vec![], constraints: vec![], success_criteria: vec!["task completed".into()] })?;`
5. `let mut envelope = build_envelope(tenant, target, &task_id);`
6. `let coordinator = DeepLoopCoordinator{ runtime: &runtime, planner: &planner, context: &context, workspace: workspace.as_ref(), reflector: &reflector, delegator: &delegator };`
7. `let run = coordinator.run(&mut envelope, plan)?;`
8. `Ok(outcome_from_run(&run))`

Three **pure free functions** carry the invoker's own logic (independently unit-tested):
- `extract_goal(input: &Value) -> String` — `input["goal"]` else `input["user_text"]` else `"Execute the requested task"`.
- `build_envelope(tenant, target, task_id) -> TaskEnvelope` — direct struct literal: `state: Created`, `scope: TenantScope{tenant, team: None}`, `locale: LocaleContext{worker_default_locale:"en-US", requested_locale:None, human_locale:None, policy: WorkerLocalePolicy::PreferRequested, propagation: LocalePropagation::PropagateToDelegates, output: OutputLocaleGuidance::MatchRequested}`.
- `outcome_from_run(run: &DeepLoopRun) -> InvokeOutcome` — `ok = matches!(run.status, DeepLoopStatus::Completed)`; `output = json!({"status": format!("{:?}", run.status), "artifact_ids": run.output_artifact_ids})`; `events = vec![]`.

`task_id` = `idempotency_key.unwrap_or("task-unknown")`.

## 3. Error handling

`create_plan` (`PlanningError`) and `run` (`DeepLoopError`) propagate via `?` into `anyhow::Result` inside the `spawn_blocking` closure; the `JoinHandle` is awaited and a join error maps to `anyhow`. The bridge maps these to a `{code:"invoke_failed"}` response (in `greentic-dw-operala-bridge::build_response`). No `unwrap`/`expect` in non-test code.

## 4. Testing

- `extract_goal`: `{"goal":"X"}` → "X"; `{"user_text":"Y"}` → "Y"; `{}` → default.
- `build_envelope`: fields populated correctly (tenant, worker_id==target, state Created, team None).
- `outcome_from_run`: a `DeepLoopRun{status: Completed, output_artifact_ids:["a"]}` → `ok==true`, output status "Completed", artifact_ids carried; a `Failed` run → `ok==false`.
- **End-to-end `invoke`** with a **scripted** stub LLM (a `Mutex<VecDeque<String>>` popped per `chat`): responses, in call order, are `[plan_json, "[]", review_json]` where `plan_json = serde_json::to_string(&PlanDocument{minimal, empty steps})` and `review_json = serde_json::to_string(&ReviewOutcome{accept})` — **built by serializing the real structs** so the JSON always matches the live serde shape (no hand-written JSON). Assert `invoke(...).await` is `Ok`, `outcome.output["status"]` is present. (Implementer: verify `LlmPlanningProvider::evaluate_completion` yields `Satisfied` for an empty-steps plan so the loop completes deterministically with exactly those 3 LLM calls; if not, seed the plan so completion is deterministic, and assert the resulting terminal status.)

## 5. Limitations

- One in-memory workspace per dispatch (no cross-call persistence); `WorkspaceScope` derived from dispatch metadata. Persistent backend is future.
- `StaticEngine(Step)` mirrors the harness; richer engine policies are future.
- Locale defaults are fixed (`en-US`, PreferRequested); a manifest-driven envelope (`DigitalWorkerManifest::to_task_envelope`) is a future enhancement.
- This is the seam→loop wiring only. Remaining: runner in-proc operala serve spawn (constructs a `DeepWorkerInvoker` with the runner's configured LLM and calls `run_bridge`), designer deep-worker authoring, live-LLM prompt tuning.
