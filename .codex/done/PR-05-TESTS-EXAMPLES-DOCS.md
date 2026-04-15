# PR-05: Add conformance tests, examples, and docs for v0.2

## Objective
Make the new model easy to implement and verify.

## Scope
- capability-aware manifest examples
- resolution/binding examples
- tests for operation compatibility checks
- tests for task-state provider abstraction
- docs explaining:
  - capabilities
  - `pack.cbor` mappings
  - runtime bindings
  - flow/bundle/setup lifecycle

## Deliverables
- examples directory refresh
- test fixtures
- migration notes from v0.1 to v0.2
- version strings in docs/examples/tests should follow the workspace version defined at the root
- examples and fixtures should reference shared capability artifacts from `../greentic-cap` instead of copying their declarations
