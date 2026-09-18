# Deep-Worker RAG via ContextProvider — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the deep-worker's own planning and reflection LLM calls knowledge/RAG grounding by making semantic retrieval a function of the existing `ContextProvider` seam and threading the (currently discarded) context package into planning/reflection prompts.

**Architecture:** Add a `query` field to `BuildContextRequest`; a knowledge-aware `ContextProvider` impl calls the W3 `Knowledge` (Chronicle doc-RAG) backend and returns chunks as inline-content `ContextFragment`s; `deep_loop` renders the package to a string and passes it into planning/reflection request DTOs whose prompt builders append it. Fail-open, backward-compatible (additive `Option` fields).

**Tech Stack:** Rust 1.95, `greentic-dw-{context,runtime,planning,reflection}`, `greentic-dw-providers`, `greentic-dw-knowledge` (W3), `async-trait`, `serde`, `thiserror`. Reference implementation for shapes: aw-runtime knowledge seam (greentic-runner PR #448, `crates/greentic-aw-runtime/src/knowledge.rs`).

**Spec:** `docs/superpowers/specs/2026-06-17-deep-worker-rag-context-design.md`

---

## Pre-work (Task 0): Anchor exact current signatures

Before writing code, open and confirm the exact current shapes (the tasks below
quote them; verify against the real files and adjust literals if they have
drifted on `research`):

- `greentic-dw-context/src/model.rs` — `BuildContextRequest`, `ContextFragment`,
  `ContextFragmentKind`, `ContextPackage`, `ContextBudget`.
- `greentic-dw-context/src/traits.rs` — `ContextProvider`.
- `greentic-dw-runtime/src/deep_loop.rs` — the `build_context` call site (~L162)
  and the planning/reflection call sites (`next_actions` ~L93, `review_step`
  ~L218).
- `greentic-dw-planning/src/{traits.rs,model.rs}` and
  `greentic-dw-providers/planning/llm-outline/src/{provider.rs,prompt.rs}`.
- `greentic-dw-reflection/src/{traits.rs,model.rs}` and
  `greentic-dw-providers/reflection/llm-critic/src/lib.rs`.
- `greentic-dw-knowledge` (W3) — `Knowledge` trait, `KnowledgeQuery`,
  `RetrievedChunk` (already on `greenticai/greentic-dw-providers` research via #73).

This task produces no commit; it just confirms the literals used below.

---

## Task 1: `dw-context` — query field, inline content, render helper

**Files:**
- Modify: `crates/greentic-dw-context/src/model.rs`
- Test: same file (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Write failing tests for the new fields + render helper**

Add to `crates/greentic-dw-context/src/model.rs` tests:

```rust
#[cfg(test)]
mod rag_tests {
    use super::*;

    fn knowledge_fragment(text: &str, ordinal: u32) -> ContextFragment {
        ContextFragment {
            fragment_id: format!("k{ordinal}"),
            kind: ContextFragmentKind::KnowledgeChunk,
            content_ref: String::new(),
            content: Some(text.to_string()),
            provenance: "knowledge".into(),
            ordinal,
        }
    }

    #[test]
    fn build_context_request_query_defaults_none() {
        let req: BuildContextRequest =
            serde_json::from_str(r#"{"fragment_refs":[],"budget":{"max_fragments":8,"max_bytes":16384}}"#)
                .unwrap();
        assert!(req.query.is_none());
    }

    #[test]
    fn render_context_emits_knowledge_block_in_ordinal_order() {
        let pkg = ContextPackage {
            package_id: "p1".into(),
            fragments: vec![knowledge_fragment("second", 1), knowledge_fragment("first", 0)],
            budget: ContextBudget { max_fragments: 8, max_bytes: 16384 },
        };
        let out = render_context(&pkg);
        assert!(out.contains("<knowledge>"));
        assert!(out.contains("</knowledge>"));
        let first = out.find("first").unwrap();
        let second = out.find("second").unwrap();
        assert!(first < second, "fragments must render in ordinal order");
    }

    #[test]
    fn render_context_empty_when_no_inline_content() {
        let pkg = ContextPackage {
            package_id: "p1".into(),
            fragments: vec![ContextFragment {
                fragment_id: "a".into(),
                kind: ContextFragmentKind::WorkspaceArtifact,
                content_ref: "artifact://x".into(),
                content: None,
                provenance: "ws".into(),
                ordinal: 0,
            }],
            budget: ContextBudget { max_fragments: 8, max_bytes: 16384 },
        };
        assert_eq!(render_context(&pkg), "");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p greentic-dw-context rag_tests`
Expected: FAIL (compile error — `query`, `content`, `KnowledgeChunk`, `render_context` missing).

- [ ] **Step 3: Add the fields, variant, and helper**

In `BuildContextRequest`:
```rust
    /// Semantic retrieval query. When `Some`, a knowledge-aware
    /// `ContextProvider` performs RAG; `None` preserves fragment-ref-only
    /// behaviour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
```

In `ContextFragment`:
```rust
    /// Inline renderable text for retrieval results (e.g. knowledge chunks).
    /// Reference-only fragments leave this `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
```

In `ContextFragmentKind`:
```rust
    /// A passage retrieved from the worker's knowledge corpus (RAG).
    KnowledgeChunk,
```

Add the free function (same module):
```rust
/// Render the inline-content fragments of a context package into a delimited
/// `<knowledge>` block, in ascending `ordinal` order. Returns an empty string
/// when no fragment carries inline content. Mirrors the aw-runtime
/// `knowledge::augment_system_prompt` shape for cross-runtime consistency.
pub fn render_context(package: &ContextPackage) -> String {
    let mut frags: Vec<&ContextFragment> =
        package.fragments.iter().filter(|f| f.content.is_some()).collect();
    if frags.is_empty() {
        return String::new();
    }
    frags.sort_by_key(|f| f.ordinal);
    let mut out = String::from(
        "<knowledge>\nRelevant passages retrieved from the worker's knowledge base:\n",
    );
    for f in frags {
        if let Some(text) = &f.content {
            out.push_str("- ");
            out.push_str(text.trim());
            out.push('\n');
        }
    }
    out.push_str("</knowledge>");
    out
}
```

Update every existing `ContextFragment { .. }` literal in the crate (and its
tests) to add `content: None` (additive field; the compiler lists each site).

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p greentic-dw-context && cargo clippy -p greentic-dw-context --all-targets -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-dw-context/src/model.rs
git commit -m "feat(context): query field + inline fragment content + render_context (deep-worker RAG)"
```

---

## Task 2: Knowledge-aware `ContextProvider` in `greentic-dw-providers`

**Files:**
- Create: `context/knowledge/Cargo.toml`, `context/knowledge/src/lib.rs` (new crate
  `greentic-dw-context-knowledge`), OR extend the existing `context/retrieval`
  impl if the repo prefers one crate per category — follow the established
  `greentic-dw-providers` layout confirmed in Task 0.
- Test: `context/knowledge/src/lib.rs` (`#[cfg(test)]`)

- [ ] **Step 1: Write the failing test (stub `Knowledge`, assert mapping + fail-open)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use greentic_dw_knowledge::{Knowledge, KnowledgeQuery, RetrievedChunk, IngestOutcome,
        KnowledgeChunk, KnowledgeResult, KnowledgeError};
    use greentic_types::TenantCtx;
    use std::sync::Arc;

    struct StubKnowledge { hits: Vec<RetrievedChunk>, fail: bool }
    #[async_trait::async_trait]
    impl Knowledge for StubKnowledge {
        async fn ingest(&self, _t: &TenantCtx, _c: Vec<KnowledgeChunk>) -> KnowledgeResult<IngestOutcome> {
            Ok(IngestOutcome::default())
        }
        async fn search(&self, _t: &TenantCtx, q: KnowledgeQuery) -> KnowledgeResult<Vec<RetrievedChunk>> {
            if self.fail { return Err(KnowledgeError::Backend("boom".into())); }
            let n = q.limit.unwrap_or(usize::MAX);
            Ok(self.hits.iter().take(n).cloned().collect())
        }
    }

    fn hit(text: &str) -> RetrievedChunk {
        RetrievedChunk { text: text.into(), score: 1.0, doc_id: None, chunk_index: None,
            metadata: serde_json::Map::new() }
    }

    #[tokio::test]
    async fn query_maps_to_knowledge_fragments_bounded_by_top_k() {
        let kb = Arc::new(StubKnowledge { hits: vec![hit("a"), hit("b"), hit("c")], fail: false });
        let p = KnowledgeContextProvider::new(kb, /*top_k*/ 2, tenant());
        let pkg = p.build_context(BuildContextRequest {
            fragment_refs: vec![], query: Some("refunds".into()),
            budget: ContextBudget { max_fragments: 8, max_bytes: 16384 },
        }).unwrap();
        let texts: Vec<_> = pkg.fragments.iter().filter_map(|f| f.content.clone()).collect();
        assert_eq!(texts, vec!["a".to_string(), "b".to_string()]);
        assert!(pkg.fragments.iter().all(|f| matches!(f.kind, ContextFragmentKind::KnowledgeChunk)));
    }

    #[tokio::test]
    async fn no_query_yields_no_knowledge_fragments() {
        let kb = Arc::new(StubKnowledge { hits: vec![hit("a")], fail: false });
        let p = KnowledgeContextProvider::new(kb, 5, tenant());
        let pkg = p.build_context(BuildContextRequest {
            fragment_refs: vec![], query: None,
            budget: ContextBudget { max_fragments: 8, max_bytes: 16384 },
        }).unwrap();
        assert!(pkg.fragments.is_empty());
    }

    #[tokio::test]
    async fn retrieval_failure_is_fail_open() {
        let kb = Arc::new(StubKnowledge { hits: vec![], fail: true });
        let p = KnowledgeContextProvider::new(kb, 5, tenant());
        let pkg = p.build_context(BuildContextRequest {
            fragment_refs: vec![], query: Some("x".into()),
            budget: ContextBudget { max_fragments: 8, max_bytes: 16384 },
        }).unwrap();
        assert!(pkg.fragments.is_empty());
    }
}
```
(`tenant()` helper builds a valid `TenantCtx`; copy the pattern from the W3
`knowledge/chronicle` tests confirmed in Task 0.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p greentic-dw-context-knowledge`
Expected: FAIL (compile error — `KnowledgeContextProvider` missing).

- [ ] **Step 3: Implement `KnowledgeContextProvider`**

```rust
use std::sync::Arc;
use greentic_dw_context::{BuildContextRequest, ContextError, ContextFragment,
    ContextFragmentKind, ContextPackage, ContextProvider};
use greentic_dw_knowledge::{Knowledge, KnowledgeQuery};
use greentic_types::TenantCtx;

/// A `ContextProvider` that performs knowledge/RAG retrieval when the request
/// carries a `query`. Backed by the W3 `Knowledge` (Chronicle doc-RAG) seam.
pub struct KnowledgeContextProvider {
    knowledge: Arc<dyn Knowledge>,
    top_k: usize,
    tenant: TenantCtx,
}

impl KnowledgeContextProvider {
    pub fn new(knowledge: Arc<dyn Knowledge>, top_k: usize, tenant: TenantCtx) -> Self {
        Self { knowledge, top_k: top_k.max(1), tenant }
    }
}

impl ContextProvider for KnowledgeContextProvider {
    fn build_context(&self, req: BuildContextRequest) -> Result<ContextPackage, ContextError> {
        let mut fragments = Vec::new();
        if let Some(query) = req.query.clone() {
            // Block on the async search; deep-loop already runs on a runtime.
            // (If ContextProvider is async in this repo, await directly instead.)
            let hits = futures::executor::block_on(self.knowledge.search(
                &self.tenant,
                KnowledgeQuery { query, limit: Some(self.top_k) },
            ))
            .unwrap_or_default(); // fail-open (D4)
            for (i, h) in hits.into_iter().enumerate() {
                fragments.push(ContextFragment {
                    fragment_id: format!("knowledge-{i}"),
                    kind: ContextFragmentKind::KnowledgeChunk,
                    content_ref: String::new(),
                    content: Some(h.text),
                    provenance: "knowledge".into(),
                    ordinal: i as u32,
                });
            }
        }
        Ok(ContextPackage {
            package_id: format!("ctx-{}", req.fragment_refs.first().cloned().unwrap_or_default()),
            fragments,
            budget: req.budget,
        })
    }
    // compress_context / summarize_context: delegate to defaults or the base impl
    // per the trait shape confirmed in Task 0.
}
```
NOTE (Task 0 decision): confirm whether `ContextProvider` is sync or async in
this repo. The deep-worker mapping shows the trait methods are **sync**
(`fn build_context(...) -> Result<...>`), so the async `Knowledge::search` is
bridged via `futures::executor::block_on` (add `futures` dep) or
`tokio::runtime::Handle::current().block_on(...)`. If the trait is async,
`await` directly and drop the bridge.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p greentic-dw-context-knowledge && cargo clippy -p greentic-dw-context-knowledge --all-targets -- -D warnings && cargo fmt --check`
Expected: PASS, no warnings.

- [ ] **Step 5: Commit**

```bash
git add context/knowledge
git commit -m "feat(context): knowledge-aware ContextProvider over W3 Knowledge backend"
```

---

## Task 3: Planning + reflection DTOs and prompt builders

**Files:**
- Modify: `crates/greentic-dw-planning/src/model.rs` (request DTOs),
  `crates/greentic-dw-reflection/src/model.rs`
- Modify: `greentic-dw-providers/planning/llm-outline/src/prompt.rs`,
  `greentic-dw-providers/reflection/llm-critic/src/lib.rs`
- Test: each modified crate's test module

- [ ] **Step 1: Write failing tests asserting context appears in built prompts**

For llm-outline (`prompt.rs` tests):
```rust
#[test]
fn outline_prompt_appends_context_when_present() {
    let req = PlanRequest { /* …minimal valid… */ ..Default::default() };
    let ctx = "<knowledge>\n- Refunds within 5 days.\n</knowledge>";
    let prompt = build_outline_prompt(&config(), &req, Some(ctx));
    assert!(prompt.contains("Refunds within 5 days."));
}

#[test]
fn outline_prompt_unchanged_without_context() {
    let req = PlanRequest { ..Default::default() };
    let with = build_outline_prompt(&config(), &req, None);
    assert!(!with.contains("<knowledge>"));
}
```
Mirror an equivalent pair for llm-critic `build_prompt`.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p greentic-dw-planning-llm-outline -p greentic-dw-reflection-llm-critic`
Expected: FAIL (signature mismatch — builders don't take a context arg yet).

- [ ] **Step 3: Add `context` to DTOs + builders**

In each request DTO (`NextActionsRequest`, `RevisePlanRequest`,
`ReviewStepRequest`, `ReviewFinalRequest`):
```rust
    /// Pre-rendered knowledge/context block to ground this call (deep-worker RAG).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
```

In `build_outline_prompt` (and `build_prompt` for the critic), change the
signature to accept `context: Option<&str>` and append before returning:
```rust
    if let Some(ctx) = context {
        if !ctx.is_empty() {
            out.push_str("\n\n");
            out.push_str(ctx);
        }
    }
```
Update the provider call sites (`provider.rs:34`, `lib.rs:49`) to pass the
request's `context.as_deref()`.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p greentic-dw-planning -p greentic-dw-reflection -p greentic-dw-planning-llm-outline -p greentic-dw-reflection-llm-critic && cargo clippy --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-dw-planning crates/greentic-dw-reflection planning/llm-outline reflection/llm-critic
git commit -m "feat(planning,reflection): optional pre-rendered context in request DTOs + prompts"
```

---

## Task 4: `deep_loop` — retrieve, render, thread (stop discarding)

**Files:**
- Modify: `crates/greentic-dw-runtime/src/deep_loop.rs`
- Test: `crates/greentic-dw-runtime/src/deep_loop.rs` tests (or a tests/ file),
  with stub knowledge-aware `ContextProvider` + capturing planning/reflection stubs.

- [ ] **Step 1: Write the failing integration-style test**

Build a `DeepLoopCoordinator` with: a stub `ContextProvider` returning one
`KnowledgeChunk` fragment with content `"Refunds within 5 days."`; capturing stub
`PlanningProvider`/`ReflectionProvider` that record the `context` field of the
requests they receive. Run one step; assert both captured requests' `context`
contains `"Refunds within 5 days."`. (This is the deep-worker analogue of
aw-runtime's `knowledge_chunks_inject_into_system_prompt`.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p greentic-dw-runtime deep_loop`
Expected: FAIL (context is `None` — deep_loop still discards the package).

- [ ] **Step 3: Wire it in `deep_loop.rs`**

Replace the discarded call (~L162):
```rust
    let context_pkg = self.context.build_context(BuildContextRequest {
        fragment_refs: vec![step.step_id.clone()],
        query: Some(step.goal_or_title()),   // confirm the step's goal accessor in Task 0
        budget: ContextBudget { max_fragments: 8, max_bytes: 16_384 },
    })?;
    let context_text = {
        let rendered = greentic_dw_context::render_context(&context_pkg);
        (!rendered.is_empty()).then_some(rendered)
    };
```
Thread `context: context_text.clone()` into the `next_actions` /`revise_plan`
requests (~L93) and the `review_step` / `review_final` requests (~L218).

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p greentic-dw-runtime && cargo clippy -p greentic-dw-runtime --all-targets -- -D warnings`
Expected: PASS — the `_context_package` is gone and grounding reaches both layers.

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-dw-runtime/src/deep_loop.rs
git commit -m "feat(deep-loop): retrieve+render context and thread into planning+reflection"
```

---

## Task 5: Host wiring (config-gated knowledge ContextProvider mount)

**Files:**
- Modify: the dw-runtime host construction site (confirm in Task 0 — where the
  `ContextProvider` is built for `DeepLoopCoordinator`).
- Test: host-level test asserting the knowledge provider is mounted when config
  enables it and the plain provider otherwise.

- [ ] **Step 1: Write the failing test** — with knowledge config present, the
  built coordinator's context provider returns knowledge fragments for a query;
  absent, it returns none.

- [ ] **Step 2: Run to verify failure** — `cargo test` on the host crate; FAIL.

- [ ] **Step 3: Implement the gated mount** — when knowledge config is present,
  construct `KnowledgeContextProvider::new(KnowledgeChronicle::from_config(...),
  top_k, tenant)` (reuse the W3 `from_config` + `DwEmbedderBridge` path, mirroring
  aw-runtime W4 4d); otherwise wire the existing non-retrieving provider.

- [ ] **Step 4: Run to verify pass** — `cargo test` + `cargo clippy --all-targets -- -D warnings`.

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(dw-host): mount knowledge ContextProvider when configured"
```

---

## Final verification

- [ ] `bash ci/local_check.sh` (or the repo's canonical gate) green across the
  touched crates.
- [ ] Manual trace: a deep-worker step with a knowledge corpus produces planner
  and reflector prompts containing the `<knowledge>` block (add a `tracing`
  debug span if helpful).

## Self-review notes (against the spec)
- §4.1 dw-context (query + content + KnowledgeChunk + render_context) → Task 1. ✓
- §4.2 knowledge ContextProvider → Task 2. ✓
- §4.3 deep_loop thread → Task 4. ✓
- §4.4 planning/reflection DTOs + builders → Task 3. ✓
- §4.5 host wiring → Task 5. ✓
- §5 error handling (fail-open) → Task 2 Step 1 (`retrieval_failure_is_fail_open`) + Task 4. ✓
- §6 testing → tests in Tasks 1–5. ✓
- Open questions (query granularity, top_k, budget) resolved at Task 0 with the
  spec's proposed defaults (step goal, single top_k, shared budget truncate-lowest).
