use crate::{DwProviderSourceRef, PackSourceRef, TemplateCatalogEntry, TemplateSourceRef};
use greentic_cap_types::CapabilityId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// High-level status for a setup requirement captured in composition output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SetupRequirementStatus {
    Required,
    Satisfied,
    Deferred,
}

/// Planned bundle inclusion entry carried by composition before final bundle generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BundleInclusionPlan {
    pub pack_id: String,
    pub source_ref: PackSourceRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applies_to_agents: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

/// Pack dependency referenced by the resolved composition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PackDependencyRef {
    pub pack_id: String,
    pub source_ref: PackSourceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applies_to_agents: Vec<String>,
}

/// Resolved provider binding for an agent capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProviderBinding {
    pub provider_id: String,
    pub source_ref: DwProviderSourceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_category: Option<String>,
}

/// Capability binding selected for a single agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityBinding {
    pub capability_id: CapabilityId,
    pub provider_binding: ProviderBinding,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack_capability_id: Option<String>,
}

/// Resolved behavior configuration for an agent after template and answer application.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BehaviorConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enabled_question_block_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, serde_json::Value>,
}

/// Setup item that must be completed before the composed application can be finalized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SetupRequirement {
    pub requirement_id: String,
    pub status: SetupRequirementStatus,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_schema_ref: Option<TemplateSourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_block_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applies_to_agents: Vec<String>,
}

/// Source provenance captured for a resolved agent or shared pack decision.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CompositionSourceProvenance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_source_ref: Option<TemplateSourceRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_source_refs: Vec<DwProviderSourceRef>,
}

/// Application-level metadata for the resolved composition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwCompositionApplicationMetadata {
    pub application_id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Packaging intent captured before app-pack materialization.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwCompositionOutputPlan {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_pack_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_bundle_id: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub supports_multi_agent_app_pack: bool,
}

/// Resolved composition for a single digital worker agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DwAgentComposition {
    pub agent_id: String,
    pub display_name: String,
    pub template_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_template: Option<TemplateCatalogEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capability_bindings: Vec<CapabilityBinding>,
    #[serde(default)]
    pub behavior_config: BehaviorConfig,
    #[serde(default)]
    pub source_provenance: CompositionSourceProvenance,
}

/// Canonical resolved composition document bridging wizard answers to later materialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DwCompositionDocument {
    pub application: DwCompositionApplicationMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<DwAgentComposition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_pack_dependencies: Vec<PackDependencyRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bundle_plan: Vec<BundleInclusionPlan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved_setup_items: Vec<SetupRequirement>,
    #[serde(default)]
    pub output_plan: DwCompositionOutputPlan,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_provenance: Option<CompositionSourceProvenance>,
}
