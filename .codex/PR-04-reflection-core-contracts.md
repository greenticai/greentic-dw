# PR-04: Reflection Core Contracts

## Title
feat(dw): add reflection contracts for step review, plan review, and final review

## Why
Deep agents need an explicit critique/review mechanism so outputs can be accepted, revised, retried,
or escalated. This should remain auditable and pluggable rather than being buried inside the engine.

## Scope
Create a reflection core crate with:
- `ReflectionProvider`
- `ReviewOutcome`
- `ReviewFinding`
- `SuggestedAction`
- `ReviewTarget`

## File tree
```text
crates/
  greentic-dw-reflection/
    Cargo.toml
    src/
      lib.rs
      error.rs
      traits.rs
      model.rs
      fixtures.rs
```

## Trait
```rust
pub trait ReflectionProvider: Send + Sync {
    fn review_step(&self, req: ReviewStepRequest) -> Result<ReviewOutcome, ReflectionError>;
    fn review_plan(&self, req: ReviewPlanRequest) -> Result<ReviewOutcome, ReflectionError>;
    fn review_final(&self, req: ReviewFinalRequest) -> Result<ReviewOutcome, ReflectionError>;
}
```

## Review outcome
`verdict` enum:
- `Accept`
- `Revise`
- `Retry`
- `Delegate`
- `Fail`

Add:
- `score: Option<f32>`
- `findings: Vec<ReviewFinding>`
- `suggested_actions: Vec<SuggestedAction>`
- `binding: bool`

## Tests
- round-trip serialization
- advisory vs binding behavior retained in model
- suggested actions carry typed references to plan step / artifact / agent

## Acceptance criteria
- Reflection outcomes are typed and reusable.
- No provider implementation yet.
