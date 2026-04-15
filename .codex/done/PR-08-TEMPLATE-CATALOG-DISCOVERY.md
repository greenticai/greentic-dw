# PR-08: Add template catalog and discovery for DW wizard

## Title
`feat(template-catalog): add template catalog and discovery for dw wizard`

## Objective
Add a catalog layer so the DW wizard can discover templates from repo assets and distributable sources instead of hardcoded menu entries.

## Deliverables
- `TemplateCatalogEntry`
- `TemplateCatalog`
- template loader and resolver

## Source Support
- local repo assets
- local dev paths
- `oci://...`
- `store://...`
- `repo://...`

## Catalog Entry Shape
- template id
- display name
- summary
- source ref
- version
- maturity
- tags
- capability summary
- suitability for default mode only or both modes
- whether the template supports multi-agent app packs

## Reuse Guidance
- Resolve source refs through the shared PR-06 contract.
- For non-local sources, follow `greentic-distributor-client` mapping and canonicalization rules rather than open-coding fetch assumptions in the wizard.

## Acceptance Criteria
- wizard can list templates without hardcoding them
- templates can be resolved from a source ref
- catalog format is stable enough for later reuse in `gtc wizard`
