# LLM-backed PlanningProvider (Design) — deep-worker brain, slice 1

- **Date:** 2026-06-21
- **Status:** Design approved, ready for planning
- **Surface:** greentic-dw (new crate `greentic-dw-planning-llm`). greentic-dw only.
- **Part of:** SP-3 deep-worker "brain". This is the **first** concrete LLM-backed provider; reflection + delegation providers follow as separate slices.

## 1. Background

greentic-dw's `DeepLoopCoordinator` needs `&dyn PlanningProvider` (+ reflection/delegation/context/workspace). Today only **mocks** exist — there is no production planner. This crate ships the first real one: `LlmPlanningProvider`, backed by `greentic-llm` (the house multi-provider LLM abstraction, `LlmProvider::chat`).

## 2. The contract (verified)

`greentic_dw_planning::PlanningProvider` — **5 sync methods**:
- `create_plan(CreatePlanRequest{goal, assumptions, constraints, success_criteria}) -> PlanDocument`
- `revise_plan(RevisePlanRequest{plan, reason, context}) -> PlanRevision`
- `next_actions(NextActionsRequest{plan, context}) -> Vec<PlannedAction{step_id, action}>`
- `record_step_result(StepResultRequest{plan, step_id, status}) -> PlanDocument`
- `evaluate_completion(CompletionCheckRequest{plan}) -> CompletionState{Incomplete|Satisfied|Unsatisfied}`

All DTOs derive `serde` + `schemars::JsonSchema`. `PlanningError { Validation(String), Provider(String) }`. Validation rules in `greentic_dw_planning::validate` (non-empty plan_id/goal/success_criteria; unique non-empty step_ids; deps/edges reference existing steps; no self-deps/edges; `PlannedAction.step_id` must reference a plan step).

## 3. Key decisions

- **LLM:** `greentic-llm` (`RigBackend`/`LlmProvider`), injected as `Arc<dyn greentic_llm::LlmProvider>` (provider/model/credential chosen by the caller — the crate is LLM-agnostic). Dep: `greentic-llm = "=1.2.7-research"`.
- **Sync-over-async:** `LlmProvider::chat` is async; the trait is sync. Bridge via an owned current-thread `tokio::runtime::Runtime` created lazily (a `Once/Mutex`-guarded handle) and `block_on`, using `tokio::task::block_in_place` when already inside a multi-thread runtime so it never panics. **Contract:** the deep-loop must invoke the provider from a blocking context (the production operala invoker calls `DeepLoopCoordinator::run` via `spawn_blocking`, mirroring aw-runtime's sync-tool bridge).
- **Structured output:** `greentic-llm` has no JSON mode. Prompt instructs "respond with ONLY this JSON" (schema embedded via `schemars` schema-for the target type), then `serde_json::from_str` the reply. A lenient extractor strips ```json fences / surrounding prose before parsing. Parse failure → `PlanningError::Provider`.
- **LLM-backed vs deterministic:**
  - LLM: `create_plan` (goal → full `PlanDocument`), `next_actions` (plan+context → `Vec<PlannedAction>`), `revise_plan` (plan+reason → `PlanRevision`). These three share ONE helper `complete_json::<T>(system, user) -> Result<T, PlanningError>`.
  - Deterministic (no LLM): `record_step_result` (set the step's status; mark steps whose deps are all terminal `Ready`), `evaluate_completion` (all steps terminal → `Satisfied`, any failed → `Unsatisfied`, else `Incomplete`).
- **Validation gate:** every LLM-produced `PlanDocument` is run through `greentic_dw_planning::validate::*` before returning; `next_actions` validates each `step_id` against the plan. Invalid → `PlanningError::Validation`.
- **Prompts are first-draft** (refinable with live-LLM tuning later); correctness of this slice is proven against a **stub LLM** in tests, independent of prompt quality.

## 4. Components / file structure (`crates/greentic-dw-planning-llm/`)
- `Cargo.toml` — deps: `greentic-dw-planning` (workspace), `greentic-llm` (=1.2.7-research), `serde_json`, `tokio` (rt + rt-multi-thread), `schemars` (for schema-in-prompt), `serde`. dev: `tokio` (macros), `async-trait` (for the stub).
- `src/lib.rs` — `LlmPlanningProvider { llm: Arc<dyn LlmProvider>, rt: … }` + `PlanningProvider` impl (the 2 deterministic methods inline; the 3 LLM methods delegate to helpers).
- `src/bridge.rs` — the sync/async `block_on` bridge.
- `src/prompt.rs` — system+user prompt builders per method + JSON-fence extractor.
- (tests inline) — `StubLlm` implementing `greentic_llm::LlmProvider`.

## 5. Data flow

deep-loop (sync, on a blocking thread) → `LlmPlanningProvider::next_actions` → `prompt` → `bridge::block_on(llm.chat(req))` → extract JSON → `serde_json::from_str::<Vec<PlannedAction>>` → validate step_ids → return. (create_plan/revise_plan analogous with their target types.)

## 6. Error handling

- `chat` error → `PlanningError::Provider(msg)`. JSON extract/parse failure → `Provider(msg)`. Validation failure (invalid plan / unknown step_id) → `Validation(msg)`. No panics; no `unwrap`/`expect` in non-test code.

## 7. Testing (stub LLM — deterministic, no network)

- `StubLlm` returns a canned `ChatResponse{content}`; tests feed valid/invalid JSON.
- `next_actions`: stub returns `[{"step_id":"s1","action":"execute"}]` for a plan with step `s1` → `Ok` with that action; stub returns an unknown step_id → `Validation`; stub returns non-JSON → `Provider`.
- `create_plan`: stub returns a valid `PlanDocument` JSON → `Ok` and passes `validate`; stub returns a plan with empty `success_criteria` → `Validation`.
- `revise_plan`: stub returns a `PlanRevision` JSON → `Ok`.
- deterministic: `record_step_result` sets the step status + readies dependents; `evaluate_completion` returns Satisfied/Incomplete/Unsatisfied per step states.
- bridge: a smoke test that `block_on` runs a trivial async future to completion.

## 8. Risks / limitations (explicit)

- **Prompt quality is first-draft**; real planning quality needs live-LLM iteration — out of scope for this slice (stub-tested). Documented in the crate.
- **Sync/async bridge** must be called from a blocking context; documented + the production operala invoker (separate slice) is responsible for `spawn_blocking`.
- This is **one** of three brain providers; a working deep-worker also needs reflection + delegation providers + the operala invoker wiring (separate slices).
