# PR-06: Add pack source resolution contract for DW providers and templates

## Title
`feat(source-resolution): add pack source resolution contract for dw providers and templates`

## Objective
Define one canonical source-ref model for DW templates, provider catalogs, composition documents, and bundle plans.

## Why First
Template catalogs and provider catalogs both need the same source language. This should not be duplicated.

## Deliverables
- `PackSourceRef`
- `TemplateSourceRef`
- `SourceResolutionPolicy`
- support for `oci://`
- support for `store://`
- support for `repo://`
- support for local path and dev refs

## Reuse Guidance
- Align the contract with `greentic-distributor-client` artifacts and resolution semantics, especially the source kind split and canonical-ref/provenance concepts already used for `oci`, `repo`, and `store`.
- Do not introduce a DW-only URI grammar if `greentic-distributor-client` already models the same distinction.

## Acceptance Criteria
- one canonical source-ref model is used across template and provider catalogs
- the contract is compatible with future distributor, repo, and store usage
- the source-ref model can be embedded in composition docs and bundle plans
