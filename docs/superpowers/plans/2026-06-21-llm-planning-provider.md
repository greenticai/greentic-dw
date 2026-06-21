# LLM-backed PlanningProvider — Implementation Plan (deep-worker brain, slice 1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** A new `greentic-dw-planning-llm` crate with `LlmPlanningProvider` — a `greentic_dw_planning::PlanningProvider` backed by `greentic-llm` (create_plan/revise_plan/next_actions via LLM; record_step_result/evaluate_completion deterministic), proven against a stub LLM.

**Architecture:** Sync trait over async LLM via a `block_on` bridge; structured output via prompt + lenient JSON parse + `greentic_dw_planning::validate`. LLM injected as `Arc<dyn greentic_llm::LlmProvider>`.

**Tech Stack:** Rust edition 2024, greentic-llm 1.2.7-research, tokio, serde/serde_json, schemars.

## Global Constraints

- Edition 2024, rust 1.94 (greentic-dw workspace). No `.unwrap()`/`.expect()` in non-test code.
- LLM-agnostic: depend only on `greentic_llm::LlmProvider` (caller supplies provider/model/credential). Dep `greentic-llm = "=1.2.7-research"`.
- Every LLM-produced `PlanDocument` passes `greentic_dw_planning::validate` before return; `next_actions` validates each `step_id` against the plan. Map LLM/parse errors → `PlanningError::Provider`; validation → `PlanningError::Validation`.
- Sync/async bridge must never panic (handle "already in a runtime"). Document that production calls happen on a blocking thread.
- Conventional commits; NO Claude/AI co-author or attribution.
- Worktree `.worktrees/planning-llm` (greentic-dw), branch `feat/llm-planning-provider`. greentic-dw pushes via SSH.
- Scoped builds: `cargo build/test/clippy -p greentic-dw-planning-llm`. Prefix `CARGO_NET_GIT_FETCH_WITH_CLI=true` if a private git dep fetch fails. If "No space left on device", STOP/report BLOCKED.

## Reference facts (verified)

- `PlanningProvider` (sync, 5 methods) + DTOs in `crates/greentic-dw-planning/src/{traits,model,error,validate}.rs`. `PlannedAction{step_id, action}`; `PlanDocument{plan_id, goal, status, revision, assumptions, constraints, success_criteria, steps, edges, metadata}`; `PlanStep{step_id, title, kind, status, depends_on, ...}`; `PlanStepStatus{Pending,Ready,Running,Blocked,Completed,Failed,Skipped}`; `CompletionState{Incomplete,Satisfied,Unsatisfied}`; `CreatePlanRequest{goal,assumptions,constraints,success_criteria}`; `RevisePlanRequest{plan,reason,context}`; `NextActionsRequest{plan,context}`; `StepResultRequest{plan,step_id,status}`; `CompletionCheckRequest{plan}`; `PlanRevision{revision,reason,changed_step_ids,metadata}`. `PlanningError{Validation(String),Provider(String)}`. All DTOs derive serde + schemars::JsonSchema.
- `greentic_dw_planning::validate` — public validator(s) for a `PlanDocument` (read the module for the exact fn name(s), e.g. `validate_plan(&PlanDocument) -> Result<(), ...>`). Reuse them; map their error to `PlanningError::Validation(e.to_string())`.
- greentic-llm public API (read `greentic-llm/src/provider.rs` for EXACT signatures): `#[async_trait] trait LlmProvider { fn capabilities(&self)->Capabilities; fn provider_name(&self)->&'static str; fn model(&self)->&str; async fn chat(&self, ChatRequest)->Result<ChatResponse,LlmError>; async fn chat_stream(&self, ChatRequest)->Result<ChatStream,LlmError>; }`. `ChatRequest{messages:Vec<ChatMessage>, tools, tool_choice, max_tokens, temperature}`; `ChatResponse{content:String, tool_calls, finish_reason}`; `ChatMessage` with `system(..)`/`user(..)` constructors (confirm in provider.rs). Mirror `greentic-designer-admin/src/llm_probe.rs` for usage.
- greentic-dw workspace `Cargo.toml`: add `crates/greentic-dw-planning-llm` to `members`; `workspace.dependencies` has `serde_json`, `thiserror`; declare `greentic-llm`, `tokio`, `schemars`, `serde` inline in the crate.

---

## Task 1: Crate scaffold + bridge + stub + deterministic methods

