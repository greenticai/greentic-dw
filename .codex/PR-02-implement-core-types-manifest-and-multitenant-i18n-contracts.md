# PR-02 — Implement DW core types, manifest schema, locale handling, and tenant/team contracts

## Objective / Outcome
Introduce the canonical DW type layer and manifest/schema foundation, including locale-aware Digital Workers and mandatory tenant scope with optional team scope.

## Repo status
New repo

## Depends on
- PR-01 repo bootstrap landed

## Scope
- Implement `greentic-dw-types` and `greentic-dw-manifest` foundations.
- Define task envelope, lifecycle enums, manifest structs, and validation rules.
- Add explicit locale/i18n model: worker locale policy, requested locale, human locale, locale propagation, and output locale guidance.
- Add multi-tenant contracts: tenant required, team optional, inheritance/override semantics.

## Acceptance criteria
- Canonical DW types compile and are documented.
- Schema and Rust types cover locale-aware DW behavior.
- Tenant and optional team scope are part of public contracts.

## Codex prompt
```text
Implement the first real DW contracts in the new `greentic-dw` repo.

Create canonical core types and manifest/schema support, with first-class:
- locale-aware DW behavior
- tenant scope
- optional team scope

Deliverables:
- `greentic-dw-types`
- `greentic-dw-manifest`
- task envelope types
- lifecycle enums
- public manifest structs
- schema validation tests
- docs for locale + tenant/team semantics
```
