use crate::{PackSourceRef, SourceRefError, TemplateMaturity, TemplateSourceRef};
use greentic_cap_types::CapabilityId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use thiserror::Error;

/// Environment suitability marker for provider catalog filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DwProviderEnvironmentSuitability {
    Oss,
    Local,
    Dev,
    Demo,
    Enterprise,
    Prod,
}

/// Pack-oriented source wrapper for provider catalog entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwProviderSourceRef {
    #[serde(flatten)]
    pub source: PackSourceRef,
}

impl DwProviderSourceRef {
    pub fn from_raw(raw_ref: impl Into<String>) -> Result<Self, SourceRefError> {
        Ok(Self {
            source: PackSourceRef::from_raw(raw_ref)?,
        })
    }
}

/// Capability coverage and pack-level capability metadata for a provider.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwProviderCapabilityProfile {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capability_contract_ids: Vec<CapabilityId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pack_capability_ids: Vec<String>,
}

/// Default and recommendation hints for provider selection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwProviderDefaultProfile {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_default_choice: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_recommended_choice: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommended_for_families: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommended_for_templates: Vec<String>,
}

/// Discoverable provider catalog entry for capability-backed DW providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwProviderCatalogEntry {
    pub provider_id: String,
    pub family: String,
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    pub display_name: String,
    pub summary: String,
    pub source_ref: DwProviderSourceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    pub maturity: TemplateMaturity,
    #[serde(default)]
    pub capability_profile: DwProviderCapabilityProfile,
    #[serde(default)]
    pub default_profile: DwProviderDefaultProfile,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub template_compatibility: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_setup_schema_refs: Vec<TemplateSourceRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_question_block_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suitability: Vec<DwProviderEnvironmentSuitability>,
}

/// Stable provider catalog document used for provider discovery and filtering.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwProviderCatalog {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<DwProviderCatalogEntry>,
}

#[derive(Debug, Error)]
pub enum DwProviderCatalogError {
    #[error("failed to read provider catalog from `{path}`: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse provider catalog from `{origin}`: {source}")]
    Parse {
        origin: String,
        source: serde_json::Error,
    },
    #[error("provider `{provider_id}` was not found in the catalog")]
    NotFound { provider_id: String },
}

impl DwProviderCatalog {
    /// Load a provider catalog from a JSON string.
    pub fn from_json_str(source: &str) -> Result<Self, DwProviderCatalogError> {
        serde_json::from_str(source).map_err(|source| DwProviderCatalogError::Parse {
            origin: "inline provider catalog json".to_string(),
            source,
        })
    }

    /// Load a provider catalog from a JSON file on disk.
    pub fn from_json_path(path: impl AsRef<Path>) -> Result<Self, DwProviderCatalogError> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path).map_err(|source| DwProviderCatalogError::Read {
            path: path.display().to_string(),
            source,
        })?;

        serde_json::from_str(&contents).map_err(|source| DwProviderCatalogError::Parse {
            origin: path.display().to_string(),
            source,
        })
    }

    /// Return a provider entry by id.
    pub fn find(&self, provider_id: &str) -> Option<&DwProviderCatalogEntry> {
        self.entries
            .iter()
            .find(|entry| entry.provider_id == provider_id)
    }

    /// List providers by family.
    pub fn list_by_family(&self, family: &str) -> Vec<&DwProviderCatalogEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.family == family)
            .collect()
    }

    /// List providers suitable for a given environment.
    pub fn list_by_suitability(
        &self,
        suitability: DwProviderEnvironmentSuitability,
    ) -> Vec<&DwProviderCatalogEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.suitability.contains(&suitability))
            .collect()
    }

    /// Return providers marked as defaults or recommendations for a family.
    pub fn recommended_for_family(&self, family: &str) -> Vec<&DwProviderCatalogEntry> {
        self.entries
            .iter()
            .filter(|entry| {
                entry.family == family
                    && (entry.default_profile.is_default_choice
                        || entry.default_profile.is_recommended_choice
                        || entry
                            .default_profile
                            .recommended_for_families
                            .iter()
                            .any(|candidate| candidate == family))
            })
            .collect()
    }

    /// Resolve the provider source ref used for pack inclusion.
    pub fn resolve_source_ref(
        &self,
        provider_id: &str,
    ) -> Result<&DwProviderSourceRef, DwProviderCatalogError> {
        self.find(provider_id)
            .map(|entry| &entry.source_ref)
            .ok_or_else(|| DwProviderCatalogError::NotFound {
                provider_id: provider_id.to_string(),
            })
    }
}
