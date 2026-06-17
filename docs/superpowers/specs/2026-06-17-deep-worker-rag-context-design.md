# Deep-Worker RAG via the ContextProvider seam — design

Date: 2026-06-17
Status: Approved (design); implementation plan to follow.
Repo: `greentic-dw` (+ `greentic-dw-providers` host wiring)

## 1. Problem

The deep-worker (`greentic-dw-runtime`) is a plan-reflect-revise-delegate
orchestrator. It does **not** prompt an LLM directly for the end-user agent turn —
it delegates execution to the agentic-worker runtime (`greentic-aw-runtime`),
which already performs knowledge auto pre-retrieval (RAG-as-context) as of
greentic-runner PR #448.

Two gaps remain inside the deep-worker's **own** LLM-calling layers:

1. **Planning and reflection run on static prompts.** `deep_loop.rs` calls the
   `PlanningProvider` (`next_actions` / `revise_plan`) and `ReflectionProvider`
   (`review_step` / `review_final`) traits. Their concrete LLM implementations
   (`llm-outline`, `llm-critic`) build prompts that carry **no retrieved
   knowledge** — the planner decides next actions and the reflector critiques
   step output with zero grounding from the worker's knowledge corpus.
2. **The context package is built and discarded.** `deep_loop.rs` (~L162) calls
   `ContextProvider::build_context(BuildContextRequest { fragment_refs:
   vec![step.step_id], budget })` and binds the result to `_context_package` —
   underscore-prefixed, never consumed. `BuildContextRequest` has no `query`
   field, so the context layer cannot do semantic retrieval today.

This spec wires **full knowledge/RAG into deep-worker planning and reflection**
by making retrieval a function of the existing `ContextProvider` seam, and by
threading the (currently orphaned) context package into the planning/reflection
prompts.

## 2. Decisions

- **D1 — Retrieval-as-context, not a second seam.** Knowledge retrieval lives
  behind the existing `ContextProvider::build_context` seam (driven by a new
  `query` field), rather than a parallel `Knowledge` field on the deep-loop.
  This unifies context-building, reuses the context-package plumbing, and fixes
  the orphaned `_context_package` in the same change. (Chosen over mirroring the
  aw-runtime `Knowledge` seam, which would leave two retrieval paths and the
  orphan unsolved.)
- **D2 — Reuse the W3 `Knowledge` backend.** The knowledge-aware
  `ContextProvider` calls the same `greentic-dw-knowledge` / `KnowledgeChronicle`
  doc-RAG backend used by aw-runtime, tenant-scoped. No new retrieval substrate.
- **D3 — Planning/reflection receive a pre-rendered context *string*, not a
  `ContextPackage`.** The deep-loop renders the package to text and passes
  `context: Option<String>` into the planning/reflection request DTOs. This
  decouples those crates from `dw-context` types (no new cross-crate type
  dependency) and keeps the prompt builders trivial (append the block).
