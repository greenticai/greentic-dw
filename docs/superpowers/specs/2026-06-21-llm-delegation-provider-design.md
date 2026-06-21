# LLM-backed DelegationProvider (Design) — deep-worker brain, slice 3

- **Date:** 2026-06-21
- **Status:** Design approved, ready for planning
- **Surface:** greentic-dw (new crate `greentic-dw-delegation-llm`).
- **Part of:** SP-3 deep-worker brain, slice 3 (last of the three reasoning providers). Mirrors `greentic-dw-planning-llm` / `greentic-dw-reflection-llm` (both on `research`).

## 1. Contract (verified)

`greentic_dw_delegation::DelegationProvider` — **3 sync methods**:
- `choose_delegate(DelegationRequest{goal, candidate_agents}) -> DelegationDecision{mode: DelegationMode(none|single|parallel|map_reduce), target_agents, merge_policy: MergePolicy(first_success|collect_all|majority_vote|weighted_merge|reducer_artifact), rationale}` — **LLM-driven**.
- `start_subtask(StartSubtaskRequest{envelope: SubtaskEnvelope}) -> DelegationHandle{subtask_id, target_agent}` — **deterministic** (map envelope → handle).
- `merge_result(MergeSubtaskResultRequest{merge_policy, results: Vec<SubtaskResultEnvelope{subtask_id, status, output_artifact_ref, notes}>}) -> DelegationMergeResult{accepted_artifact_refs, summary}` — **deterministic** (apply policy).

All DTOs derive serde + `schemars::JsonSchema`. `DelegationError{Validation(String), Provider(String)}`. (No `validate()` helper on the DTOs.)

## 2. Design (mirror the two existing provider crates)

New crate `greentic-dw-delegation-llm`:
- `src/bridge.rs` — copy `block_on` verbatim (from reflection-llm/planning-llm).
- `src/prompt.rs` — copy `extract_json` + `json_schema_for`; add `system_for_choose_delegate()` (embed `DelegationDecision` schema, "respond with ONLY a JSON object…") + `user_for_choose_delegate(&DelegationRequest)` (serialize the request).
- `src/lib.rs` — `LlmDelegationProvider { llm: Arc<dyn greentic_llm::LlmProvider> }` + `new`; copy `complete_json` (errors → `DelegationError::Provider`) + `StubLlm`.
  - `choose_delegate`: `let decision: DelegationDecision = self.complete_json(&system_for_choose_delegate(), user_for_choose_delegate(&req))?;` + a light validation: if `decision.mode != None` and `decision.target_agents.is_empty()` → `DelegationError::Validation("delegation decision selects a mode but names no target agents")`. Return decision.
  - `start_subtask` (deterministic): `Ok(DelegationHandle { subtask_id: req.envelope.subtask_id, target_agent: req.envelope.target_agent })`.
  - `merge_result` (deterministic, per `req.merge_policy`):
    - `FirstSuccess`: first result whose `status` is success-like (`eq_ignore_ascii_case` to `"success"`/`"succeeded"`/`"completed"`) with a non-empty `output_artifact_ref` → `accepted = [that ref]`; else `accepted = []`. `summary` notes the chosen/none.
    - All other policies (`CollectAll`/`MajorityVote`/`WeightedMerge`/`ReducerArtifact`): `accepted = ` every result's non-empty `output_artifact_ref`; `summary = format!("merged {n} subtask result(s) under {policy:?}")`. (Sophisticated/LLM merge is a future enhancement; deterministic collect is a safe first cut.)
- `Cargo.toml` — mirror reflection-llm; dep `greentic-dw-delegation = { workspace = true }` (or path); `greentic-llm = { git = ..., tag = "v1.2.6-research" }`. Add to workspace `members`.

## 3. Error handling

LLM/parse → `DelegationError::Provider`; the choose_delegate light-validation → `Validation`. No panics; no `unwrap`/`expect` in non-test code (bridge transient-runtime build excepted).

## 4. Testing (stub LLM)

- `choose_delegate`: stub returns a valid `DelegationDecision` JSON (`mode:"single", target_agents:["a"], merge_policy:"first_success", rationale:"…"`) → `Ok`; `mode:"single", target_agents:[]` → `Validation`; non-JSON → `Provider`; fenced JSON → `Ok`.
- `start_subtask`: returns a handle echoing envelope `subtask_id`/`target_agent` (no LLM).
- `merge_result`: `FirstSuccess` over results `[failed, success]` → `accepted = [the success ref]`; `CollectAll` → all non-empty refs; empty results → empty accepted + summary.
- `extract_json` units + bridge smoke (copied).

## 5. Limitations

- Prompts first-draft (stub-tested); merge_result is a deterministic first cut (no LLM synthesis for weighted/reducer policies). Same duplication note (future `greentic-dw-llm-common`).
- Completes the 3 reasoning providers. Remaining for a working deep-worker: context + workspace providers, the production `OperalaDispatchInvoker` wiring `DeepLoopCoordinator`, runner serve spawn, designer authoring, live-LLM tuning.
