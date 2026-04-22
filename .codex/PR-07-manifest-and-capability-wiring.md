# PR-07: Manifest and Capability Wiring

## Title
feat(dw): add manifest/capability support for planning, workspace, delegation, reflection, and context families

## Why
The new provider families must be discoverable and selectable in the same way as the existing families.

## Scope
Update manifest/composition/configuration code to support the five new families.

## Concrete work

### 1) Capability families
Add canonical capability identifiers:
- `greentic.cap.planning.plan`
- `greentic.cap.workspace.artifacts`
- `greentic.cap.delegation.route`
- `greentic.cap.reflection.review`
- `greentic.cap.context.compose`

### 2) Manifest family enums
Update any provider family enums/registries to include:
- planning
- workspace
- delegation
- reflection
- context

### 3) Composition validation
Require:
- if a deep loop is enabled, at minimum planning + context must be configured
- if plan step kind `Delegate` exists, delegation provider must be configured
- if reflection policy is mandatory, reflection provider must be configured

### 4) Defaults
Do not auto-enable deep mode globally.
Require explicit opt-in from app spec / pack composition.

## Tests
- manifest accepts new families
- invalid deep-loop composition rejected
- capabilities appear in inspect/validate output

## Acceptance criteria
- New families are fully discoverable in manifests and validation output.
