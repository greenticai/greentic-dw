# PR-13: Resolve template answers into provider-bound DW composition

## Title
`feat(composition-resolver): resolve template answers into provider-bound dw composition`

## Objective
Add the logic that takes the template descriptor, provider catalog, and QA answers and produces a resolved composition document.

## Deliverables
- resolver logic for applying template defaults
- selecting provider defaults
- applying user overrides
- validating required capabilities
- resolving provider source refs
- building the pack dependency list
- building multi-agent composition

## Acceptance Criteria
- default-mode answers can produce a complete composition when defaults are sufficient
- personalised mode can override any binding or config allowed by the template
- unresolved items are surfaced explicitly
