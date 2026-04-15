use crate::{
    DigitalWorkerTemplate, SourceRefKind, TemplateCapabilitySummary, TemplateDescriptorError,
    TemplateMaturity, TemplateModeSuitability, TemplateSourceRef,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Discoverable catalog entry for a template descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemplateCatalogEntry {
    pub template_id: String,
    pub display_name: String,
    pub summary: String,
    pub source_ref: TemplateSourceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub maturity: TemplateMaturity,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default)]
    pub capability_summary: TemplateCapabilitySummary,
    pub mode_suitability: TemplateModeSuitability,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub supports_multi_agent_app_pack: bool,
}

/// Stable template catalog document used by the wizard for discovery.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemplateCatalog {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<TemplateCatalogEntry>,
    #[serde(skip)]
    #[schemars(skip)]
    base_dir: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum TemplateCatalogError {
    #[error("failed to read template catalog from `{path}`: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse template catalog from `{origin}`: {source}")]
    Parse {
        origin: String,
        source: serde_json::Error,
    },
    #[error("template `{template_id}` was not found in the catalog")]
    NotFound { template_id: String },
    #[error("template source `{raw_ref}` with kind `{kind:?}` cannot be resolved locally")]
    UnsupportedLocalResolution {
        raw_ref: String,
        kind: SourceRefKind,
    },
    #[error(
        "template source `{raw_ref}` resolves outside the catalog directory `{base_dir}` and is rejected"
    )]
    EscapesCatalogRoot { raw_ref: String, base_dir: String },
    #[error(transparent)]
    Descriptor(#[from] TemplateDescriptorError),
}

impl TemplateCatalog {
    /// Load a template catalog from a JSON string.
    pub fn from_json_str(source: &str) -> Result<Self, TemplateCatalogError> {
        serde_json::from_str(source).map_err(|source| TemplateCatalogError::Parse {
            origin: "inline template catalog json".to_string(),
            source,
        })
    }

    /// Load a template catalog from a JSON file on disk.
    pub fn from_json_path(path: impl AsRef<Path>) -> Result<Self, TemplateCatalogError> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path).map_err(|source| TemplateCatalogError::Read {
            path: path.display().to_string(),
            source,
        })?;

        let mut catalog: Self =
            serde_json::from_str(&contents).map_err(|source| TemplateCatalogError::Parse {
                origin: path.display().to_string(),
                source,
            })?;
        catalog.base_dir = path.parent().and_then(|parent| parent.canonicalize().ok());
        catalog.rebase_local_refs(path.parent());
        Ok(catalog)
    }

    /// Return a catalog entry by template id.
    pub fn find(&self, template_id: &str) -> Option<&TemplateCatalogEntry> {
        self.entries
            .iter()
            .find(|entry| entry.template_id == template_id)
    }

    /// Resolve a template descriptor for local/dev sources.
    pub fn resolve_template(
        &self,
        template_id: &str,
    ) -> Result<DigitalWorkerTemplate, TemplateCatalogError> {
        let entry = self
            .find(template_id)
            .ok_or_else(|| TemplateCatalogError::NotFound {
                template_id: template_id.to_string(),
            })?;

        match entry.source_ref.source.kind {
            SourceRefKind::LocalPath | SourceRefKind::DevPath => {
                let resolved_path =
                    self.resolve_local_ref_within_catalog(&entry.source_ref.source.raw_ref)?;
                DigitalWorkerTemplate::from_json_path(&resolved_path)
                    .map_err(TemplateCatalogError::from)
            }
            kind => Err(TemplateCatalogError::UnsupportedLocalResolution {
                raw_ref: entry.source_ref.source.raw_ref.clone(),
                kind,
            }),
        }
    }
}

impl DigitalWorkerTemplate {
    /// Build a catalog entry from a template descriptor and its source ref.
    pub fn to_catalog_entry(
        &self,
        source_ref: TemplateSourceRef,
        version: Option<String>,
        mode_suitability: TemplateModeSuitability,
    ) -> TemplateCatalogEntry {
        TemplateCatalogEntry {
            template_id: self.metadata.id.clone(),
            display_name: self.metadata.name.clone(),
            summary: self.metadata.summary.clone(),
            source_ref,
            version,
            maturity: self.metadata.maturity,
            tags: self.metadata.tags.clone(),
            capability_summary: TemplateCapabilitySummary {
                required_capabilities: self.capability_plan.required_capabilities.clone(),
                optional_capabilities: self.capability_plan.optional_capabilities.clone(),
            },
            mode_suitability,
            supports_multi_agent_app_pack: self.supports_multi_agent_app_pack,
        }
    }
}

impl TemplateCatalog {
    fn resolve_local_ref_within_catalog(
        &self,
        raw_ref: &str,
    ) -> Result<PathBuf, TemplateCatalogError> {
        let candidate = self.resolve_local_ref_path(raw_ref);

        let Some(base_dir) = &self.base_dir else {
            return Ok(candidate);
        };

        let checked_target = match candidate.canonicalize() {
            Ok(path) => path,
            Err(_) => {
                let parent = candidate.parent().unwrap_or(base_dir.as_path());
                let normalized_parent = match parent.canonicalize() {
                    Ok(path) => path,
                    Err(_) => self.normalize_path(parent),
                };
                match candidate.file_name() {
                    Some(name) => normalized_parent.join(name),
                    None => normalized_parent,
                }
            }
        };

        if checked_target.starts_with(base_dir) {
            Ok(candidate)
        } else {
            Err(TemplateCatalogError::EscapesCatalogRoot {
                raw_ref: raw_ref.to_string(),
                base_dir: base_dir.display().to_string(),
            })
        }
    }

    fn resolve_local_ref_path(&self, raw_ref: &str) -> PathBuf {
        let raw_path = PathBuf::from(raw_ref);
        let resolved = if raw_path.is_absolute() {
            raw_path
        } else if let Some(base_dir) = &self.base_dir {
            base_dir.join(raw_path)
        } else {
            raw_path
        };

        self.normalize_path(&resolved)
    }

    fn normalize_path(&self, path: &Path) -> PathBuf {
        let mut normalized = PathBuf::new();

        for component in path.components() {
            use std::path::Component;

            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    normalized.pop();
                }
                other => normalized.push(other.as_os_str()),
            }
        }

        normalized
    }

    fn rebase_local_refs(&mut self, base_dir: Option<&Path>) {
        let Some(base_dir) = base_dir else {
            return;
        };

        for entry in &mut self.entries {
            if matches!(
                entry.source_ref.source.kind,
                SourceRefKind::LocalPath | SourceRefKind::DevPath
            ) {
                let raw_ref = PathBuf::from(&entry.source_ref.source.raw_ref);
                if raw_ref.is_relative() {
                    entry.source_ref.source.raw_ref = base_dir.join(raw_ref).display().to_string();
                }
            }
        }
    }
}
