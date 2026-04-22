# PR-04: Update flow/bundle/setup integration for greentic-dw v0.2

## Objective
Wire `greentic-dw` into the main Greentic lifecycle and artifact flow.

## Scope
- Keep `component-dw` as a normal frequently used component in flow catalogs
- Ensure bundle/setup can surface unresolved DW capability needs
- Ensure setup can finalize environment-specific provider bindings
- Align examples with `gtc wizard`, `gtc setup`, `gtc start`, `gtc stop`

## Deliverables
- updated integration docs
- example bundle/build metadata shapes
- example setup-time binding refinements
- example `component-dw` config docs
- example artifacts should use the workspace package version from the root `Cargo.toml`
- treat the capability workspace as the source of truth for shared capability artifacts during the path-based integration phase
