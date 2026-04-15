# Incident Analysis Multi-Agent Example

This example shows a coordinator-driven incident analysis flow with explicit delegation and final review.

## What it contains

- `manifests/incident-analysis.manifest.json`
  Deep-agent manifest with delegation enabled.
- `fixtures/delegation.log-analysis.json`
  Explicit subtask envelope for the log-analysis agent.
- `fixtures/delegation.change-correlation.json`
  Explicit subtask envelope for the change-correlation agent.
- `expected/application.json`
  Multi-agent application view with coordinator and reviewer routing.
- `expected/final-review.json`
  Final review outcome after merge.

## How to run

```bash
cargo test -p greentic-dw-testing incident_analysis_example_assets_validate
```
