# PR-41: End-to-End Deep Loop Harness

## Title
test: add end-to-end harness for deep loop lifecycle

## Scope
Add a harness that runs:
- create plan
- compile context
- execute mocked step
- persist workspace artifact
- reflect
- revise or complete

Use deterministic fixtures and avoid network calls by default.

## Acceptance criteria
- One command runs the full deep loop locally in CI.
