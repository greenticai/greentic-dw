//! Build `.gtpack` archives from [`DwApplicationPackSpec`].
//!
//! Bridges the resolved DW handoff contract
//! ([`greentic_dw_types::DwApplicationPackSpec`]) and the canonical pack
//! writer ([`greentic_pack::builder::PackBuilder`]). Output is a tenant-
//! installable `.gtpack` ZIP archive.
//!
//! ## v0.1 scope (Phase B.2 first slice)
//!
//! Mechanical translation of spec metadata + agents → `PackMeta`:
//! - `metadata` → `pack_id`, `version`, `name`, `description`, `kind`
//! - `agents[*].agent_id` → entry_flows when no generated_flows present
//!
//! **Asset content NOT yet wired.** `DwApplicationPackSpec` carries asset
//! *descriptors* (path, format, kind) but not bytes. v0.2 will add an
//! `AssetSupplier` callback so the wizard / deployer can resolve bytes
//! from filesystem, OCI, or store. For now, the produced pack contains
//! only the manifest + sbom + signatures (no asset payloads).
//!
//! ## v0.2 backlog
//!
//! - `AssetSupplier` trait for byte resolution
//! - Wire `spec.assets` / `spec.generated_*` into `with_asset_bytes`
//! - Swap `PackKind::Application` placeholder for `PackKind::DwApplication`
//!   (depends on greentic-pack Phase B.1 merge)
//! - Deterministic timestamp (golden snapshot tests)

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::unwrap_used, clippy::expect_used)]

mod meta;

pub use meta::{DwPackBuildOptions, PackMetaBuildError};

use std::path::{Path, PathBuf};

use greentic_dw_types::DwApplicationPackSpec;
use greentic_pack::builder::{BuildResult, FlowBundle, PackBuilder};
use serde_json::json;
use thiserror::Error;

