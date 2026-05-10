//! Build `.gtpack` archives from [`DwApplicationPackSpec`].
//!
//! Bridges the resolved DW handoff contract
//! ([`greentic_dw_types::DwApplicationPackSpec`]) and the canonical pack
//! writer ([`greentic_pack::builder::PackBuilder`]). Output is a tenant-
//! installable `.gtpack` ZIP archive.
//!
//! ## v0.2 scope (Phase B.2 second slice)
//!
//! - Mechanical translation `spec.metadata` + `spec.agents` → `PackMeta`
//!   (carried over from v0.1)
//! - **Asset wiring**: [`AssetSupplier`] trait resolves bytes for
//!   `spec.assets`, `spec.generated_configs`, `spec.generated_flows`,
//!   `spec.generated_prompts`. Caller provides supplier ([`MapAssetSupplier`]
//!   for tests, custom impl for filesystem / OCI / store backends).
//! - **Deterministic builds**: pair [`DwPackBuildOptions::fixed_now`] with
//!   a deterministic supplier to get byte-stable `.gtpack` output for
//!   golden snapshot testing.
//!
//! ## v0.3 backlog
//!
//! - Promote `spec.generated_flows` to real [`FlowBundle`]s (currently
//!   written as plain assets; flow body still resolves through supplier).
//! - Swap `PackKind::Application` placeholder for `PackKind::DwApplication`
//!   once a `greentic-pack-lib` version with that variant publishes to
//!   crates.io (greenticai/greentic-pack#147 merged into research).
//! - `FsAssetSupplier` for filesystem-rooted resolution.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::unwrap_used, clippy::expect_used)]

mod meta;
mod supplier;

pub use meta::{DwPackBuildOptions, PackMetaBuildError};
pub use supplier::{
    AssetDescriptor, AssetSupplier, AssetSupplierError, ChainedAssetSupplier, FsAssetSupplier,
    HttpAssetSupplier, MapAssetSupplier, NoAssetSupplier,
};

use std::path::{Path, PathBuf};

use greentic_dw_types::DwApplicationPackSpec;
use greentic_pack::builder::{BuildResult, FlowBundle, PackBuilder};
use serde_json::json;
use thiserror::Error;

