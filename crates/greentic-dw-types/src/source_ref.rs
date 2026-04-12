use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Canonical source kind used for template and pack references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceRefKind {
    Oci,
    Store,
    Repo,
    LocalPath,
    DevPath,
}

/// Transport and resolution hints carried alongside a source reference.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SourceTransportHints {
    /// Prefer offline resolution when the caller supports cached artifacts.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub offline: bool,
    /// Allow local HTTP endpoints for dev-only source flows.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_insecure_local_http: bool,
}

/// Shared canonical source reference for packs, templates, and later catalog entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SourceRef {
    /// The user-facing source ref, such as `oci://...` or `repo://...`.
    pub raw_ref: String,
    /// Parsed or declared source kind.
    pub kind: SourceRefKind,
    /// Optional transport hints reused by downstream resolution layers.
    #[serde(default)]
    pub transport_hints: SourceTransportHints,
    /// Marks a source as local-development only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dev_mode: bool,
}

/// Pack-oriented wrapper around the canonical source reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PackSourceRef {
    #[serde(flatten)]
    pub source: SourceRef,
}

/// Template-oriented wrapper around the canonical source reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemplateSourceRef {
    #[serde(flatten)]
    pub source: SourceRef,
}

/// Policy hints used when resolving source refs into concrete artifacts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SourceResolutionPolicy {
    /// Reject mutable tags when the caller requires a pinned source.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub require_immutable_refs: bool,
    /// Allow `dev://` and filesystem-backed refs.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_dev_sources: bool,
    /// Prefer cached or previously pinned resolutions.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub prefer_offline: bool,
    /// Optional registry base used when mapping `repo://...` refs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_registry_base: Option<String>,
    /// Optional registry base used when mapping `store://...` refs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_registry_base: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SourceRefError {
    #[error("source ref must not be empty")]
    Empty,
    #[error("source ref `{raw_ref}` does not match declared kind `{kind:?}`")]
    KindMismatch {
        raw_ref: String,
        kind: SourceRefKind,
    },
    #[error("source ref `{raw_ref}` uses an unsupported scheme")]
    UnsupportedScheme { raw_ref: String },
}

impl SourceRef {
    /// Build a source ref by inferring the kind from the raw reference string.
    pub fn from_raw(raw_ref: impl Into<String>) -> Result<Self, SourceRefError> {
        let raw_ref = raw_ref.into();
        let kind = Self::infer_kind(&raw_ref)?;
        Self::new(raw_ref, kind)
    }

    /// Build a source ref from an explicit kind and raw reference.
    pub fn new(raw_ref: impl Into<String>, kind: SourceRefKind) -> Result<Self, SourceRefError> {
        let raw_ref = raw_ref.into();
        if raw_ref.trim().is_empty() {
            return Err(SourceRefError::Empty);
        }

        let inferred = Self::infer_kind(&raw_ref)?;
        if inferred != kind {
            return Err(SourceRefError::KindMismatch { raw_ref, kind });
        }

        Ok(Self {
            dev_mode: matches!(kind, SourceRefKind::DevPath),
            raw_ref,
            kind,
            transport_hints: SourceTransportHints::default(),
        })
    }

    /// Infer the source kind from a raw source ref string.
    pub fn infer_kind(raw_ref: &str) -> Result<SourceRefKind, SourceRefError> {
        let trimmed = raw_ref.trim();
        if trimmed.is_empty() {
            return Err(SourceRefError::Empty);
        }

        if trimmed.starts_with("oci://") {
            return Ok(SourceRefKind::Oci);
        }
        if trimmed.starts_with("store://") {
            return Ok(SourceRefKind::Store);
        }
        if trimmed.starts_with("repo://") {
            return Ok(SourceRefKind::Repo);
        }
        if trimmed.starts_with("dev://") {
            return Ok(SourceRefKind::DevPath);
        }
        if trimmed.contains("://") {
            return Err(SourceRefError::UnsupportedScheme {
                raw_ref: trimmed.to_string(),
            });
        }

        Ok(SourceRefKind::LocalPath)
    }
}

impl PackSourceRef {
    pub fn from_raw(raw_ref: impl Into<String>) -> Result<Self, SourceRefError> {
        Ok(Self {
            source: SourceRef::from_raw(raw_ref)?,
        })
    }
}

impl TemplateSourceRef {
    pub fn from_raw(raw_ref: impl Into<String>) -> Result<Self, SourceRefError> {
        Ok(Self {
            source: SourceRef::from_raw(raw_ref)?,
        })
    }
}
