# greentic-dw-cli

Wizard CLI for creating and replaying DW manifests. The `manifest_version` answer field maps to
the manifest's `worker_version`, while the manifest schema version is fixed in the DW manifest
crate.

For template-backed multi-agent flows, the emitted `AnswerDocument` now supports two replay forms:

- legacy flat keys in `design_answers`, such as `agent.1.support_behavior`
- structured per-agent entries in `agent_answers`, keyed by stable agent ids like `agent-1`

The CLI reads both forms for compatibility, but new replay/edit integrations should prefer
`agent_answers` because it keeps agent-local behavior answers and provider overrides explicit.

See [examples/answers/support-squad-create-answers.json](/Users/maarten/Documents/GitHub/agentic/greentic-dw/examples/answers/support-squad-create-answers.json) for a multi-agent replay example using the preferred structured shape.
