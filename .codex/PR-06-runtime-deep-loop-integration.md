# PR-06: Runtime Deep Loop Integration

## Title
feat(dw): integrate planning, context, workspace, reflection, and delegation into the runtime loop

## Why
Once the core contracts exist, the runtime needs an explicit deep-agent loop:
plan -> context -> execute -> workspace -> reflect -> revise/delegate -> continue.

## Dependencies
- PR-01 through PR-05

## Scope
Add a runtime orchestration module that wires the new families together without breaking
existing deterministic execution semantics.

## File tree
```text
crates/
  greentic-dw-runtime/
    src/
      deep_loop.rs
      deep_loop_types.rs
      deep_loop_tests.rs
```

## Concrete work

### 1) Add `DeepLoopCoordinator`
```rust
pub struct DeepLoopCoordinator<'a> {
    pub planner: &'a dyn PlanningProvider,
    pub context: &'a dyn ContextProvider,
    pub workspace: &'a dyn WorkspaceProvider,
    pub reflector: &'a dyn ReflectionProvider,
    pub delegator: &'a dyn DelegationProvider,
}
```

### 2) Add step orchestration
Pseudo-flow:
1. create or load plan
2. fetch ready steps
3. compile step context
4. execute via existing runtime/tool/engine path
5. persist outputs into workspace
6. review step result
7. revise plan or delegate if needed
8. stop only when completion state is terminal

### 3) Preserve existing execution authority
Do not let new families bypass:
- control
- engine selection
- observer hooks

### 4) Add state transitions
Explicit loop statuses:
- idle
- planning
- executing
- reflecting
- revising
- delegating
- completed
- failed

## Tests
- runtime executes a 2-step plan deterministically
- failed reflection causes revise
- delegation step emits `SubtaskEnvelope`
- final completion only when planner returns terminal completion

## Acceptance criteria
- Deep loop works with stub providers.
- Existing non-deep flows continue to work unchanged.
