# PR-14: Add application pack materialization contract for DW apps

## Title
`feat(pack-materialization): add application pack materialization contract for dw apps`

## Objective
Define the formal contract that `greentic-pack` receives from `greentic-dw` when creating a DW application pack.

## Deliverables
- `DwApplicationPackSpec`
- `DwApplicationPackAsset`
- `DwApplicationPackRequirement`
- `DwApplicationPackLayout`
- optional `DwGeneratedConfigAsset`
- optional `DwGeneratedFlowAsset`
- optional `DwGeneratedPromptAsset`

## Pack Spec Shape
- app pack metadata
- included agents
- generated assets
- required capability declarations
- dependency pack refs
- setup metadata
- i18n assets when relevant

## Acceptance Criteria
- the spec can represent a single-agent or multi-agent application pack
- the spec can be consumed by `greentic-pack`
- dependencies remain external pack refs rather than inlined provider implementations
