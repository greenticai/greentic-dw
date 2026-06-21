# LLM-backed DelegationProvider — Implementation Plan (deep-worker brain, slice 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** New `greentic-dw-delegation-llm` crate: `LlmDelegationProvider` implementing `greentic_dw_delegation::DelegationProvider` — `choose_delegate` LLM-backed; `start_subtask`/`merge_result` deterministic. Mirrors the merged planning-llm / reflection-llm crates.

**Tech Stack:** Rust edition 2024, greentic-llm v1.2.6-research (git tag), tokio, serde/serde_json, schemars.

## Global Constraints

- Edition 2024. No `.unwrap()`/`.expect()` in non-test code (bridge transient-runtime build excepted).
- `greentic-llm = { git = "https://github.com/greenticai/greentic-llm", tag = "v1.2.6-research" }` (NO local path / patch).
- LLM/parse → `DelegationError::Provider`; choose_delegate light-validation → `DelegationError::Validation`.
- Conventional commits; NO Claude/AI co-author or attribution.
- Worktree `.worktrees/delegation-llm` (greentic-dw), branch `feat/llm-delegation-provider`. greentic-dw pushes via SSH.
- ALWAYS prefix cargo with `CARGO_NET_GIT_FETCH_WITH_CLI=true`. Scoped: `cargo build/test/clippy -p greentic-dw-delegation-llm`. If "No space left on device", STOP/report BLOCKED.

## Reference (mirror — on this branch)

`crates/greentic-dw-reflection-llm/src/{bridge.rs, prompt.rs, lib.rs}` + `Cargo.toml` (and/or `greentic-dw-planning-llm`). Copy `block_on`, `extract_json`, `json_schema_for`, `complete_json`, `StubLlm`. Contract: `greentic_dw_delegation::{DelegationProvider, DelegationRequest, DelegationDecision, DelegationMode, MergePolicy, StartSubtaskRequest, DelegationHandle, MergeSubtaskResultRequest, SubtaskResultEnvelope, DelegationMergeResult, DelegationError}`.

---

## Task 1: `greentic-dw-delegation-llm` crate

**Files:**
- Create: `crates/greentic-dw-delegation-llm/{Cargo.toml, src/lib.rs, src/bridge.rs, src/prompt.rs}`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Produces: `LlmDelegationProvider::new(llm: Arc<dyn greentic_llm::LlmProvider>) -> Self` implementing `DelegationProvider`.

- [ ] **Step 1: Read the mirror reference**

Read `crates/greentic-dw-reflection-llm/src/{bridge.rs, prompt.rs, lib.rs}` + `Cargo.toml`. Copy `block_on`, `extract_json`, `json_schema_for`, `complete_json`, `StubLlm`.

- [ ] **Step 2: Cargo.toml + member**

Copy reflection-llm's Cargo.toml; `name = "greentic-dw-delegation-llm"`, description for delegation, dep `greentic-dw-delegation = { workspace = true }` (path dep if no workspace entry), keep `greentic-llm` git tag. Add `"crates/greentic-dw-delegation-llm",` to root `Cargo.toml` `[workspace] members`.

- [ ] **Step 3: bridge.rs + prompt.rs**

Copy `bridge.rs` verbatim. In `prompt.rs`, copy `extract_json` + `json_schema_for` + the `extract_json` tests, and add:
```rust
pub fn system_for_choose_delegate() -> String {
    format!(
        "You are a delegation strategist in a deep-worker system. Decide whether and how to \
delegate the goal to sub-agents. Respond with ONLY a JSON object matching this schema (no prose, \
no markdown fences):\n\n{}",
        json_schema_for::<greentic_dw_delegation::DelegationDecision>()
    )
}
pub fn user_for_choose_delegate(req: &greentic_dw_delegation::DelegationRequest) -> String {
    serde_json::to_string_pretty(req).unwrap_or_else(|_| "{}".into())
}
```

- [ ] **Step 4: lib.rs — provider + 3 methods + tests (TDD)**

