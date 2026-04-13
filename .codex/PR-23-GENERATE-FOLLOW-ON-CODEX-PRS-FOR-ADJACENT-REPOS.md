# PR-23 — Generate follow-on Codex PR docs for adjacent repos

## Title
`feat(codex-docs): generate follow-on PR docs for adjacent repos from dw audit`

## Objective
After the audit is complete, generate reviewable `.codex` PR docs for the sibling repos that need changes.

## Candidate repos
- `../greentic-pack`
- `../greentic-bundle`
- `../greentic-setup`
- `../greentic-start`
- `../greentic-flow`
- `../greentic-component` only if the audit shows real need

## Required behavior
This PR should not implement cross-repo code changes. Instead, it should generate:
- repo-specific PR markdown docs
- goals
- deliverables
- acceptance criteria
- dependencies on DW-side contracts

## Minimum expected PR outputs
### For `greentic-pack`
A PR to add or extend:
- `Create/update application pack`
- `3) Add/edit digital workers`
- delegation into the DW design wizard

### For `greentic-bundle`
A PR to include capability packs automatically when app packs or extension packs declare required capabilities.

### For `greentic-setup`
A PR to consume deferred DW/provider setup requirements.

### For `greentic-start`
A PR to validate unresolved setup/runtime requirements before launch.

### For `greentic-flow`
A PR only if generated DW flow assets need explicit support.

### For `greentic-component`
A PR only if templates truly require component generation in v1.

## Acceptance criteria
- Follow-on PR docs are generated, not just described
- They are repo-specific
- They reference the audited reality rather than assumptions
- They are ready for user review before any code is applied
