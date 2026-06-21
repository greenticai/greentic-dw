# LLM-backed ContextProvider — Implementation Plan (deep-worker brain, slice 4b)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** New `greentic-dw-context-llm` crate: `LlmContextProvider` implementing `greentic_dw_context::ContextProvider` — `build_context` deterministic; `compress_context`/`summarize_context` LLM-backed, writing the result text as an artifact to a `WorkspaceProvider`.

**Architecture:** Wraps `Arc<dyn LlmProvider>` + `Arc<dyn WorkspaceProvider>` + a `WorkspaceScope` + an `AtomicU64` id counter. LLM calls go through `bridge::block_on`; the reply text is stored verbatim (no JSON parse).

**Tech Stack:** Rust edition 2024, `greentic-dw-context` + `greentic-dw-workspace` (workspace/path deps), `greentic-llm` git tag `v1.2.6-research`, tokio (bridge). Dev: `greentic-dw-workspace-mem`.

## Global Constraints

- Edition 2024. **No `.unwrap()`/`.expect()` in non-test code** (the `bridge` transient-runtime build is the one documented exception).
- `greentic-llm = { git = "https://github.com/greenticai/greentic-llm", tag = "v1.2.6-research" }` (NO local path/patch).
- Invalid input package → `ContextError::Validation` (via `validate_context_package`); LLM/workspace failure → `ContextError::Provider`.
- Conventional commits; **NO Claude/AI co-author or attribution**.
- Worktree `.worktrees/context-llm` (greentic-dw), branch `feat/llm-context-provider`. greentic-dw pushes via **SSH**.
- ALWAYS prefix cargo with `CARGO_NET_GIT_FETCH_WITH_CLI=true`. Scoped: `cargo build/test/clippy -p greentic-dw-context-llm`. On "No space left on device": STOP, report BLOCKED.

## Reference (read first)

- Contract + DTOs + validator: `crates/greentic-dw-context/src/{traits.rs, model.rs, error.rs, validate.rs}`.
- Mirror template (bridge, StubLlm, Cargo.toml, LLM call shape): `crates/greentic-dw-reflection-llm/src/{bridge.rs, lib.rs, prompt.rs}` + its `Cargo.toml`.
- Workspace DTOs + the test store: `crates/greentic-dw-workspace/src/{traits.rs, model.rs}` and `crates/greentic-dw-workspace-mem/src/lib.rs` (`InMemoryWorkspaceProvider`).

---

## Task 1: `greentic-dw-context-llm` crate

**Files:**
- Create: `crates/greentic-dw-context-llm/{Cargo.toml, src/lib.rs, src/bridge.rs, src/prompt.rs}`
- Modify: root `Cargo.toml` (`[workspace] members`)

**Interfaces:**
- Produces: `LlmContextProvider::new(llm: Arc<dyn greentic_llm::LlmProvider>, workspace: Arc<dyn greentic_dw_workspace::WorkspaceProvider>, scope: greentic_dw_workspace::WorkspaceScope) -> Self` implementing `greentic_dw_context::ContextProvider`.

- [ ] **Step 1: Read the references**

Read `crates/greentic-dw-context/src/{traits.rs, model.rs, error.rs, validate.rs}` (the 3 methods, all DTO fields, `validate_context_package`). Read `crates/greentic-dw-reflection-llm/src/{bridge.rs, lib.rs}` for the `block_on` bridge, the `StubLlm` test double, and the `ChatRequest`/`llm.chat` call shape. Read `crates/greentic-dw-workspace/src/model.rs` (CreateArtifactRequest/ArtifactRef/ArtifactKind/ArtifactMetadata/WorkspaceScope) and `crates/greentic-dw-workspace-mem/src/lib.rs` (`InMemoryWorkspaceProvider::with_clock`, `new`).

- [ ] **Step 2: Cargo.toml + workspace member**

