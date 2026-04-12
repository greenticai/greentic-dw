# PR-12: Add composable QA assembly for DW core templates and providers

## Title
`feat(qa-assembly): add composable QA assembly for dw core templates and providers`

## Objective
Define how `greentic-dw` assembles one QA flow from DW core questions, template questions, provider questions, composition questions, and packaging questions.

## Deliverables
- `DwWizardQuestionAssembly`
- `QuestionSource`
- `ModeVisibilityPolicy`
- `DefaultModeFilter`
- `PersonalisedModeFilter`

## Mode Behavior
- default mode includes only unresolved required questions, dependency-driven questions, provider questions needed for defaults, and setup questions needed for pack or bundle creation
- personalised mode includes required and optional sections, provider override sections, advanced sections, and packaging options

## Reuse Guidance
- Reuse `greentic-qa` concepts for question blocks, visibility, defaults, and conditional assembly if that repo or crate is available.
- Avoid introducing a bespoke DW-only question graph if the same concerns already live in `greentic-qa`.

## Acceptance Criteria
- one schema graph can produce both default and personalised flows
- visible-if and default logic is driven by reusable QA contracts
- providers and templates can contribute question blocks declaratively