**Files:**
- Create: `crates/greentic-dw-planning-llm/Cargo.toml`, `src/lib.rs`, `src/bridge.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Produces: `LlmPlanningProvider::new(llm: Arc<dyn greentic_llm::LlmProvider>) -> Self`; `bridge::block_on<F: Future>(fut: F) -> F::Output`. Implements `record_step_result` + `evaluate_completion` (LLM methods return `PlanningError::Provider("not implemented")` until Task 2).

- [ ] **Step 1: Cargo.toml + workspace member**

`crates/greentic-dw-planning-llm/Cargo.toml`:
```toml
[package]
name = "greentic-dw-planning-llm"
version = { workspace = true }
edition = { workspace = true }
license = { workspace = true }
description = "LLM-backed PlanningProvider for the Greentic deep-worker (greentic-llm)."
publish = false

[dependencies]
greentic-dw-planning = { workspace = true }
greentic-llm = "=1.2.7-research"
serde = { version = "1", features = ["derive"] }
serde_json = { workspace = true }
schemars = "0.8"
tokio = { version = "1", features = ["rt", "rt-multi-thread"] }

[dev-dependencies]
async-trait = "0.1"
tokio = { version = "1", features = ["rt", "rt-multi-thread", "macros"] }
```
Add `"crates/greentic-dw-planning-llm",` to root `Cargo.toml` `[workspace] members`. (Confirm `greentic-dw-planning` has a `[workspace.dependencies.greentic-dw-planning]` entry; if not, use a relative path dep.)

- [ ] **Step 2: bridge.rs (sync/async, panic-safe) + test**

```rust
//! Run an async future to completion from a sync context without panicking,
//! whether or not a tokio runtime is already active.
use std::future::Future;

/// Block on `fut`. If called inside a multi-thread tokio runtime, uses
/// `block_in_place` + the current handle; otherwise spins up a transient
/// current-thread runtime. MUST NOT be called from a current-thread runtime's
/// async context (the deep-loop invokes providers on a blocking thread).
pub fn block_on<F: Future>(fut: F) -> F::Output {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(fut)),
        Err(_) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build transient current-thread runtime")
            .block_on(fut),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn block_on_runs_future() {
        assert_eq!(block_on(async { 1 + 1 }), 2);
    }
}
```
(The `.expect` is in the `Err` arm building a fresh runtime — this is non-test code; replace with a fallback that maps failure into the caller's error path if a no-`expect` policy is strict. Acceptable here as runtime-build failure is unrecoverable; if the reviewer flags it, change `block_on` to return `Result` and propagate. Implementer: prefer returning `Result` if it keeps callers clean.)

- [ ] **Step 3: lib.rs — struct + deterministic methods + LLM stubs**

Implement `LlmPlanningProvider { llm: Arc<dyn greentic_llm::LlmProvider> }`, `new`, and `impl PlanningProvider`:
- `record_step_result`: clone `req.plan`; set the matching step's `status = req.status`; then for every step whose `depends_on` are all terminal (`Completed`/`Skipped`) and which is `Pending`, set it `Ready`. Return the plan. (Generalises the mock; no hardcoded ids.)
- `evaluate_completion`: if any step `Failed` → `Unsatisfied`; else if all steps terminal (`Completed`/`Skipped`) → `Satisfied`; else `Incomplete`.
- `create_plan`/`revise_plan`/`next_actions`: `Err(PlanningError::Provider("not implemented".into()))` (filled in Task 2).

Add a `#[cfg(test)] mod tests` with a `StubLlm` (impl `greentic_llm::LlmProvider`; `chat` returns a field-stored canned `ChatResponse`; `chat_stream`/`capabilities`/`provider_name`/`model` minimal — read provider.rs for exact types) and tests for `record_step_result` (status set + dependent readied) and `evaluate_completion` (the 3 states).

- [ ] **Step 4: build + test + clippy**

`cargo test -p greentic-dw-planning-llm` (bridge + deterministic tests pass), `cargo clippy -p greentic-dw-planning-llm --all-targets -- -D warnings`, `cargo fmt -p greentic-dw-planning-llm`. (Prefix `CARGO_NET_GIT_FETCH_WITH_CLI=true` if needed.)

- [ ] **Step 5: commit**

```bash
git add crates/greentic-dw-planning-llm Cargo.toml
git commit -m "feat(planning-llm): scaffold crate + sync/async bridge + deterministic plan bookkeeping"
```

---

## Task 2: LLM-backed create_plan / next_actions / revise_plan

**Files:**
- Create: `crates/greentic-dw-planning-llm/src/prompt.rs`
- Modify: `crates/greentic-dw-planning-llm/src/lib.rs`

