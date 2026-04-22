use crate::{PackSourceRef, TemplateSourceRef};
use greentic_cap_types::CapabilityId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use thiserror::Error;

/// Release maturity of a template descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemplateMaturity {
    Experimental,
    Beta,
    Stable,
    Deprecated,
}

/// Whether a template supports only the default wizard path or both modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemplateModeSuitability {
    DefaultOnly,
    BothModes,
}

/// Display-oriented metadata for a digital worker template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemplateMetadata {
    pub id: String,
    pub name: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub maturity: TemplateMaturity,
}

/// Capability requirements and defaults declared by a template.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemplateCapabilityPlan {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_capabilities: Vec<CapabilityId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub optional_capabilities: Vec<CapabilityId>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub default_provider_ids: BTreeMap<CapabilityId, String>,
}

/// Summary of capability coverage carried by a catalog entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemplateCapabilitySummary {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_capabilities: Vec<CapabilityId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub optional_capabilities: Vec<CapabilityId>,
}

/// Reference to a reusable question block contributed by DW core, templates, or providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemplateQuestionBlockRef {
    pub block_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<TemplateSourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// Default answer values supplied by the template descriptor.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TemplateDefaults {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, serde_json::Value>,
}

/// Behavior shaping for one wizard mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemplateModeBehavior {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub question_block_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_optional_sections: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_provider_overrides: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_packaging_overrides: bool,
}

/// Behavior scaffolding for default and personalised wizard modes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemplateBehaviorScaffold {
    pub default_mode_behavior: TemplateModeBehavior,
    pub personalised_mode_behavior: TemplateModeBehavior,
}

/// Packaging hint for expected agent layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemplateAgentLayoutHint {
    SingleAgent,
    MultiAgentReady,
    MultiAgentRecommended,
}

/// Packaging and downstream materialization hints declared by the template.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemplatePackagingHints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_agent_layout: Option<TemplateAgentLayoutHint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub support_pack_refs: Vec<PackSourceRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_agent_roles: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bundle_notes: Vec<String>,
}

/// Canonical digital worker template descriptor loaded from declarative JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DigitalWorkerTemplate {
    pub metadata: TemplateMetadata,
    pub capability_plan: TemplateCapabilityPlan,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub question_blocks: Vec<TemplateQuestionBlockRef>,
    #[serde(default)]
    pub defaults: TemplateDefaults,
    pub behavior_scaffold: TemplateBehaviorScaffold,
    #[serde(default)]
    pub packaging_hints: TemplatePackagingHints,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub supports_multi_agent_app_pack: bool,
}

#[derive(Debug, Error)]
pub enum TemplateDescriptorError {
    #[error("failed to read template descriptor from `{path}`: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse template descriptor from `{origin}`: {source}")]
    Parse {
        origin: String,
        source: serde_json::Error,
    },
}

impl DigitalWorkerTemplate {
    /// Load a template descriptor from a JSON string.
    pub fn from_json_str(source: &str) -> Result<Self, TemplateDescriptorError> {
        serde_json::from_str(source).map_err(|source| TemplateDescriptorError::Parse {
            origin: "inline template json".to_string(),
            source,
        })
    }

    /// Load a template descriptor from any reader containing JSON.
    pub fn from_json_reader(mut reader: impl Read) -> Result<Self, TemplateDescriptorError> {
        let mut contents = String::new();
        reader
            .read_to_string(&mut contents)
            .map_err(|source| TemplateDescriptorError::Read {
                path: "<reader>".to_string(),
                source,
            })?;
        Self::from_json_str(&contents)
    }

    /// Load a template descriptor from a JSON file on disk.
    pub fn from_json_path(path: impl AsRef<Path>) -> Result<Self, TemplateDescriptorError> {
        let path = path.as_ref();
        let contents =
            fs::read_to_string(path).map_err(|source| TemplateDescriptorError::Read {
                path: path.display().to_string(),
                source,
            })?;

        serde_json::from_str(&contents).map_err(|source| TemplateDescriptorError::Parse {
            origin: path.display().to_string(),
            source,
        })
    }
}
