# PR-00: Audit greentic-dw for v0.2 capability migration

## Objective
Audit the current `greentic-dw` repo and identify all places that need migration from the older direct-reference model to the new capability-driven model.

## Outcomes
- inventory manifests/schemas/types that assume direct provider refs
- inventory runtime code that assumes hard-coded providers
- inventory docs/examples/tests that need updating
- produce a short migration plan in-repo

## Checklist
- Review DW manifest schema and examples
- Review runtime provider lookup assumptions
- Review state/resume handling assumptions
- Review flow/bundle/setup integration points
- Review any existing `pack.cbor` handling or pack metadata assumptions
- Note mismatches against v0.2 spec

## Deliverables
- audit markdown doc
- issue list / TODO list
- no major behavior change yet
