# Deep Pack / Bundle Integration Example

This example shows how a typed composition can be turned into a DW application pack spec and final bundle plan.

## What it contains

- `manifests/deep-pack.manifest.json`
  Deep-agent manifest example.
- `fixtures/composition.json`
  Resolved composition used as the source of truth.
- `expected/application-pack.json`
  Expected output of `DwCompositionDocument::to_application_pack_spec()`.
- `expected/bundle-plan.json`
  Expected output of `DwCompositionDocument::to_bundle_plan()`.
- `expected/inspect-output.json`
  Example inspect/validation view for humans and tooling.

## How to run

```bash
cargo test -p greentic-dw-testing deep_pack_bundle_example_matches_generated_outputs
```