Create `crates/greentic-dw-context-llm/Cargo.toml`:
```toml
[package]
name = "greentic-dw-context-llm"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
description = "LLM-backed ContextProvider for the Greentic deep-worker (greentic-llm)."
publish = false

[dependencies]
greentic-dw-context = { workspace = true }
greentic-dw-workspace = { workspace = true }
greentic-llm = { git = "https://github.com/greenticai/greentic-llm", tag = "v1.2.6-research" }
async-trait = "0.1"
futures-util = "0.3"
serde = { workspace = true }
serde_json = { workspace = true }
schemars = { workspace = true }
tokio = { version = "1", features = ["rt", "rt-multi-thread"] }

[dev-dependencies]
greentic-dw-workspace-mem = { path = "../greentic-dw-workspace-mem" }
tokio = { version = "1", features = ["rt", "rt-multi-thread", "macros"] }
```
If `greentic-dw-context` / `greentic-dw-workspace` are not in root `[workspace.dependencies]`, use `{ path = "../greentic-dw-context" }` / `{ path = "../greentic-dw-workspace" }` instead (check how `greentic-dw-reflection-llm` declares its contract dep, and mirror that form). Add `"crates/greentic-dw-context-llm",` to root `Cargo.toml` `[workspace] members`.

- [ ] **Step 3: bridge.rs**

Copy `crates/greentic-dw-reflection-llm/src/bridge.rs` **verbatim** (including its two `#[cfg(test)]` tests). No edits needed.

- [ ] **Step 4: prompt.rs**

Create `crates/greentic-dw-context-llm/src/prompt.rs`:
```rust
//! Prompt builders for the LLM-backed context methods.
//!
//! `render_package` flattens a `ContextPackage` into readable text; the
//! `system_for_*` / `user_for_*` builders instruct the model to return PLAIN
//! TEXT (no JSON, no fences) — the reply is stored verbatim as an artifact.

use greentic_dw_context::ContextPackage;

/// Flatten a context package into a stable, human-readable block: one line per
/// fragment in `ordinal` order, including inline `content` when present.
pub fn render_package(package: &ContextPackage) -> String {
    let mut fragments: Vec<&greentic_dw_context::ContextFragment> = package.fragments.iter().collect();
    fragments.sort_by_key(|f| f.ordinal);
    let mut out = format!("Context package {} ({} fragments):\n", package.package_id, package.fragments.len());
    for fragment in fragments {
        out.push_str(&format!("[{}] {:?} {}", fragment.ordinal, fragment.kind, fragment.content_ref));
        if let Some(text) = &fragment.content {
            out.push_str(" :: ");
            out.push_str(text.trim());
        }
        out.push('\n');
    }
    out
}

/// System prompt for `compress_context`.
pub fn system_for_compress() -> String {
    "You are a context-compression assistant in a deep-worker system. Condense the supplied \
context into the smallest faithful form that preserves every fact needed for downstream \
reasoning. Respond with ONLY the compressed text — no preamble, no JSON, no markdown fences."
        .to_string()
}

/// System prompt for `summarize_context`.
pub fn system_for_summarize() -> String {
    "You are a context-summarization assistant in a deep-worker system. Produce a concise \
prose summary of the supplied context. Respond with ONLY the summary text — no preamble, no \
JSON, no markdown fences."
        .to_string()
}

/// User prompt wrapping the rendered package for compression.
pub fn user_for_compress(rendered: &str) -> String {
    format!("Compress the following context:\n\n{rendered}")
}

/// User prompt wrapping the rendered package for summarization.
pub fn user_for_summarize(rendered: &str) -> String {
    format!("Summarize the following context:\n\n{rendered}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_dw_context::{ContextBudget, ContextFragment, ContextFragmentKind};

    fn pkg() -> ContextPackage {
        ContextPackage {
            package_id: "p1".into(),
            fragments: vec![
                ContextFragment { fragment_id: "f1".into(), kind: ContextFragmentKind::KnowledgeChunk, content_ref: "ref-b".into(), content: Some("second".into()), provenance: "x".into(), ordinal: 1 },
                ContextFragment { fragment_id: "f0".into(), kind: ContextFragmentKind::MemoryItem, content_ref: "ref-a".into(), content: None, provenance: "x".into(), ordinal: 0 },
            ],
            budget: ContextBudget { max_fragments: 8, max_bytes: 4096 },
        }
    }

    #[test]
    fn render_package_orders_by_ordinal_and_includes_inline_content() {
        let out = render_package(&pkg());
        let first = out.find("ref-a").unwrap();
        let second = out.find("ref-b").unwrap();
        assert!(first < second, "fragments must render in ordinal order");
        assert!(out.contains("second"), "inline content must be included");
        assert!(out.contains("p1"));
    }
}
```

- [ ] **Step 5: Write the failing tests (TDD — RED)**