/// Errors produced by [`build_dw_pack`].
#[derive(Debug, Error)]
pub enum DwPackBuildError {
    /// `DwApplicationPackSpec` was rejected during translation to `PackMeta`.
    #[error("invalid pack spec: {0}")]
    InvalidSpec(#[from] PackMetaBuildError),

    /// Underlying `PackBuilder` failed to write the archive.
    #[error("pack builder error: {0}")]
    Build(#[from] anyhow::Error),
}

/// Result returned by [`build_dw_pack`].
#[derive(Debug)]
pub struct DwPackArtifact {
    /// Path of the produced `.gtpack` file on disk.
    pub pack_path: PathBuf,
    /// Pack id from `spec.metadata.pack_id`.
    pub pack_id: String,
    /// Underlying `BuildResult` from `greentic-pack`.
    pub build: BuildResult,
}

/// Build a `.gtpack` archive from a `DwApplicationPackSpec`.
///
/// Uses default options (current UTC clock, dev signing). For deterministic
/// output (golden tests), use [`build_dw_pack_with_options`].
pub fn build_dw_pack(
    spec: &DwApplicationPackSpec,
    out_path: impl AsRef<Path>,
) -> Result<DwPackArtifact, DwPackBuildError> {
    build_dw_pack_with_options(spec, out_path, DwPackBuildOptions::default())
}

/// Build a `.gtpack` archive with explicit build options.
///
/// Use this for tests + deterministic byte-stable output.
pub fn build_dw_pack_with_options(
    spec: &DwApplicationPackSpec,
    out_path: impl AsRef<Path>,
    options: DwPackBuildOptions,
) -> Result<DwPackArtifact, DwPackBuildError> {
    let pack_path = out_path.as_ref().to_path_buf();
    let pack_id = spec.metadata.pack_id.clone();
    let meta = meta::build_pack_meta(spec, &options)?;
    let entry_flow_ids = meta.entry_flows.clone();

    let mut builder = PackBuilder::new(meta);

    // v0.1 stub: PackBuilder requires at least one flow. Emit a placeholder
    // flow per entry-flow id so the archive validates structurally. v0.2
    // will replace these with real flow bytes resolved from
    // `spec.generated_flows` via an `AssetSupplier` callback.
    for flow_id in &entry_flow_ids {
        builder = builder.with_flow(stub_flow_bundle(flow_id));
    }

    // v0.2 hook: wire spec.assets / generated_* via builder.with_asset_bytes.
    // Skipped here because the spec types only carry asset descriptors
    // (path/format/kind), not bytes. Caller-provided supplier in v0.2.

    let build = builder.build(&pack_path)?;

    Ok(DwPackArtifact {
        pack_path,
        pack_id,
        build,
    })
}

/// Build a minimal placeholder [`FlowBundle`] for v0.1 scaffolding.
///
/// v0.2 replaces this with a resolver that reads bytes from
/// `spec.generated_flows[*].path` via the caller-provided supplier.
fn stub_flow_bundle(flow_id: &str) -> FlowBundle {
    let flow_json = json!({
        "id": flow_id,
        "kind": "flow/v1",
        "entry": "start",
        "nodes": [],
    });
    let bytes = serde_json::to_vec(&flow_json).unwrap_or_default();
    let hash = blake3::hash(&bytes).to_hex().to_string();

    FlowBundle {
        id: flow_id.to_string(),
        kind: "flow/v1".to_string(),
        entry: "start".to_string(),
        yaml: format!("id: {flow_id}\nentry: start\n"),
        json: flow_json,
        hash_blake3: hash,
        nodes: Vec::new(),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    use greentic_dw_types::{
        DwApplicationPackAgent, DwApplicationPackLayout, DwApplicationPackMetadata,
        DwApplicationPackSpec,
    };
    use tempfile::tempdir;

    fn sample_spec() -> DwApplicationPackSpec {
        DwApplicationPackSpec {
            metadata: DwApplicationPackMetadata {
                pack_id: "pack.test.dw.demo".to_string(),
                application_id: "app.demo".to_string(),
                display_name: "Demo DW Application".to_string(),
                version: Some("0.1.0".to_string()),
                multi_agent: false,
            },
            agents: vec![DwApplicationPackAgent {
                agent_id: "demo-agent".to_string(),
                display_name: "Demo Agent".to_string(),
                template_id: "orchestrator-default".to_string(),
                asset_root: "agents/demo-agent".to_string(),
            }],
            assets: Vec::new(),
            generated_configs: Vec::new(),
            generated_flows: Vec::new(),
            generated_prompts: Vec::new(),
            requirements: Vec::new(),
            dependency_pack_refs: Vec::new(),
            setup_requirements: Vec::new(),
            layout: DwApplicationPackLayout {
                app_root: "app".to_string(),
                shared_asset_roots: Vec::new(),
                layout_hint: None,
            },
        }
    }

    #[test]
    fn build_emits_gtpack_file() {
        let dir = tempdir().expect("tempdir");
        let out = dir.path().join("demo.gtpack");

        let spec = sample_spec();
        let artifact = build_dw_pack(&spec, &out).expect("build ok");

        assert_eq!(artifact.pack_path, out);
        assert_eq!(artifact.pack_id, "pack.test.dw.demo");
        assert!(out.exists(), "output gtpack must exist");
        let metadata = std::fs::metadata(&out).expect("metadata");
        assert!(metadata.len() > 0, "gtpack must not be empty");
    }

    #[test]
    fn build_rejects_spec_with_no_agents() {
        let dir = tempdir().expect("tempdir");
        let out = dir.path().join("empty.gtpack");

        let mut spec = sample_spec();
        spec.agents.clear();

        let err = build_dw_pack(&spec, &out).expect_err("must reject");
        match err {
            DwPackBuildError::InvalidSpec(_) => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn build_with_options_uses_fixed_now() {
        use time::OffsetDateTime;

        let dir = tempdir().expect("tempdir");
        let out = dir.path().join("fixed.gtpack");

        let spec = sample_spec();
        let opts = DwPackBuildOptions {
            fixed_now: Some(OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("epoch")),
            ..Default::default()
        };

        let artifact = build_dw_pack_with_options(&spec, &out, opts).expect("build ok");
        assert!(artifact.pack_path.exists());
    }
}
