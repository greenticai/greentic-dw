//! Workspace root package for greentic-dw.
//!
//! It hosts the workspace-level performance tests (`tests/`) and the `perf`
//! benchmark (`benches/`). The `greentic-dw` binary lives in
//! `crates/greentic-dw-cli` (`src/bin/greentic-dw.rs`), which is outside the
//! workspace until greentic-dw-authoring is published on the 1.2.0-dev
//! crates.io lane — see the `exclude` comment in the root `Cargo.toml`.
