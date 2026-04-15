# PR-03: Delegation Core Contracts

## Title
feat(dw): add delegation contracts for subtask routing and isolated subagent envelopes

## Why
Deep agents rely on subagents for specialization, isolation, and parallelization. LangChain’s guidance
explicitly frames subagents as isolated workers that prevent context bloat.

## Scope
Create a delegation core crate with:
- `DelegationProvider`
- `DelegationDecision`
- `SubtaskEnvelope`
- `SubtaskResultEnvelope`
- `MergePolicy`

## File tree
```text
crates/
  greentic-dw-delegation/
    Cargo.toml
    src/
      lib.rs
      error.rs
      traits.rs
      model.rs
      validate.rs
      fixtures.rs
```

## Concrete work

### 1) Trait
```rust
pub trait DelegationProvider: Send + Sync {
    fn choose_delegate(&self, req: DelegationRequest) -> Result<DelegationDecision, DelegationError>;
    fn start_subtask(&self, req: StartSubtaskRequest) -> Result<DelegationHandle, DelegationError>;
    fn merge_result(&self, req: MergeSubtaskResultRequest) -> Result<DelegationMergeResult, DelegationError>;
}
```

### 2) Decision modes
Support:
- `None`
- `Single`
- `Parallel`
- `MapReduce`

### 3) Envelope design
`SubtaskEnvelope` must include:
- `subtask_id`
- `parent_run_id`
- `target_agent`
- `goal`
- `context_package_ref`
- `expected_output_schema`
- `permissions_profile`
- `deadline`
- `return_policy`

### 4) Merge policy
Support:
- first_success
- collect_all
- majority_vote
- weighted_merge
- reducer_artifact

## Tests
- decision serialization
- envelope validation
- merge policy round-trip
- target-agent and schema checks

## Acceptance criteria
- Delegation is modeled without granting direct runtime mutation power to subagents.
- Envelopes are explicit and replayable.