**Interfaces:**
- Consumes: `bridge::block_on`, the LLM, `greentic_dw_planning::validate`.
- Produces: the 3 LLM methods + a private `complete_json::<T: DeserializeOwned>(&self, system: &str, user: String) -> Result<T, PlanningError>` helper.

- [ ] **Step 1: prompt.rs — builders + JSON extractor**

- `extract_json(reply: &str) -> &str`: strip ```json / ``` fences and leading/trailing prose (find first `{`/`[` to last `}`/`]`).
- `system_for_next_actions()`, `system_for_create_plan()`, `system_for_revise_plan()`: instruct "respond with ONLY JSON matching this schema" and embed `serde_json::to_string_pretty(&schemars::schema_for!(T))` for the target type (`Vec<PlannedAction>` → schema_for PlannedAction; `PlanDocument`; `PlanRevision`).
- `user_for_*`: serialize the request (`serde_json::to_string_pretty(&req.plan)`, goal, context, reason) into a readable prompt.

- [ ] **Step 2: complete_json helper + the 3 methods (TDD)**

Add tests FIRST (stub returns canned JSON):
- `next_actions_parses_and_validates`: plan with step `s1`; stub `chat` returns `[{"step_id":"s1","action":"execute"}]` → `Ok(vec![PlannedAction{s1,execute}])`.
- `next_actions_rejects_unknown_step_id`: stub returns `[{"step_id":"ghost","action":"x"}]` → `Err(Validation)`.
- `next_actions_bad_json_is_provider_error`: stub returns `"not json"` → `Err(Provider)`.
- `create_plan_parses_and_validates`: stub returns a valid `PlanDocument` JSON (non-empty plan_id/goal/success_criteria, one step) → `Ok` (passes `validate`).
- `create_plan_invalid_plan_is_validation_error`: stub returns a plan with empty `success_criteria` → `Err(Validation)`.
- `revise_plan_parses`: stub returns a `PlanRevision` JSON → `Ok`.

Run RED, then implement:
- `complete_json`: build `ChatRequest{ messages: vec![ChatMessage::system(system), ChatMessage::user(user)], tools: vec![], tool_choice: None, max_tokens: Some(4096), temperature: Some(0.2) }`; `bridge::block_on(self.llm.chat(req)).map_err(|e| Provider(e.to_string()))?`; `extract_json(&resp.content)`; `serde_json::from_str::<T>(...).map_err(|e| Provider(...))`.
- `next_actions`: `let actions: Vec<PlannedAction> = self.complete_json(system_for_next_actions(), user_for_next_actions(&req))?;` then validate each `action.step_id` ∈ `req.plan.steps` (else `Validation`). Return actions.
- `create_plan`: `let plan: PlanDocument = self.complete_json(...)?;` then `greentic_dw_planning::validate::<the validator>(&plan).map_err(|e| Validation(e.to_string()))?;` return plan.
- `revise_plan`: `let rev: PlanRevision = self.complete_json(...)?;` return rev.

Run GREEN: `cargo test -p greentic-dw-planning-llm`.

- [ ] **Step 3: clippy + fmt + commit**

`cargo clippy -p greentic-dw-planning-llm --all-targets -- -D warnings` clean; `cargo fmt -p greentic-dw-planning-llm`.
```bash
git add crates/greentic-dw-planning-llm/src/lib.rs crates/greentic-dw-planning-llm/src/prompt.rs
git commit -m "feat(planning-llm): LLM-backed create_plan/next_actions/revise_plan with validation"
```

---

## Manual verification

`cargo test -p greentic-dw-planning-llm` green; the crate exposes `LlmPlanningProvider::new(llm)` usable as a `PlanningProvider`. (Real planning quality needs live-LLM prompt tuning — out of scope; tests use a stub.)

## Self-Review (during planning)

- **Spec coverage:** §4 crate/bridge/deterministic → Task 1; LLM methods + prompts + validation → Task 2; §7 testing folded in.
- **Placeholders:** bridge + deterministic semantics + helper/method structure are concrete; the only reads are greentic-llm's exact `ChatMessage`/`LlmProvider` signatures (named: `greentic-llm/src/provider.rs`, mirror `llm_probe.rs`) and `greentic_dw_planning::validate`'s fn name. These are real APIs to confirm, not invented.
- **Type consistency:** `complete_json::<T>` + `PlannedAction`/`PlanDocument`/`PlanRevision` + `PlanningError::{Provider,Validation}` consistent across tasks.
