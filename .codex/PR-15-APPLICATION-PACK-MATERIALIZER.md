# PR-15: Generate DW application pack spec from resolved composition

## Title
`feat(pack-materializer): generate dw application pack spec from resolved composition`

## Objective
Implement the conversion from the resolved composition document to `DwApplicationPackSpec`.

## Deliverables
- materializer logic that generates app-pack metadata
- per-agent asset layout
- capability requirements
- provider dependency refs
- setup and config assets

## Acceptance Criteria
- one composition with one agent produces one app pack spec
- one composition with many agents produces one multi-agent app pack spec
- provider packs appear as dependencies rather than duplicated assets
