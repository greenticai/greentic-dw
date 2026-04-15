# PR-01: Planning Core Contracts

## Title
feat(dw): add planning core contracts, plan documents, and validation

## Why
Deep agents need an explicit work structure that can be inspected, revised, replayed, and audited.
Greentic DW already has deterministic runtime patterns and multi-agent composition, but it lacks a first-class
planning family for task graphs and replanning.

## Scope
Create a new planning core crate in `greentic-dw` that defines:
- `PlanningProvider`
- `PlanDocument`
- `PlanStep`
- `PlanEdge`
- `PlanRevision`
- `CompletionState`
- validators and fixture helpers

## Target file tree
```text
crates/
  greentic-dw-planning/
    Cargo.toml
    src/
      lib.rs
      error.rs
      traits.rs
      model.rs
      validate.rs
      fixtures.rs
      serde.rs
```

## Concrete work

### 1) Add crate
Create `crates/greentic-dw-planning`.

### 2) Add public trait
```rust
pub trait PlanningProvider: Send + Sync {
    fn create_plan(&self, req: CreatePlanRequest) -> Result<PlanDocument, PlanningError>;
    fn revise_plan(&self, req: RevisePlanRequest) -> Result<PlanRevision, PlanningError>;
    fn next_actions(&self, req: NextActionsRequest) -> Result<Vec<PlannedAction>, PlanningError>;
    fn record_step_result(&self, req: StepResultRequest) -> Result<PlanDocument, PlanningError>;
    fn evaluate_completion(&self, req: CompletionCheckRequest) -> Result<CompletionState, PlanningError>;
}
```

### 3) Add model
`PlanDocument` must include:
- `plan_id`
- `goal`
- `status`
- `revision`
- `assumptions: Vec<String>`
- `constraints: Vec<String>`
- `success_criteria: Vec<String>`
- `steps: Vec<PlanStep>`
- `edges: Vec<PlanEdge>`
- `metadata: BTreeMap<String, String>`

`PlanStep` must include:
- `step_id`
- `title`
- `kind: PlanStepKind`
- `status: PlanStepStatus`
- `depends_on: Vec<String>`
- `assigned_agent: Option<String>`
- `inputs_schema_ref: Option<String>`
- `output_schema_ref: Option<String>`
- `retry_count: u32`

Step kinds:
- `Research`
- `ToolCall`
- `Delegate`
- `Review`
- `Compose`
- `Decision`
- `Custom(String)`

### 4) Validation rules
Add `validate.rs` with:
- unique `plan_id`
- unique `step_id`
- no dangling dependencies
- no cycles unless `allow_cycles` feature is explicitly enabled
- blocked step detection
- illegal terminal states rejected

### 5) Serialization
Add JSON and CBOR round-trip tests.

## Integration
Expose the crate from the workspace root and update any shared facade crate if present.

## Tests
Add:
- `plan_round_trip_json`
- `plan_round_trip_cbor`
- `plan_rejects_duplicate_step_ids`
- `plan_rejects_unknown_dependency`
- `plan_rejects_cycle`
- `next_actions_returns_only_ready_steps`

## Acceptance criteria
- Planning core compiles with no provider implementation yet.
- All models are serializable.
- Validators reject invalid graphs deterministically.
- `next_actions()` can be used by runtime integration in a later PR.

## Out of scope
- LLM planning logic
- runtime execution changes
- provider selection