Create `crates/greentic-dw-context-llm/src/lib.rs` with the module wiring and a `#[cfg(test)] mod tests`. Copy the `StubLlm` double from `crates/greentic-dw-reflection-llm/src/lib.rs` (the `async_trait` impl + `futures_util` stream), and add a `FailingLlm` whose `chat` returns `Err(LlmError...)`. Use the real `greentic_dw_workspace_mem::InMemoryWorkspaceProvider` as the workspace.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use greentic_dw_context::*;
    use greentic_dw_workspace::{ReadArtifactRequest, WorkspaceProvider, WorkspaceScope};
    use greentic_dw_workspace_mem::InMemoryWorkspaceProvider;
    // ... StubLlm (canned content) + FailingLlm (chat -> Err) copied/adapted from reflection-llm tests ...

    fn scope() -> WorkspaceScope {
        WorkspaceScope { tenant: "t".into(), team: None, session: "s".into(), agent: None, run: "r".into() }
    }
    fn budget(max_fragments: u32) -> ContextBudget { ContextBudget { max_fragments, max_bytes: 4096 } }

    fn provider_with(content: &str) -> (LlmContextProvider, std::sync::Arc<InMemoryWorkspaceProvider>) {
        let ws = std::sync::Arc::new(InMemoryWorkspaceProvider::new());
        let llm = std::sync::Arc::new(StubLlm::with_response(content));
        (LlmContextProvider::new(llm, ws.clone(), scope()), ws)
    }

    #[test]
    fn build_context_assembles_fragments_within_budget() {
        let (cx, _ws) = provider_with("");
        let pkg = cx.build_context(BuildContextRequest {
            fragment_refs: vec!["a".into(), "b".into(), "c".into()],
            query: None, budget: budget(8),
        }).unwrap();
        assert_eq!(pkg.fragments.len(), 3);
        assert_eq!(pkg.fragments[0].ordinal, 0);
        assert!(!pkg.package_id.is_empty());
    }

    #[test]
    fn build_context_caps_to_max_fragments() {
        let (cx, _ws) = provider_with("");
        let pkg = cx.build_context(BuildContextRequest {
            fragment_refs: vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
            query: None, budget: budget(2),
        }).unwrap();
        assert_eq!(pkg.fragments.len(), 2);
    }

    #[test]
    fn build_context_zero_budget_is_validation_error() {
        let (cx, _ws) = provider_with("");
        let err = cx.build_context(BuildContextRequest { fragment_refs: vec![], query: None, budget: budget(0) }).unwrap_err();
        assert!(matches!(err, ContextError::Validation(_)));
    }

    fn package_for_test() -> ContextPackage {
        ContextPackage {
            package_id: "pkg-1".into(),
            fragments: vec![ContextFragment {
                fragment_id: "f0".into(), kind: ContextFragmentKind::MemoryItem,
                content_ref: "ref-a".into(), content: Some("body".into()),
                provenance: "test".into(), ordinal: 0,
            }],
            budget: budget(8),
        }
    }

    #[test]
    fn compress_writes_artifact_and_returns_ref() {
        let (cx, ws) = provider_with("COMPRESSED");
        let out = cx.compress_context(CompressContextRequest { package: package_for_test() }).unwrap();
        assert!(out.compressed_artifact_ref.contains("::compressed::"));
        assert_eq!(out.source_package_id, "pkg-1");
        assert_eq!(out.fragment_count, 1);
        let stored = ws.read_artifact(ReadArtifactRequest { artifact_id: out.compressed_artifact_ref.clone() }).unwrap();
        assert_eq!(stored.body, "COMPRESSED");
    }

    #[test]
    fn summarize_writes_artifact_and_returns_ref() {
        let (cx, ws) = provider_with("SUMMARY");
        let out = cx.summarize_context(SummarizeContextRequest { package: package_for_test() }).unwrap();
        assert!(out.artifact_ref.contains("::summary::"));
        let stored = ws.read_artifact(ReadArtifactRequest { artifact_id: out.artifact_ref.clone() }).unwrap();
        assert_eq!(stored.body, "SUMMARY");
    }

    #[test]
    fn compress_invalid_package_is_validation_error() {
        let (cx, _ws) = provider_with("X");
        let mut bad = package_for_test();
        bad.package_id = "".into();
        let err = cx.compress_context(CompressContextRequest { package: bad }).unwrap_err();
        assert!(matches!(err, ContextError::Validation(_)));
    }

    #[test]
    fn compress_llm_failure_is_provider_error() {
        let ws = std::sync::Arc::new(InMemoryWorkspaceProvider::new());
        let cx = LlmContextProvider::new(std::sync::Arc::new(FailingLlm), ws, scope());
        let err = cx.compress_context(CompressContextRequest { package: package_for_test() }).unwrap_err();
        assert!(matches!(err, ContextError::Provider(_)));
    }
}
```

- [ ] **Step 6: Run tests — verify they FAIL**

Run: `CARGO_NET_GIT_FETCH_WITH_CLI=true cargo test -p greentic-dw-context-llm`
Expected: FAIL (type/impl not defined).

- [ ] **Step 7: Implement `LlmContextProvider` (GREEN)**

In `lib.rs`, above the tests:
```rust
//! LLM-backed [`ContextProvider`] for greentic-dw.
//!
//! `build_context` assembles a package from fragment refs deterministically.
//! `compress_context` / `summarize_context` call the LLM and store the reply
//! text as an artifact in a [`WorkspaceProvider`], returning the artifact ref.

