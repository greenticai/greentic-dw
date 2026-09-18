# LLM-backed ContextProvider (Design) — deep-worker brain, slice 4b

- **Date:** 2026-06-21
- **Status:** Design approved (user chose "Workspace lalu Context"), ready for planning
- **Surface:** greentic-dw (new crate `greentic-dw-context-llm`).
- **Part of:** SP-3 deep-worker brain, slice 4b — the **last** of the five providers. Depends on slice 4a (`greentic-dw-workspace-mem`, merged) because `compress`/`summarize` write artifacts to a `WorkspaceProvider`. Mixed: `build_context` is deterministic; `compress_context`/`summarize_context` are LLM-backed.

## 1. Contract (verified)

`greentic_dw_context::ContextProvider` — **3 sync methods**:
- `build_context(BuildContextRequest{fragment_refs: Vec<String>, query: Option<String>, budget: ContextBudget{max_fragments,max_bytes}}) -> ContextPackage{package_id, fragments: Vec<ContextFragment>, budget}` — **deterministic assembly**.
- `compress_context(CompressContextRequest{package: ContextPackage}) -> CompressedContext{source_package_id, compressed_artifact_ref, fragment_count}` — **LLM**, writes the compressed text to the workspace.
- `summarize_context(SummarizeContextRequest{package: ContextPackage}) -> SummaryArtifactRef{artifact_ref}` — **LLM**, writes the summary to the workspace.

`ContextFragment{fragment_id, kind: ContextFragmentKind, content_ref, content: Option<String>, provenance, ordinal: u32}`. `ContextError{Validation(String), Provider(String)}`. Reusable validator: `validate_context_package(&ContextPackage) -> Result<(), ContextError>` (rejects empty `package_id`, zero budget, `fragments.len() > max_fragments`, out-of-order ordinals, empty fragment id/content_ref/provenance).

Workspace contract (from slice 4a): `greentic_dw_workspace::{WorkspaceProvider, CreateArtifactRequest, ArtifactRef, ArtifactMetadata, ArtifactKind, WorkspaceScope, WorkspaceError}`. `create_artifact(CreateArtifactRequest{artifact: ArtifactRef{artifact_id, kind, scope}, metadata: ArtifactMetadata{title, tags, mime_type}, body}) -> ArtifactRef`.

LLM contract (from the three reasoning providers): `greentic_llm::{LlmProvider, ChatRequest, ChatMessage, ChatResponse}`; `llm.chat(ChatRequest{messages, tools, tool_choice, max_tokens, temperature}) -> Result<ChatResponse{content,..}, LlmError>` (async). Sync bridge: `bridge::block_on`.

## 2. Design

New crate `greentic-dw-context-llm`:

```rust
pub struct LlmContextProvider {
    llm: Arc<dyn greentic_llm::LlmProvider>,
    workspace: Arc<dyn greentic_dw_workspace::WorkspaceProvider>,
    scope: greentic_dw_workspace::WorkspaceScope,   // the run scope artifacts are written under
    counter: std::sync::atomic::AtomicU64,           // monotonic id source (no random/time)
}
```
Constructor: `new(llm, workspace, scope) -> Self`.

- `src/bridge.rs` — `block_on` copied verbatim from the reasoning crates.
- `src/prompt.rs` — `render_package(&ContextPackage) -> String` (each fragment as a line: `[{ordinal}] {kind:?} {content_ref}` plus inline `content` when present); `system_for_compress()` / `system_for_summarize()` (text-only instructions, NOT JSON); `user_for_compress(&str)` / `user_for_summarize(&str)` (wrap the rendered package).
- `src/lib.rs` — provider + two private helpers:
  - `complete_text(system, user) -> Result<String, ContextError>` — build a `ChatRequest` (max_tokens ~2048, temperature 0.2), `bridge::block_on(self.llm.chat(req))`, map `LlmError` → `ContextError::Provider`, return `response.content` **verbatim** (no JSON parse).
  - `write_artifact(id, kind, title, body) -> Result<(), ContextError>` — `self.workspace.create_artifact(...)` under `self.scope`, map `WorkspaceError` → `ContextError::Provider(format!("workspace write failed: {e}"))`.

