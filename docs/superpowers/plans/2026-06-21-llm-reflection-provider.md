# LLM-backed ReflectionProvider — Implementation Plan (deep-worker brain, slice 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** New `greentic-dw-reflection-llm` crate: `LlmReflectionProvider` implementing `greentic_dw_reflection::ReflectionProvider` (review_step/review_plan/review_final → `ReviewOutcome`) via `greentic-llm`, a direct mirror of the merged `greentic-dw-planning-llm`.

**Architecture:** Same as planning-llm — sync trait over async LLM via `block_on`; prompt → chat → `extract_json` → parse `ReviewOutcome` → `ReviewOutcome::validate()`. Stub-LLM tested.

**Tech Stack:** Rust edition 2024, greentic-llm v1.2.6-research (git tag), tokio, serde/serde_json, schemars.

## Global Constraints

- Edition 2024, rust 1.94. No `.unwrap()`/`.expect()` in non-test code (except the bridge's documented transient-runtime build).
- `greentic-llm = { git = "https://github.com/greenticai/greentic-llm", tag = "v1.2.6-research" }` (NOT a local path; NOT 1.2.7). No `[patch.crates-io]` for greentic-llm.
- Map LLM/parse errors → `ReflectionError::Provider`; `ReviewOutcome::validate()` failure → its `ReflectionError::Validation` (propagate via `?`).
- Conventional commits; NO Claude/AI co-author or attribution.
- Worktree `.worktrees/reflection-llm` (greentic-dw), branch `feat/llm-reflection-provider`. greentic-dw pushes via SSH.
- ALWAYS prefix cargo with `CARGO_NET_GIT_FETCH_WITH_CLI=true` (private git dep). Scoped: `cargo build/test/clippy -p greentic-dw-reflection-llm`. If "No space left on device", STOP/report BLOCKED.

## Reference (mirror these — already on this branch)

- `crates/greentic-dw-planning-llm/src/bridge.rs` — copy `block_on` verbatim.
- `crates/greentic-dw-planning-llm/src/prompt.rs` — copy `extract_json` + `json_schema_for::<T>()`; adapt the `system_for_*`/`user_for_*` to reflection.
- `crates/greentic-dw-planning-llm/src/lib.rs` — copy the `complete_json` helper + `StubLlm` test double; adapt the provider struct + methods to reflection.
- `crates/greentic-dw-planning-llm/Cargo.toml` — copy deps (swap `greentic-dw-planning` → `greentic-dw-reflection`).
- Contract: `greentic_dw_reflection::{ReflectionProvider, ReviewStepRequest, ReviewPlanRequest, ReviewFinalRequest, ReviewOutcome, ReviewVerdict, ReflectionError}`. `ReviewOutcome::validate(&self) -> Result<(), ReflectionError>` exists — use it.

---

## Task 1: `greentic-dw-reflection-llm` crate (mirror planning-llm)

**Files:**
- Create: `crates/greentic-dw-reflection-llm/{Cargo.toml, src/lib.rs, src/bridge.rs, src/prompt.rs}`
- Modify: `Cargo.toml` (workspace `members`)

**Interfaces:**
- Produces: `LlmReflectionProvider::new(llm: Arc<dyn greentic_llm::LlmProvider>) -> Self` implementing `ReflectionProvider`.

- [ ] **Step 1: Read the mirror reference**

Read `crates/greentic-dw-planning-llm/src/{bridge.rs, prompt.rs, lib.rs}` and `Cargo.toml`. These are the exact template. Note the `complete_json`, `StubLlm`, `extract_json`, `json_schema_for`, and the `greentic-llm` ChatRequest/ChatMessage usage to copy.

- [ ] **Step 2: Cargo.toml + workspace member**

Create `crates/greentic-dw-reflection-llm/Cargo.toml` copying planning-llm's, with `name = "greentic-dw-reflection-llm"`, `description = "LLM-backed ReflectionProvider for the Greentic deep-worker (greentic-llm)."`, and dependency `greentic-dw-reflection = { workspace = true }` instead of `greentic-dw-planning`. Keep `greentic-llm = { git = ..., tag = "v1.2.6-research" }` and the rest identical. (If `greentic-dw-reflection` lacks a `[workspace.dependencies]` entry, use a path dep `{ path = "../greentic-dw-reflection" }`.) Add `"crates/greentic-dw-reflection-llm",` to the root `Cargo.toml` `[workspace] members`.

- [ ] **Step 3: bridge.rs**

Copy `crates/greentic-dw-planning-llm/src/bridge.rs` verbatim (the `block_on` fn + its test).

- [ ] **Step 4: prompt.rs**

Copy `extract_json` + `json_schema_for::<T>()` + the `extract_json` unit tests verbatim. Replace the three planning `system_for_*`/`user_for_*` with reflection ones:
```rust
pub fn system_for_review_step() -> String {
    format!(
        "You are a reviewing assistant in a deep-worker system. Review the step's output and \
return your assessment. Respond with ONLY a JSON object matching this schema (no prose, no \
markdown fences):\n\n{}",
        json_schema_for::<greentic_dw_reflection::ReviewOutcome>()
    )
}
pub fn system_for_review_plan() -> String { /* same, wording: "Review the plan revision" */ }
pub fn system_for_review_final() -> String { /* same, wording: "Review the final output" */ }

pub fn user_for_review_step(req: &greentic_dw_reflection::ReviewStepRequest) -> String {
    serde_json::to_string_pretty(req).unwrap_or_else(|_| "{}".into())
}
pub fn user_for_review_plan(req: &greentic_dw_reflection::ReviewPlanRequest) -> String { /* same */ }
pub fn user_for_review_final(req: &greentic_dw_reflection::ReviewFinalRequest) -> String { /* same */ }
```

- [ ] **Step 5: lib.rs — provider + complete_json + StubLlm + tests (TDD)**

Copy planning-llm's `complete_json` (rename error mapping to `ReflectionError::Provider`) and `StubLlm` test double. Implement:
```rust
pub struct LlmReflectionProvider { llm: std::sync::Arc<dyn greentic_llm::LlmProvider> }
impl LlmReflectionProvider {
    pub fn new(llm: std::sync::Arc<dyn greentic_llm::LlmProvider>) -> Self { Self { llm } }
    fn complete_json<T: serde::de::DeserializeOwned>(&self, system: &str, user: String) -> Result<T, ReflectionError> { /* mirror planning, map errors to Provider */ }
}
impl greentic_dw_reflection::ReflectionProvider for LlmReflectionProvider {
    fn review_step(&self, req: ReviewStepRequest) -> Result<ReviewOutcome, ReflectionError> {
        let outcome: ReviewOutcome = self.complete_json(&prompt::system_for_review_step(), prompt::user_for_review_step(&req))?;
        outcome.validate()?;
        Ok(outcome)
    }
    fn review_plan(&self, req: ReviewPlanRequest) -> Result<ReviewOutcome, ReflectionError> { /* same w/ plan prompts */ }
    fn review_final(&self, req: ReviewFinalRequest) -> Result<ReviewOutcome, ReflectionError> { /* same w/ final prompts */ }
}
```

Write tests FIRST (RED) using the copied `StubLlm`:
- `review_step_parses_valid_outcome`: stub returns `{"verdict":"accept"}` → `Ok`, `verdict == Accept`.
- `review_step_invalid_score_is_validation`: stub returns `{"verdict":"revise","score":1.5}` → `Err(Validation)` (via `ReviewOutcome::validate`).
- `review_step_bad_json_is_provider`: stub returns `"nope"` → `Err(Provider)`.
- `review_step_handles_fenced_json`: stub returns ```json {"verdict":"fail"} ``` → `Ok`.
- `review_plan_parses` and `review_final_parses`: stub returns a valid outcome → `Ok`.

- [ ] **Step 6: Build + test + clippy + fmt**

`CARGO_NET_GIT_FETCH_WITH_CLI=true cargo test -p greentic-dw-reflection-llm` (all pass), `... cargo clippy -p greentic-dw-reflection-llm --all-targets -- -D warnings` clean, `cargo fmt -p greentic-dw-reflection-llm`.

- [ ] **Step 7: Commit**

```bash
git add crates/greentic-dw-reflection-llm Cargo.toml Cargo.lock
git commit -m "feat(reflection-llm): LLM-backed ReflectionProvider (deep-worker brain slice 2)"
```

---

## Manual verification

`cargo test -p greentic-dw-reflection-llm` green; `LlmReflectionProvider::new(llm)` usable as a `ReflectionProvider`. (Prompts first-draft; live-LLM tuning out of scope.)

## Self-Review (during planning)

- **Spec coverage:** §3 crate/bridge/prompt/lib → Task 1; §5 testing folded in.
- **Placeholders:** the structure copies the merged, reviewed planning-llm (named files to mirror); reflection-specific bits (3 methods, `ReviewOutcome::validate`, prompts) are concrete. The only reads are the planning-llm files (template) + confirming `greentic-dw-reflection` workspace-dep form.
- **Type consistency:** `complete_json::<ReviewOutcome>`, `ReviewOutcome::validate`, `ReflectionError::{Provider,Validation}` consistent throughout.