pub mod bridge;
mod prompt;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use greentic_dw_context::{
    BuildContextRequest, CompressContextRequest, CompressedContext, ContextError, ContextFragment,
    ContextFragmentKind, ContextPackage, ContextProvider, SummarizeContextRequest,
    SummaryArtifactRef, validate_context_package,
};
use greentic_dw_workspace::{
    ArtifactKind, ArtifactMetadata, ArtifactRef, CreateArtifactRequest, WorkspaceProvider,
    WorkspaceScope,
};
use greentic_llm::{ChatMessage, ChatRequest, LlmProvider};

/// An LLM-backed [`ContextProvider`]. Compression/summarization results are
/// persisted to `workspace` under `scope`.
pub struct LlmContextProvider {
    llm: Arc<dyn LlmProvider>,
    workspace: Arc<dyn WorkspaceProvider>,
    scope: WorkspaceScope,
    counter: AtomicU64,
}

impl LlmContextProvider {
    /// Create a provider over the given LLM, workspace store, and run scope.
    pub fn new(
        llm: Arc<dyn LlmProvider>,
        workspace: Arc<dyn WorkspaceProvider>,
        scope: WorkspaceScope,
    ) -> Self {
        Self { llm, workspace, scope, counter: AtomicU64::new(0) }
    }

    fn next_id(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::Relaxed)
    }

    /// One-shot LLM call returning the reply text verbatim (no JSON parse).
    fn complete_text(&self, system: &str, user: String) -> Result<String, ContextError> {
        let request = ChatRequest {
            messages: vec![ChatMessage::system(system), ChatMessage::user(user)],
            tools: vec![],
            tool_choice: None,
            max_tokens: Some(2048),
            temperature: Some(0.2),
        };
        let response = bridge::block_on(self.llm.chat(request))
            .map_err(|llm_error| ContextError::Provider(llm_error.to_string()))?;
        Ok(response.content)
    }

    /// Persist `body` as a new artifact under the provider's scope.
    fn write_artifact(
        &self,
        artifact_id: &str,
        kind: ArtifactKind,
        title: &str,
        body: String,
    ) -> Result<(), ContextError> {
        let request = CreateArtifactRequest {
            artifact: ArtifactRef {
                artifact_id: artifact_id.to_string(),
                kind,
                scope: self.scope.clone(),
            },
            metadata: ArtifactMetadata {
                title: title.to_string(),
                tags: vec![],
                mime_type: Some("text/plain".to_string()),
            },
            body,
        };
        self.workspace
            .create_artifact(request)
            .map_err(|e| ContextError::Provider(format!("workspace write failed: {e}")))?;
        Ok(())
    }
}

impl ContextProvider for LlmContextProvider {
    fn build_context(&self, req: BuildContextRequest) -> Result<ContextPackage, ContextError> {
        let take = (req.budget.max_fragments as usize).min(req.fragment_refs.len());
        let fragments: Vec<ContextFragment> = req
            .fragment_refs
            .iter()
            .take(take)
            .enumerate()
            .map(|(i, content_ref)| ContextFragment {
                fragment_id: format!("frag-{i}"),
                kind: ContextFragmentKind::WorkspaceArtifact,
                content_ref: content_ref.clone(),
                content: None,
                provenance: "build_context".to_string(),
                ordinal: i as u32,
            })
            .collect();
        let package = ContextPackage {
            package_id: format!("context-{}", self.next_id()),
            fragments,
            budget: req.budget,
        };
        validate_context_package(&package)?;
        Ok(package)
    }

