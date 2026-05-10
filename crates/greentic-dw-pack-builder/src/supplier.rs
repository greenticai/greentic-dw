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
use std::io::Read;

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

/// HTTP-based supplier that fetches asset bytes from a URL endpoint.
///
/// URL construction: `<base_url>/<descriptor.path()>` with leading/trailing
/// slash normalisation. A descriptor with `path = "assets/icon.bin"` and
/// `base_url = "https://store.example.com/packs"` resolves to
/// `https://store.example.com/packs/assets/icon.bin`.
///
/// Error mapping:
/// - HTTP 404 → [`AssetSupplierError::NotFound`]
/// - HTTP 4xx/5xx (other) → [`AssetSupplierError::Io`]
/// - Network failure (connection refused, timeout, TLS error) → [`AssetSupplierError::Io`]
///
/// Sync HTTP via [`ureq`] — fits the sync [`AssetSupplier::provide`]
/// signature without spawning a tokio runtime. Suitable for wizard /
/// deployer pipelines that fetch from a Greentic Store HTTP API or
/// arbitrary HTTP endpoint.
#[derive(Debug, Clone)]
pub struct HttpAssetSupplier {
    base_url: String,
}

impl HttpAssetSupplier {
    /// Create an HTTP supplier rooted at `base_url`.
    ///
    /// Trailing slashes in `base_url` and leading slashes in
    /// `descriptor.path()` are normalised on each fetch.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }

    /// Construct the resolution URL for a descriptor.
    ///
    /// Public for testability — not typically called by consumers.
    pub fn url_for(&self, descriptor: &AssetDescriptor<'_>) -> String {
        let base = self.base_url.trim_end_matches('/');
        let path = descriptor.path().trim_start_matches('/');
        format!("{base}/{path}")
    }
}

impl AssetSupplier for HttpAssetSupplier {
    fn provide(&self, descriptor: AssetDescriptor<'_>) -> Result<Vec<u8>, AssetSupplierError> {
        let url = self.url_for(&descriptor);
        let asset_id = descriptor.asset_id().to_string();

        let response = ureq::get(&url).call().map_err(|e| {
            // Distinguish HTTP 404 from other error classes.
            if let ureq::Error::StatusCode(404) = e {
                AssetSupplierError::NotFound(asset_id.clone())
            } else {
                AssetSupplierError::Io {
                    asset_id: asset_id.clone(),
                    source: anyhow::Error::msg(format!("ureq: {e}")),
                }
            }
        })?;

        let mut bytes = Vec::new();
        response
            .into_body()
            .into_reader()
            .read_to_end(&mut bytes)
            .map_err(|e| AssetSupplierError::Io {
                asset_id: asset_id.clone(),
                source: anyhow::Error::from(e),
            })?;

        Ok(bytes)
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

    #[test]
    fn http_supplier_url_construction_handles_trailing_slash() {
        let s = HttpAssetSupplier::new("https://store.example.com/packs");
        let asset = sample_asset();
        assert_eq!(
            s.url_for(&AssetDescriptor::Asset(&asset)),
            "https://store.example.com/packs/assets/logo.png"
        );

        let s2 = HttpAssetSupplier::new("https://store.example.com/packs/");
        assert_eq!(
            s2.url_for(&AssetDescriptor::Asset(&asset)),
            "https://store.example.com/packs/assets/logo.png"
        );
    }

    #[test]
    fn http_supplier_url_construction_handles_leading_slash_in_path() {
        let asset = DwApplicationPackAsset {
            asset_id: "logo".to_string(),
            path: "/assets/logo.png".to_string(),
            kind: DwApplicationPackAssetKind::Generic,
            content_type: None,
            applies_to_agents: Vec::new(),
            source_ref: None,
        };
        let s = HttpAssetSupplier::new("https://store.example.com/packs");
        assert_eq!(
            s.url_for(&AssetDescriptor::Asset(&asset)),
            "https://store.example.com/packs/assets/logo.png"
        );
    }

    #[test]
    fn http_supplier_returns_io_on_unreachable_endpoint() {
        // Loopback at port 1 (privileged, will be refused). No network needed.
        let supplier = HttpAssetSupplier::new("http://127.0.0.1:1");
        let asset = sample_asset();
        let err = supplier
            .provide(AssetDescriptor::Asset(&asset))
            .expect_err("must error");

        // Connection refused / timeout / etc. should map to Io variant
        // (NotFound is reserved for HTTP 404 responses; unreachable host
        // is not "not found", it's an I/O failure).
        match err {
            AssetSupplierError::Io { asset_id, .. } => assert_eq!(asset_id, "logo"),
            other => panic!("expected Io error, got: {other:?}"),
        }
    }
}
