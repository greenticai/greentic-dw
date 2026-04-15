# PR-16: Add bundle inclusion plan for DW application packs and provider packs

## Title
`feat(bundle-plan): add bundle inclusion plan for dw application packs and provider packs`

## Objective
Define the exact handoff contract for `greentic-bundle`.

## Deliverables
- `DwBundlePlan`
- `BundlePackInclusion`
- `BundleSourceResolution`
- `GeneratedAppPackRef`
- `ProviderPackRef`
- `SupportPackRef`

## Plan Coverage
- generated application pack
- selected provider packs
- template-required support packs
- version and source refs
- inclusion metadata

## Acceptance Criteria
- bundle plan can be generated from composition
- bundle plan supports multi-agent app packs
- the plan contains source refs for all selected packs
