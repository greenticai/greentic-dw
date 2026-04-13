use crate::{
    CompositionSourceProvenance, DwApplicationPackMaterializationError, DwApplicationPackSpec,
    DwBundlePlan, DwBundlePlanGenerationError, DwCompositionDocument, SetupRequirement,
    SetupRequirementStatus,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Severity level for review-time findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DwReviewWarningLevel {
    Info,
    Warning,
}

/// Review-time warning or unresolved downstream concern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwReviewWarning {
    pub warning_id: String,
    pub level: DwReviewWarningLevel,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applies_to_agents: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement_id: Option<String>,
}

/// Provenance carried with the combined review envelope.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwReviewProvenance {
    pub application_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_pack_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_bundle_id: Option<String>,
    #[serde(default)]
    pub composition_source_provenance: CompositionSourceProvenance,
}

/// Deterministic top-level review artifact for DW design-time outputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DwReviewEnvelope {
    pub composition: DwCompositionDocument,
    pub application_pack_spec: DwApplicationPackSpec,
    pub bundle_plan: DwBundlePlan,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setup_requirements: Vec<SetupRequirement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<DwReviewWarning>,
    pub provenance: DwReviewProvenance,
}

#[derive(Debug, Error)]
pub enum DwReviewEnvelopeGenerationError {
    #[error(transparent)]
    PackMaterialization(#[from] DwApplicationPackMaterializationError),
    #[error(transparent)]
    BundlePlan(#[from] DwBundlePlanGenerationError),
}

impl DwCompositionDocument {
    /// Produce a deterministic review envelope before any downstream apply behavior.
    pub fn to_review_envelope(&self) -> Result<DwReviewEnvelope, DwReviewEnvelopeGenerationError> {
        let application_pack_spec = self.to_application_pack_spec()?;
        let bundle_plan = self.to_bundle_plan()?;
        let warnings = self
            .unresolved_setup_items
            .iter()
            .filter(|item| item.status != SetupRequirementStatus::Satisfied)
            .map(|item| DwReviewWarning {
                warning_id: format!("warning.{}", item.requirement_id),
                level: DwReviewWarningLevel::Warning,
                summary: item.summary.clone(),
                applies_to_agents: item.applies_to_agents.clone(),
                requirement_id: Some(item.requirement_id.clone()),
            })
            .collect();

        Ok(DwReviewEnvelope {
            composition: self.clone(),
            application_pack_spec,
            bundle_plan,
            setup_requirements: self.unresolved_setup_items.clone(),
            warnings,
            provenance: DwReviewProvenance {
                application_id: self.application.application_id.clone(),
                generated_pack_id: self.output_plan.generated_pack_id.clone(),
                generated_bundle_id: self.output_plan.generated_bundle_id.clone(),
                composition_source_provenance: self.source_provenance.clone().unwrap_or_default(),
            },
        })
    }
}