Method semantics:
- `build_context`: take `min(fragment_refs.len(), budget.max_fragments as usize)` refs (cap to satisfy the budget); for each, emit `ContextFragment{fragment_id: format!("frag-{i}"), kind: ContextFragmentKind::WorkspaceArtifact, content_ref: ref.clone(), content: None, provenance: "build_context".into(), ordinal: i as u32}`; `package_id = format!("context-{}", next_id())`; build `ContextPackage{package_id, fragments, budget}`; `validate_context_package(&package)?`; return it. `query` is **ignored** (no RAG backend in greentic-dw — knowledge lives in greentic-runner; documented).
- `compress_context`: `validate_context_package(&req.package)?`; `text = complete_text(system_for_compress(), user_for_compress(&render_package(&req.package)))?`; `id = format!("{}::compressed::{}", req.package.package_id, next_id())`; `write_artifact(&id, ArtifactKind::PromptFragment, "compressed context", text)?`; return `CompressedContext{source_package_id: req.package.package_id.clone(), compressed_artifact_ref: id, fragment_count: req.package.fragments.len() as u32}`.
- `summarize_context`: `validate_context_package(&req.package)?`; `text = complete_text(system_for_summarize(), user_for_summarize(&render_package(&req.package)))?`; `id = format!("{}::summary::{}", req.package.package_id, next_id())`; `write_artifact(&id, ArtifactKind::ReportSection, "context summary", text)?`; return `SummaryArtifactRef{artifact_ref: id}`.

`next_id()` = `self.counter.fetch_add(1, Ordering::Relaxed)`.

## 3. Error handling

Invalid input package → `Validation` (from `validate_context_package`). LLM failure → `Provider`. Workspace write failure → `Provider`. No panics; no `unwrap`/`expect` in non-test code (the `bridge` transient-runtime build is the documented exception).

## 4. Testing (stub LLM + real in-memory workspace)

Use a `StubLlm` (copied from the reasoning crates — canned `ChatResponse.content`) and the **real** `greentic_dw_workspace_mem::InMemoryWorkspaceProvider` as the workspace (dev-dependency) for an integration-grade test.

- `build_context`: 3 refs, budget max_fragments 8 → package with 3 fragments, ordinals 0..2, non-empty package_id, `validate_context_package` passes; refs.len()=5 with max_fragments=2 → exactly 2 fragments (capped).
- `build_context`: zero `max_fragments` → `Validation` (budget invalid).
- `compress_context`: stub returns `"COMPRESSED"`; result `compressed_artifact_ref` contains `::compressed::`; `fragment_count` == package fragment count; reading that ref back from the workspace yields body `"COMPRESSED"`.
- `summarize_context`: stub returns `"SUMMARY"`; `artifact_ref` contains `::summary::`; workspace read yields body `"SUMMARY"`.
- `compress_context` with an invalid package (empty package_id) → `Validation` before any LLM/workspace call.
- LLM error path: a failing stub → `compress_context` returns `Provider`.
- `bridge` smoke tests (copied).

## 5. Limitations

- `build_context` is ref-only assembly; semantic `query`/RAG is intentionally out of scope (knowledge backend lives in greentic-runner — see the AW-memory/RAG locality note). `content` fields are left `None` (refs, not inline text).
- `compress`/`summarize` prompts are first-draft (stub-tested); the model output is stored verbatim with no structural validation (free text by design).
- One `LlmContextProvider` instance is bound to one `WorkspaceScope` (the deep-loop constructs providers per run) — matches how the coordinator wires providers.
- Completes all five providers. Remaining for a working deep-worker: the production `OperalaDispatchInvoker` wiring planner+reflector+delegator+context+workspace into `DeepLoopCoordinator` (spawn_blocking), runner in-proc operala serve spawn, designer deep-worker authoring, live-LLM prompt tuning.
