# PR-18: Add starter DW templates and provider catalog entries

## Title
`feat(starter-catalogs): add starter dw templates and provider catalog entries`

## Objective
Ship useful built-in examples so the wizard is not empty.

## Deliverables
- starter templates: `support assistant`, `approval worker`, `workflow executor`
- starter provider catalog entries for `engine`, `llm`, `memory`, `control`, `observer`, `tool`, and `task-store`

## Reuse Guidance
- Prefer starter entries that point at real pack or catalog refs already compatible with `oci://`, `store://`, or `repo://` flows.
- Keep starter catalogs as data that can later move out of this repo instead of hardcoding them into the wizard path.

## Acceptance Criteria
- wizard can load starter templates from catalog
- resolver can pick default providers from the starter provider catalog
- outputs produce valid composition and pack and bundle plans
