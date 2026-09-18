# LLM-backed ReflectionProvider (Design) — deep-worker brain, slice 2

- **Date:** 2026-06-21
- **Status:** Design approved, ready for planning
- **Surface:** greentic-dw (new crate `greentic-dw-reflection-llm`). greentic-dw only.
- **Part of:** SP-3 deep-worker brain. Slice 2, a direct mirror of the merged `greentic-dw-planning-llm` (slice 1) for the reflection domain.

## 1. Background

`DeepLoopCoordinator` needs `&dyn ReflectionProvider`; only mocks exist. This adds the LLM-backed one, structurally identical to `greentic-dw-planning-llm` (which is now on `research` and serves as the reference template: `crates/greentic-dw-planning-llm/src/{bridge,prompt,lib}.rs`).

## 2. Contract (verified)

`greentic_dw_reflection::ReflectionProvider` — **3 sync methods**, all returning `ReviewOutcome`:
- `review_step(ReviewStepRequest{plan_step_id, output_artifact_ref, context}) -> ReviewOutcome`
- `review_plan(ReviewPlanRequest{plan_id, revision}) -> ReviewOutcome`
- `review_final(ReviewFinalRequest{run_id, output_artifact_ref, context}) -> ReviewOutcome`

`ReviewOutcome { verdict: ReviewVerdict(accept|revise|retry|delegate|fail), score: Option<f32>, findings: Vec<ReviewFinding>, suggested_actions: Vec<SuggestedAction>, binding: bool }` derives serde + `schemars::JsonSchema`, and has `ReviewOutcome::validate(&self) -> Result<(), ReflectionError>` (score ∈ 0..=1; findings need non-empty code/message/target.reference; suggested_actions need non-empty action/target.reference). `ReflectionError { Validation(String), Provider(String) }`.

All 3 methods are LLM-driven (review = judgment); there are no deterministic methods (unlike planning).

## 3. Design (mirror planning-llm)

New crate `greentic-dw-reflection-llm`:
- `src/bridge.rs` — copy planning-llm's panic-safe `block_on` verbatim (small; a future `greentic-dw-llm-common` extraction can dedupe across the provider crates — noted, not done now).
- `src/prompt.rs` — `extract_json` (copy), `system_for_review_step/plan/final()` embedding the `schemars` schema of `ReviewOutcome` + "respond with ONLY a JSON object matching this schema", `user_for_*` serializing the request.
- `src/lib.rs` — `LlmReflectionProvider { llm: Arc<dyn greentic_llm::LlmProvider> }` + `new`; private `complete_json::<T: DeserializeOwned>(&self, system, user) -> Result<T, ReflectionError>` (copy planning's, mapping errors to `ReflectionError::Provider`); the 3 methods each do `let outcome: ReviewOutcome = self.complete_json(...)?; outcome.validate()?; Ok(outcome)`. `StubLlm` test double (copy planning's).
- `Cargo.toml` — deps: `greentic-dw-reflection { workspace = true }`, `greentic-llm = { git = "https://github.com/greenticai/greentic-llm", tag = "v1.2.6-research" }`, `serde`, `serde_json`, `schemars`, `tokio` (rt, rt-multi-thread), `async-trait`, `futures-util`; dev: `tokio` (macros). Add to workspace `members`.

## 4. Error handling

LLM/parse errors → `ReflectionError::Provider`; `ReviewOutcome::validate` failure → its `ReflectionError::Validation`. No panics; no `unwrap`/`expect` in non-test code (except the bridge's documented transient-runtime build).

## 5. Testing (stub LLM)

- `review_step`/`review_plan`/`review_final`: stub returns a valid `ReviewOutcome` JSON (e.g. `{"verdict":"accept"}`) → `Ok`; stub returns an outcome with `score: 1.5` → `Validation` (via `ReviewOutcome::validate`); stub returns non-JSON → `Provider`; stub returns fenced JSON → `Ok` (extract_json).
- `extract_json` units (copy planning's).
- bridge smoke test.

## 6. Risks / limitations

- Prompts first-draft (stub-tested; live tuning later) — same posture as planning-llm.
- Small duplication of bridge/complete_json/StubLlm with planning-llm; future `greentic-dw-llm-common` extraction noted.
- One of three brain providers; delegation provider + operala invoker wiring remain separate slices.
