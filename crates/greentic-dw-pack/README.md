# greentic-dw-pack

Hook/sub and capability integration surfaces for Digital Worker runtime.

This crate reuses the shared capability model from the versioned `greentic-cap` crates
and exposes a DW-facing
facade for `pack.cbor` capability sections, CBOR encode/decode helpers, and compatibility checks
against provider component self-descriptions.

It also exposes bundle/setup-facing helpers for the normal lifecycle:

- build bundle resolution artifacts from shared resolution reports
- surface unresolved capability request ids for setup-time refinement
- keep `component-dw` aligned with the versioned `greentic-cap` crates
