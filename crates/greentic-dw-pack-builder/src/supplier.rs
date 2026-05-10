//! Resolve asset bytes for assets declared in [`DwApplicationPackSpec`].
//!
//! `DwApplicationPackSpec` carries asset *descriptors* — id, path, kind,
//! optional source ref — but not byte content. The pack builder needs an
//! [`AssetSupplier`] to fetch bytes for each declared asset. Wizard /
//! deployer pipelines provide this; tests use [`MapAssetSupplier`].
//!
//! # Lookup key
//!
//! Each asset descriptor exposes an `asset_id`. Suppliers should resolve
//! bytes by `asset_id`. The pack builder writes the resolved bytes at the
//! spec-declared `path` inside the archive.

use std::collections::BTreeMap;

use greentic_dw_types::{
    DwApplicationPackAsset, DwGeneratedConfigAsset, DwGeneratedFlowAsset, DwGeneratedPromptAsset,
};
use thiserror::Error;

/// One asset descriptor kind handed to the supplier.
///
/// Borrowed so suppliers can inspect descriptor metadata without cloning.
#[derive(Debug, Clone, Copy)]
pub enum AssetDescriptor<'a> {
    /// Generic static asset (declared in `spec.assets`).
    Asset(&'a DwApplicationPackAsset),
    /// Generated configuration (declared in `spec.generated_configs`).
    Config(&'a DwGeneratedConfigAsset),
    /// Generated flow (declared in `spec.generated_flows`).
    Flow(&'a DwGeneratedFlowAsset),
    /// Generated prompt (declared in `spec.generated_prompts`).
    Prompt(&'a DwGeneratedPromptAsset),
}

impl<'a> AssetDescriptor<'a> {
    /// `asset_id` from the underlying descriptor — the supplier lookup key.
    pub fn asset_id(&self) -> &'a str {
        match self {
            Self::Asset(a) => &a.asset_id,
            Self::Config(a) => &a.asset_id,
            Self::Flow(a) => &a.asset_id,
            Self::Prompt(a) => &a.asset_id,
        }
    }

    /// `path` inside the produced `.gtpack` archive.
    pub fn path(&self) -> &'a str {
        match self {
            Self::Asset(a) => &a.path,
            Self::Config(a) => &a.path,
            Self::Flow(a) => &a.path,
            Self::Prompt(a) => &a.path,
        }
    }
}

/// Errors produced by an [`AssetSupplier::provide`] call.
#[derive(Debug, Error)]
pub enum AssetSupplierError {
    /// No bytes available for the requested `asset_id`.
    #[error("no bytes available for asset_id '{0}'")]
    NotFound(String),

    /// Backing store failed to read (filesystem, network, etc.).
    #[error("supplier I/O error for asset_id '{asset_id}': {source}")]
    Io {
        /// `asset_id` whose lookup failed.
        asset_id: String,
        /// Underlying error from the supplier.
        #[source]
        source: anyhow::Error,
    },
}

/// Resolve asset bytes by descriptor.
///
/// Suppliers are expected to be deterministic for golden tests — same
/// descriptor (by `asset_id`) yields same bytes across calls.
pub trait AssetSupplier {
    /// Resolve bytes for an asset descriptor.
    fn provide(&self, descriptor: AssetDescriptor<'_>) -> Result<Vec<u8>, AssetSupplierError>;
}

/// Supplier that errors on every request.
///
/// Default for [`crate::DwPackBuildOptions`]. Use this when a spec
/// declares no assets, or when caller wants to assert no asset bytes
/// are needed.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoAssetSupplier;

impl AssetSupplier for NoAssetSupplier {
    fn provide(&self, descriptor: AssetDescriptor<'_>) -> Result<Vec<u8>, AssetSupplierError> {
        Err(AssetSupplierError::NotFound(
            descriptor.asset_id().to_string(),
        ))
    }
}

/// In-memory supplier backed by a `BTreeMap<asset_id, bytes>`.
///
/// Use for tests + simple pipelines that resolve all assets upfront.
#[derive(Debug, Default, Clone)]
pub struct MapAssetSupplier {
    /// Asset bytes keyed by `asset_id`.
    pub bytes_by_asset_id: BTreeMap<String, Vec<u8>>,
}

impl MapAssetSupplier {
    /// New empty supplier.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert one asset's bytes.
    #[must_use]
    pub fn with(mut self, asset_id: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        self.bytes_by_asset_id.insert(asset_id.into(), bytes.into());
        self
    }
}

impl AssetSupplier for MapAssetSupplier {
    fn provide(&self, descriptor: AssetDescriptor<'_>) -> Result<Vec<u8>, AssetSupplierError> {
        let id = descriptor.asset_id();
        self.bytes_by_asset_id
            .get(id)
            .cloned()
            .ok_or_else(|| AssetSupplierError::NotFound(id.to_string()))
    }
}

/// Filesystem-rooted supplier that reads asset bytes from disk.
///
/// Mirrors the spec's `path` layout under [`base_dir`]: a descriptor with
/// `path = "assets/icon.bin"` resolves to `<base_dir>/assets/icon.bin`.
/// Returns [`AssetSupplierError::NotFound`] when the file is absent and
/// [`AssetSupplierError::Io`] for other read failures (permission, etc.).
///
/// [`base_dir`]: Self::base_dir
#[derive(Debug, Clone)]
pub struct FsAssetSupplier {
    /// Root directory containing asset files. Reads happen at
    /// `base_dir.join(descriptor.path())`.
    pub base_dir: std::path::PathBuf,
}

impl FsAssetSupplier {
    /// Create a supplier rooted at `base_dir`.
    pub fn new(base_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }
}

impl AssetSupplier for FsAssetSupplier {
    fn provide(&self, descriptor: AssetDescriptor<'_>) -> Result<Vec<u8>, AssetSupplierError> {
        let full_path = self.base_dir.join(descriptor.path());
        std::fs::read(&full_path).map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => {
                AssetSupplierError::NotFound(descriptor.asset_id().to_string())
            }
            _ => AssetSupplierError::Io {
                asset_id: descriptor.asset_id().to_string(),
                source: anyhow::Error::from(err),
            },
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use greentic_dw_types::DwApplicationPackAssetKind;

    fn sample_asset() -> DwApplicationPackAsset {
        DwApplicationPackAsset {
            asset_id: "logo".to_string(),
            path: "assets/logo.png".to_string(),
            kind: DwApplicationPackAssetKind::Generic,
            content_type: None,
            applies_to_agents: Vec::new(),
            source_ref: None,
        }
    }

    #[test]
    fn descriptor_exposes_asset_id_and_path() {
        let asset = sample_asset();
        let d = AssetDescriptor::Asset(&asset);
        assert_eq!(d.asset_id(), "logo");
        assert_eq!(d.path(), "assets/logo.png");
    }

    #[test]
    fn no_supplier_always_errors() {
        let asset = sample_asset();
        let err = NoAssetSupplier
            .provide(AssetDescriptor::Asset(&asset))
            .expect_err("must error");
        match err {
            AssetSupplierError::NotFound(id) => assert_eq!(id, "logo"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn map_supplier_returns_inserted_bytes() {
        let asset = sample_asset();
        let supplier = MapAssetSupplier::new().with("logo", b"PNGBYTES".to_vec());
        let bytes = supplier
            .provide(AssetDescriptor::Asset(&asset))
            .expect("must resolve");
        assert_eq!(bytes, b"PNGBYTES");
    }

    #[test]
    fn map_supplier_errors_on_missing() {
        let asset = sample_asset();
        let supplier = MapAssetSupplier::new();
        assert!(matches!(
            supplier.provide(AssetDescriptor::Asset(&asset)),
            Err(AssetSupplierError::NotFound(_))
        ));
    }

    #[test]
    fn fs_supplier_reads_existing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let asset_dir = dir.path().join("assets");
        std::fs::create_dir(&asset_dir).expect("mkdir");
        std::fs::write(asset_dir.join("logo.png"), b"PNGBYTES").expect("write");

        let asset = sample_asset();
        let supplier = FsAssetSupplier::new(dir.path());
        let bytes = supplier
            .provide(AssetDescriptor::Asset(&asset))
            .expect("must read");
        assert_eq!(bytes, b"PNGBYTES");
    }

    #[test]
    fn fs_supplier_returns_not_found_for_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");

        let asset = sample_asset();
        let supplier = FsAssetSupplier::new(dir.path());
        let err = supplier
            .provide(AssetDescriptor::Asset(&asset))
            .expect_err("must error");

        match err {
            AssetSupplierError::NotFound(id) => assert_eq!(id, "logo"),
            other => panic!("unexpected: {other:?}"),
        }
    }
}