Copy `complete_json` (errors → `DelegationError::Provider`) + `StubLlm`. Implement:
```rust
use greentic_dw_delegation::*;
pub struct LlmDelegationProvider { llm: std::sync::Arc<dyn greentic_llm::LlmProvider> }
impl LlmDelegationProvider {
    pub fn new(llm: std::sync::Arc<dyn greentic_llm::LlmProvider>) -> Self { Self { llm } }
    fn complete_json<T: serde::de::DeserializeOwned>(&self, system: &str, user: String) -> Result<T, DelegationError> { /* mirror; map to Provider */ }
    fn is_success_status(status: &str) -> bool {
        let s = status.trim();
        s.eq_ignore_ascii_case("success") || s.eq_ignore_ascii_case("succeeded") || s.eq_ignore_ascii_case("completed")
    }
}
impl DelegationProvider for LlmDelegationProvider {
    fn choose_delegate(&self, req: DelegationRequest) -> Result<DelegationDecision, DelegationError> {
        let decision: DelegationDecision =
            self.complete_json(&crate::prompt::system_for_choose_delegate(), crate::prompt::user_for_choose_delegate(&req))?;
        if decision.mode != DelegationMode::None && decision.target_agents.is_empty() {
            return Err(DelegationError::Validation(
                "delegation decision selects a mode but names no target agents".into(),
            ));
        }
        Ok(decision)
    }
    fn start_subtask(&self, req: StartSubtaskRequest) -> Result<DelegationHandle, DelegationError> {
        Ok(DelegationHandle { subtask_id: req.envelope.subtask_id, target_agent: req.envelope.target_agent })
    }
    fn merge_result(&self, req: MergeSubtaskResultRequest) -> Result<DelegationMergeResult, DelegationError> {
        let non_empty = |r: &SubtaskResultEnvelope| !r.output_artifact_ref.trim().is_empty();
        match req.merge_policy {
            MergePolicy::FirstSuccess => {
                match req.results.iter().find(|r| Self::is_success_status(&r.status) && non_empty(r)) {
                    Some(r) => Ok(DelegationMergeResult {
                        accepted_artifact_refs: vec![r.output_artifact_ref.clone()],
                        summary: format!("first successful subtask: {}", r.subtask_id),
                    }),
                    None => Ok(DelegationMergeResult {
                        accepted_artifact_refs: vec![],
                        summary: "no successful subtask result".into(),
                    }),
                }
            }
            other => {
                let refs: Vec<String> = req.results.iter().filter(|r| non_empty(r)).map(|r| r.output_artifact_ref.clone()).collect();
                let n = refs.len();
                Ok(DelegationMergeResult { accepted_artifact_refs: refs, summary: format!("merged {n} subtask result(s) under {other:?}") })
            }
        }
    }
}
```
(Adapt field names to the real DTOs; confirm `DelegationMode`/`MergePolicy` derive `PartialEq`/`Debug` — they do.)

Tests FIRST (StubLlm): choose_delegate valid → Ok; `mode:"single", target_agents:[]` → Validation; non-JSON → Provider; fenced JSON → Ok. start_subtask echoes envelope ids. merge_result FirstSuccess over `[{status:"failed",ref:"x"},{status:"success",ref:"y"}]` → `["y"]`; CollectAll over two non-empty → both; empty results → empty. RED → implement → GREEN.

- [ ] **Step 5: build/test/clippy/fmt**

`CARGO_NET_GIT_FETCH_WITH_CLI=true cargo test -p greentic-dw-delegation-llm` (all pass); `... cargo clippy -p greentic-dw-delegation-llm --all-targets -- -D warnings` clean; `cargo fmt -p greentic-dw-delegation-llm`.

- [ ] **Step 6: commit**

```bash
git add crates/greentic-dw-delegation-llm Cargo.toml Cargo.lock
git commit -m "feat(delegation-llm): LLM-backed DelegationProvider (deep-worker brain slice 3)"
```

---

## Manual verification

`cargo test -p greentic-dw-delegation-llm` green; `LlmDelegationProvider::new(llm)` usable as a `DelegationProvider`.

## Self-Review (during planning)

- **Spec coverage:** §2 design → Task 1; §4 testing folded in.
- **Placeholders:** structure copies the merged planning/reflection crates (named files); choose_delegate (LLM) + deterministic start_subtask/merge_result are concrete code above. Only reads: the mirror crates + confirming `greentic-dw-delegation` workspace-dep form + exact DTO field names.
- **Type consistency:** `complete_json::<DelegationDecision>`, `DelegationMode::None`, `MergePolicy::*`, `DelegationError::{Provider,Validation}` consistent.
