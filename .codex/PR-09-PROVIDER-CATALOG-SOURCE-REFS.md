# PR-09: Add provider catalog with source references for DW composition

## Title
`feat(provider-catalog): add provider catalog with source references for dw composition`

## Objective
Add a catalog for commonly used DW providers and where they come from.

## Deliverables
- `DwProviderCatalogEntry`
- `DwProviderSourceRef`
- `DwProviderCapabilityProfile`
- `DwProviderDefaultProfile`
- `DwProviderCatalog`

## Provider Entry Shape
- provider id
- family and category
- variant
- display name
- summary
- source ref
- version or channel
- maturity
- capability contract ids
- pack capability ids
- default suitability
- template compatibility
- required setup schemas or question blocks
- whether the provider is appropriate for local, dev, demo, or enterprise use

## Reuse Guidance
- Reuse the PR-06 source model and `greentic-distributor-client` source semantics.
- Keep provider metadata declarative and externalizable so `greentic-dw-providers` can become the long-term producer of these descriptors.

## Acceptance Criteria
- provider catalog can list providers by family
- provider catalog exposes default and recommended choices
- provider catalog resolves source refs for pack inclusion
- catalog supports later filtering by OSS, enterprise, dev, and prod
