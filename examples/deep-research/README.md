# Deep Research Example

This example shows a deterministic deep-agent research flow using planning, context, workspace, and reflection.

## What it contains

- `manifests/deep-research.manifest.json`
  Opt-in manifest with deep-agent capability wiring.
- `fixtures/plan.json`
  A three-step plan: collect sources, draft notes, review report.
- `fixtures/context.json`
  Compiled context with workspace and prompt fragments.
- `expected/notes.artifact.json`
  Example intermediate notes artifact.
- `expected/report.artifact.json`
  Example final report artifact.
- `expected/review.json`
  Reflection result checking evidence coverage.

## How to run

Use the repo test that validates the example assets:

```bash
cargo test -p greentic-dw-testing deep_research_example_assets_validate
```
