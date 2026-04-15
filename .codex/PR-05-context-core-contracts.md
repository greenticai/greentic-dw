# PR-05: Context Core Contracts

## Title
feat(dw): add context contracts for build, compression, summarization, and provenance

## Why
LangChain’s deep-agent docs treat context engineering as a first-class capability, including
input context, runtime context, compression, offloading, summarization, and isolation.

## Scope
Create a context core crate with:
- `ContextProvider`
- `ContextPackage`
- `ContextFragment`
- `ContextBudget`
- `CompressedContext`
- `SummaryArtifactRef`

## File tree
```text
crates/
  greentic-dw-context/
    Cargo.toml
    src/
      lib.rs
      error.rs
      traits.rs
      model.rs
      fixtures.rs
      validate.rs
```

## Trait
```rust
pub trait ContextProvider: Send + Sync {
    fn build_context(&self, req: BuildContextRequest) -> Result<ContextPackage, ContextError>;
    fn compress_context(&self, req: CompressContextRequest) -> Result<CompressedContext, ContextError>;
    fn summarize_context(&self, req: SummarizeContextRequest) -> Result<SummaryArtifactRef, ContextError>;
}
```

## Design constraints
- `ContextPackage` is an inspectable artifact, not a raw prompt string.
- Must include provenance for every fragment.
- Must support references to:
  - memory items
  - workspace artifacts
  - plan steps
  - runtime metadata

## Tests
- context package round-trip
- provenance preservation
- budget validation
- fragment ordering deterministic

## Acceptance criteria
- Context exists as a compiled document.
- Later providers can consume memory/workspace/plan references.