- **D4 — Fail-open.** Retrieval failure degrades to an empty context (the turn
  proceeds without grounding), matching the aw-runtime behaviour (#448).
- **D5 — Backward-compatible API.** `BuildContextRequest.query` and
  `ContextFragment.content` are additive `Option` fields with serde defaults, so
  existing `ContextProvider` impls and callers compile unchanged.

## 3. Architecture / data flow

```
deep_loop step
  └─ ContextProvider::build_context(BuildContextRequest {
         fragment_refs: [step.step_id],
         query: Some(<step goal/title>),   // NEW
         budget })
       └─ KnowledgeContextProvider: when query is Some,
            Knowledge::search(query, top_k) [W3 Chronicle doc-RAG]
              → Vec<RetrievedChunk> → ContextFragment { kind: KnowledgeChunk,
                                                         content: Some(text) }
  └─ ContextPackage { fragments }           // NO LONGER discarded
  └─ render_context(&package) -> String     // <knowledge> block from fragments
  └─ PlanningProvider::next_actions(NextActionsRequest { plan, context: Some(s) })
  └─ ReflectionProvider::review_step(ReviewStepRequest { …, context: Some(s) })
       └─ llm-outline / llm-critic prompt builders append the context block
       └─ LLM
```

## 4. Component changes

### 4.1 `greentic-dw-context`
- `BuildContextRequest` += `query: Option<String>` (`#[serde(default)]`). The
  retrieval query for semantic context; `None` preserves today's
  fragment-ref-only behaviour.
- `ContextFragment` += `content: Option<String>` (`#[serde(default)]`). Inline
  renderable text for retrieval results. Reference-only fragments (memory /
  artifact / plan-step) leave it `None`.
- `ContextFragmentKind` += `KnowledgeChunk` variant.
- New helper `render_context(&ContextPackage) -> String`: emits a delimited
  `<knowledge>` block listing fragments that carry inline `content`, respecting
  ordinal order; returns an empty string when none. (Mirrors aw-runtime's
  `knowledge::augment_system_prompt` shape for cross-runtime consistency.)

### 4.2 `greentic-dw-providers` — knowledge-aware `ContextProvider`
- New impl (new crate `context/knowledge`, or an extension of the existing
  `context/retrieval` impl) implementing `ContextProvider`.
- On `build_context`: when `req.query` is `Some`, resolve the tenant, call
  `Knowledge::search(KnowledgeQuery { query, limit: top_k })` against the
  W3 `KnowledgeChronicle` backend, and map each `RetrievedChunk` to a
  `ContextFragment { kind: KnowledgeChunk, content: Some(chunk.text),
  provenance: "knowledge", ordinal }`. Existing fragment-ref handling is
  preserved (composed with whatever the base impl already does).
- `top_k` sourced from provider config (default mirrors the knowledge tier, e.g.
  5); retrieval errors map to an empty fragment set (D4), logged.

### 4.3 `greentic-dw-runtime/deep_loop.rs`
- Build context with `query: Some(<step goal/title>)` (the step's
  human-readable goal is the retrieval query).
- Stop discarding: `let ctx = self.context.build_context(...)?;` then
  `let context_text = greentic_dw_context::render_context(&ctx);` (→
  `Option<String>` when non-empty).
- Thread `context: context_text.clone()` into `NextActionsRequest` /
  `RevisePlanRequest` (planning) and `ReviewStepRequest` / `ReviewFinalRequest`
  (reflection).

### 4.4 `greentic-dw-planning` + `greentic-dw-reflection`
- Request DTOs (`NextActionsRequest`, `RevisePlanRequest`, `ReviewStepRequest`,
  `ReviewFinalRequest`) += `context: Option<String>` (`#[serde(default)]`).
- `llm-outline` (`build_outline_prompt`) and `llm-critic` (`build_prompt`):
  append the context string verbatim when present (it is already a rendered
  `<knowledge>` block). No type coupling to `dw-context`.

### 4.5 Host wiring (dw-runtime host edge)
- Construct the `KnowledgeContextProvider` with the W3 `KnowledgeChronicle`
  backend (embedding secret-ref + graph driver), config-gated, mirroring how
  aw-runtime mounts the knowledge provider in W4 4d. When knowledge is not
  configured, the host wires the existing (non-retrieving) `ContextProvider` and
  behaviour is unchanged.

## 5. Error handling
- Retrieval failure (backend down, dim mismatch, invalid tenant) → empty context,
  warn-logged, turn proceeds (D4). Never propagate as a deep-loop failure.
- Tenant conversion failure → empty context + warn (same).
- `render_context` on a package with no inline-content fragments → empty string →
  `context: None` threaded (prompt builders append nothing).

## 6. Testing
- `dw-context`: unit tests for `render_context` (block shape with/without
  content fragments; ordinal ordering) and serde round-trip of the new optional
  fields (legacy JSON without them decodes).
- `KnowledgeContextProvider`: with a stub `Knowledge`, assert query→fragments
  mapping, `top_k` bound, fail-open on backend error, and that `query: None`
  produces no knowledge fragments.
- `deep_loop`: with a stub knowledge-aware `ContextProvider` + stub
  planning/reflection providers that capture their request, assert the rendered
  `<knowledge>` block reaches both the planning and reflection requests (the
  deep-worker analogue of aw-runtime's `knowledge_chunks_inject_into_system_prompt`).
- Backward-compat: existing `ContextProvider` impls and deep-loop tests compile
  and pass with the additive fields defaulted.

## 7. Out of scope (this spec)
- **Delegated-subtask context materialization** (`SubtaskEnvelope.context_package_ref`
  is built but nothing implements the retrieval side) — a separate concern; the
  delegated agent turn already gets RAG via aw-runtime #448.
- **Lightweight W3 trait crate extraction.** aw-runtime had to define a *local*
  `Knowledge` seam (W4 #448) because `greentic-dw-knowledge` core re-exports its
  DTOs from the heavy `greentic-dw-providers-common`, dragging a conflicting
  `greentic-types`. The deep-worker's knowledge `ContextProvider` lives in
  `greentic-dw-providers` (host edge), so it can depend on the W3 crate directly
  and is not blocked by this — but extracting a trait-only crate remains a
  worthwhile follow-up so runtimes can re-export instead of mirror.
- Tool-based retrieval (a planner/reflector tool to pull knowledge on demand);
  this spec is auto pre-retrieval only (consistent with epic D3).

## 8. Open questions
- **Retrieval query granularity.** Use the step goal/title as the query
  (proposed). Alternative: the overall plan goal for `next_actions`, the step
  subject for `review_step`. Decide at plan time; start with step goal.
- **`top_k` per layer.** Single `top_k` for both planning and reflection
  (proposed) vs. separate caps. Start with one.
- **Budget interaction.** How retrieved knowledge fragments count against the
  existing `ContextBudget { max_fragments, max_bytes }`. Proposed: knowledge
  fragments share the budget; if exceeded, truncate lowest-ranked first.