/// Errors produced by [`build_dw_pack`] and friends.
#[derive(Debug, Error)]
pub enum DwPackBuildError {
    /// `DwApplicationPackSpec` was rejected during translation to `PackMeta`.
    #[error("invalid pack spec: {0}")]
    InvalidSpec(#[from] PackMetaBuildError),

    /// Supplier failed to resolve bytes for a declared asset.
    #[error("asset supplier error: {0}")]
    Supplier(#[from] AssetSupplierError),

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

/// Build a `.gtpack` archive from a spec, with a [`NoAssetSupplier`].
///
/// Use this only when the spec declares no assets. For specs with
/// generated configs / flows / prompts / static assets, call
/// [`build_dw_pack_with_supplier`] or [`build_dw_pack_with_options`].
pub fn build_dw_pack(
    spec: &DwApplicationPackSpec,
    out_path: impl AsRef<Path>,
) -> Result<DwPackArtifact, DwPackBuildError> {
    build_dw_pack_with_options(
        spec,
        out_path,
        &NoAssetSupplier,
        DwPackBuildOptions::default(),
    )
}

/// Build a `.gtpack` archive with a custom [`AssetSupplier`] but default
/// metadata options (current UTC clock, default kind).
pub fn build_dw_pack_with_supplier(
    spec: &DwApplicationPackSpec,
    out_path: impl AsRef<Path>,
    supplier: &dyn AssetSupplier,
) -> Result<DwPackArtifact, DwPackBuildError> {
    build_dw_pack_with_options(spec, out_path, supplier, DwPackBuildOptions::default())
}

/// Build a `.gtpack` archive with full control over supplier and options.
///
/// Use [`DwPackBuildOptions::fixed_now`] + a deterministic supplier
/// (e.g. [`MapAssetSupplier`] with stable byte content) to produce
/// byte-stable output suitable for golden snapshot tests.
pub fn build_dw_pack_with_options(
    spec: &DwApplicationPackSpec,
    out_path: impl AsRef<Path>,
    supplier: &dyn AssetSupplier,
    options: DwPackBuildOptions,
) -> Result<DwPackArtifact, DwPackBuildError> {
    let pack_path = out_path.as_ref().to_path_buf();
    let pack_id = spec.metadata.pack_id.clone();
    let meta = meta::build_pack_meta(spec, &options)?;
    let entry_flow_ids = meta.entry_flows.clone();

    let mut builder = PackBuilder::new(meta);

    // PackBuilder requires at least one flow. v0.2 still emits stub flows
    // per entry-flow id; v0.3 will promote `spec.generated_flows` to real
    // FlowBundles (resolved via supplier).
    for flow_id in &entry_flow_ids {
        builder = builder.with_flow(stub_flow_bundle(flow_id));
    }

    builder = attach_assets(builder, spec, supplier)?;

    let build = builder.build(&pack_path)?;

    Ok(DwPackArtifact {
        pack_path,
        pack_id,
        build,
    })
}

/// Attach asset descriptors as `with_asset_bytes` entries.
///
/// **Path note**: `PackBuilder::with_asset_bytes` namespaces all entries
/// under `assets/` in the produced archive. A spec asset declared with
/// `path = "assets/icon.bin"` lands at `assets/assets/icon.bin` in the
/// archive. v0.3 may strip the `assets/` prefix from spec paths if the
/// upstream pack format treats them as repeating.
fn attach_assets(
    mut builder: PackBuilder,
    spec: &DwApplicationPackSpec,
    supplier: &dyn AssetSupplier,
) -> Result<PackBuilder, DwPackBuildError> {
    for asset in &spec.assets {
        let bytes = supplier.provide(AssetDescriptor::Asset(asset))?;
        builder = builder.with_asset_bytes(asset.path.clone(), bytes);
    }
    for config in &spec.generated_configs {
        let bytes = supplier.provide(AssetDescriptor::Config(config))?;
        builder = builder.with_asset_bytes(config.path.clone(), bytes);
    }
    // Generated flows are written as static asset bytes here (caller-provided
    // raw flow body). v0.3 will additionally promote them to real
    // FlowBundles by parsing the body — currently the FlowBundle for each
    // entry-flow id is still the stub from `stub_flow_bundle`.
    for flow in &spec.generated_flows {
        let bytes = supplier.provide(AssetDescriptor::Flow(flow))?;
        builder = builder.with_asset_bytes(flow.path.clone(), bytes);
    }
    for prompt in &spec.generated_prompts {
        let bytes = supplier.provide(AssetDescriptor::Prompt(prompt))?;
        builder = builder.with_asset_bytes(prompt.path.clone(), bytes);
    }
    Ok(builder)
}

/// Build a minimal placeholder [`FlowBundle`] for the entry-flow stub.
///
/// v0.3 replaces this with a resolver that reads bytes from
/// `spec.generated_flows[*]` via the supplier and parses them into real
/// flow definitions.
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
        DwApplicationPackAgent, DwApplicationPackAsset, DwApplicationPackAssetKind,
        DwApplicationPackLayout, DwApplicationPackMetadata, DwApplicationPackSpec,
        DwGeneratedConfigAsset,
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
    }

    #[test]
    fn build_rejects_spec_with_no_agents() {
        let dir = tempdir().expect("tempdir");
        let out = dir.path().join("empty.gtpack");

        let mut spec = sample_spec();
        spec.agents.clear();

        let err = build_dw_pack(&spec, &out).expect_err("must reject");
        assert!(matches!(err, DwPackBuildError::InvalidSpec(_)));
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

        let artifact =
            build_dw_pack_with_options(&spec, &out, &NoAssetSupplier, opts).expect("build ok");
        assert!(artifact.pack_path.exists());
    }

    #[test]
    fn build_with_supplier_writes_static_asset() {
        let dir = tempdir().expect("tempdir");
        let out = dir.path().join("with-asset.gtpack");

        let mut spec = sample_spec();
        spec.assets.push(DwApplicationPackAsset {
            asset_id: "logo".to_string(),
            path: "assets/logo.bin".to_string(),
            kind: DwApplicationPackAssetKind::Generic,
            content_type: Some("application/octet-stream".to_string()),
            applies_to_agents: Vec::new(),
            source_ref: None,
        });

        let supplier = MapAssetSupplier::new().with("logo", b"LOGOBYTES".to_vec());

        build_dw_pack_with_supplier(&spec, &out, &supplier).expect("build ok");

        let bytes = std::fs::read(&out).expect("read pack");
        // PackBuilder prefixes asset paths under `assets/`, so the spec
        // path "assets/logo.bin" lands at "assets/assets/logo.bin".
        let needle = b"assets/assets/logo.bin";
        assert!(
            bytes.windows(needle.len()).any(|w| w == needle),
            "asset path must appear in archive"
        );
    }

    #[test]
    fn build_with_supplier_writes_generated_config() {
        let dir = tempdir().expect("tempdir");
        let out = dir.path().join("with-config.gtpack");

        let mut spec = sample_spec();
        spec.generated_configs.push(DwGeneratedConfigAsset {
            asset_id: "runtime-cfg".to_string(),
            path: "configs/runtime.json".to_string(),
            format: "json".to_string(),
            applies_to_agents: Vec::new(),
        });

        let supplier =
            MapAssetSupplier::new().with("runtime-cfg", br#"{"engine":"default"}"#.to_vec());

        build_dw_pack_with_supplier(&spec, &out, &supplier).expect("build ok");

        let bytes = std::fs::read(&out).expect("read pack");
        let needle = b"assets/configs/runtime.json";
        assert!(bytes.windows(needle.len()).any(|w| w == needle));
    }

    #[test]
    fn build_propagates_supplier_not_found_error() {
        let dir = tempdir().expect("tempdir");
        let out = dir.path().join("missing.gtpack");

        let mut spec = sample_spec();
        spec.assets.push(DwApplicationPackAsset {
            asset_id: "missing-thing".to_string(),
            path: "assets/missing.bin".to_string(),
            kind: DwApplicationPackAssetKind::Generic,
            content_type: None,
            applies_to_agents: Vec::new(),
            source_ref: None,
        });

        let err = build_dw_pack(&spec, &out).expect_err("must error");
        assert!(matches!(
            err,
            DwPackBuildError::Supplier(AssetSupplierError::NotFound(_))
        ));
    }
}
