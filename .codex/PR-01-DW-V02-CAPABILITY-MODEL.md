# PR-01: Migrate greentic-dw core model to v0.2 capability-driven design

## Objective
Update core DW schemas/types/manifests to use logical capabilities and the new dependency model.

## Scope
- Add capability-aware fields in DW manifest v0.2
- Replace old direct dependency references with:
  - `profiles`
  - `requires`
  - optional runtime-facing `consumes` where needed
- Add capability families for DW, for example:
  - `cap://dw.engine.default`
  - `cap://dw.state.task-store`
  - `cap://dw.memory.session`
  - `cap://dw.observer.audit`
  - `cap://dw.control.basic`
  - `cap://dw.tool.*`

## Suggested schema direction
```yaml
version: 0.2
id: support.customer-orchestrator
profiles:
  - cap://profile/dw.production
requires:
  - cap://dw.engine.default
  - cap://dw.state.task-store
  - cap://dw.memory.short-term
  - cap://dw.observer.audit
consumes:
  - id: cap://dw.memory.short-term
    binding: short_term_memory
```

## Deliverables
- new manifest schema version
- updated Rust types
- migration/defaulting logic
- examples updated to v0.2
- workspace versioning remains centralized in the root `Cargo.toml`; do not reintroduce per-crate version literals
- reuse the shared capability declaration model from `../greentic-cap` by path instead of duplicating capability primitives locally
