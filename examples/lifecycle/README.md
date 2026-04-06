# DW Lifecycle Examples

These examples show the normal `component-dw` path through the Greentic lifecycle without
duplicating capability declarations in this repo.

Reuse boundary:
- shared capability declarations live in `examples/capability`
- DW consumes those declarations through the local capability workspace crates
- bundle/setup outputs below show how unresolved needs and finalized bindings are surfaced

## Bundle Resolution Shape

`component-dw.bundle.json` shows the metadata a bundle/setup step can emit after resolving the
DW declaration against the shared capability workspace.

## Setup Refinement Shape

`component-dw.setup.json` shows how environment-specific provider bindings can be finalized
after the bundle step.

## Lifecycle Mapping

- `gtc wizard`: builds the DW manifest and answer document
- `gtc setup`: resolves capability needs and finalizes environment-specific bindings
- `gtc start`: starts runtime execution with resolved bindings
- `gtc stop`: persists or tears down runtime state through the provider-agnostic state path