    fn compress_context(
        &self,
        req: CompressContextRequest,
    ) -> Result<CompressedContext, ContextError> {
        validate_context_package(&req.package)?;
        let rendered = prompt::render_package(&req.package);
        let text =
            self.complete_text(&prompt::system_for_compress(), prompt::user_for_compress(&rendered))?;
        let artifact_id = format!("{}::compressed::{}", req.package.package_id, self.next_id());
        self.write_artifact(&artifact_id, ArtifactKind::PromptFragment, "compressed context", text)?;
        Ok(CompressedContext {
            source_package_id: req.package.package_id.clone(),
            compressed_artifact_ref: artifact_id,
            fragment_count: req.package.fragments.len() as u32,
        })
    }

    fn summarize_context(
        &self,
        req: SummarizeContextRequest,
    ) -> Result<SummaryArtifactRef, ContextError> {
        validate_context_package(&req.package)?;
        let rendered = prompt::render_package(&req.package);
        let text = self
            .complete_text(&prompt::system_for_summarize(), prompt::user_for_summarize(&rendered))?;
        let artifact_id = format!("{}::summary::{}", req.package.package_id, self.next_id());
        self.write_artifact(&artifact_id, ArtifactKind::ReportSection, "context summary", text)?;
        Ok(SummaryArtifactRef { artifact_ref: artifact_id })
    }
}
```

For the test doubles, copy `StubLlm` from `crates/greentic-dw-reflection-llm/src/lib.rs` and add:
```rust
struct FailingLlm;
#[async_trait]
impl LlmProvider for FailingLlm {
    fn capabilities(&self) -> Capabilities { Capabilities { chat: true, tools: false, streaming: false, vision: false, system_prompt: true } }
    fn provider_name(&self) -> &'static str { "failing" }
    fn model(&self) -> &str { "failing-model" }
    async fn chat(&self, _req: ChatRequest) -> Result<ChatResponse, LlmError> { Err(LlmError::Provider("boom".to_string())) }
    async fn chat_stream(&self, _req: ChatRequest) -> Result<ChatStream, LlmError> { Err(LlmError::Provider("boom".to_string())) }
}
```
(Confirm the exact `LlmError` variant name from `greentic_llm` — if `LlmError::Provider` differs, use whatever single-field string variant exists, e.g. `LlmError::Other`/`LlmError::Api`. Pick any variant that constructs from a message; the test only asserts the mapped `ContextError::Provider`.)

- [ ] **Step 8: Run tests — verify they PASS**

Run: `CARGO_NET_GIT_FETCH_WITH_CLI=true cargo test -p greentic-dw-context-llm`
Expected: all PASS (build 3 + compress/summarize 2 + invalid 1 + llm-fail 1 + render 1 + bridge 2 ≈ 10).

- [ ] **Step 9: clippy + fmt**

Run: `CARGO_NET_GIT_FETCH_WITH_CLI=true cargo clippy -p greentic-dw-context-llm --all-targets -- -D warnings` (clean); `cargo fmt -p greentic-dw-context-llm`.

- [ ] **Step 10: Commit**

```bash
git add crates/greentic-dw-context-llm Cargo.toml Cargo.lock
git commit -m "feat(context-llm): LLM-backed ContextProvider with workspace-backed artifacts (deep-worker brain slice 4b)"
```

---

## Manual verification

`cargo test -p greentic-dw-context-llm` green; `LlmContextProvider::new(llm, workspace, scope)` usable as a `ContextProvider`; compress/summarize artifacts are readable back from the workspace.

## Self-Review (during planning)

- **Spec coverage:** §1 contract → all 3 methods in Task 1; §2 semantics → Steps 4/7 code; §4 testing → Step 5 (build within/over budget, zero budget, compress/summarize round-trip via real in-memory workspace, invalid package, LLM failure, render ordering). 
- **Placeholders:** none — full code in Steps 2/4/5/7. The only "confirm exact name" notes are the `LlmError` variant and the contract-dep declaration form (workspace vs path), both with explicit fallback instructions.
- **Type consistency:** DTO field names match `model.rs` verbatim (`fragment_refs`, `package_id`, `compressed_artifact_ref`, `source_package_id`, `fragment_count`, `artifact_ref`); `validate_context_package` reused; `ContextError::{Validation,Provider}`; workspace `CreateArtifactRequest`/`ArtifactKind` from slice 4a; LLM `ChatRequest`/`ChatMessage`/`chat` from the reasoning crates; `bridge::block_on` copied verbatim.
