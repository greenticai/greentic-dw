# PR-08: Fixtures, Docs, and Migration Notes in Core Repo

## Title
docs(dw): add core fixtures, migration notes, and authoring guidance for deep-agent families

## Scope
Add:
- fixture JSON/CBOR examples for all five family documents
- authoring notes
- migration notes for existing apps that stay non-deep
- developer-facing examples of plan/context/artifact/review docs

## File tree
```text
fixtures/deep/
  plan.basic.json
  plan.basic.cbor
  context.basic.json
  artifact.note.json
  review.accept.json
  delegation.single.json

docs/
  deep-agents/
    contracts-overview.md
    authoring-notes.md
    migration.md
```

## Acceptance criteria
- Every new model has at least one fixture.
- Docs explain that these families are optional and opt-in.
