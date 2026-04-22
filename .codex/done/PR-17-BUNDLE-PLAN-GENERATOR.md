# PR-17: Generate bundle inclusion plan from DW composition

## Title
`feat(bundle-plan-generator): generate bundle inclusion plan from dw composition`

## Objective
Generate the final pack list for `greentic-bundle`.

## Deliverables
- logic to collect selected provider packs
- collect the generated application pack
- deduplicate shared pack refs
- preserve agent-specific provenance where useful
- produce a deterministic ordered pack list

## Acceptance Criteria
- bundle plan deduplicates shared provider packs
- bundle plan preserves one generated app pack containing many workers
- output is deterministic and testable
