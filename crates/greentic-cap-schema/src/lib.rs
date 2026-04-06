//! Schema and validation helpers for the Greentic capability model.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use greentic_cap_resolver::CapabilityResolutionReport;
use schemars::JsonSchema;
use schemars::schema::RootSchema;
use serde::{Deserialize, Serialize};

pub use greentic_cap_types::{
    CapabilityBinding, CapabilityBindingKind, CapabilityComponentDescriptor,
    CapabilityComponentOperation, CapabilityConsume, CapabilityConsumeMode, CapabilityDeclaration,
    CapabilityId, CapabilityIdError, CapabilityMetadata, CapabilityOffer, CapabilityProfile,
    CapabilityProviderOperationMap, CapabilityProviderRef, CapabilityRequirement,
    CapabilityResolution, CapabilityValidationError,
};

/// Errors produced by the schema helper layer.
#[derive(Debug, thiserror::Error)]
pub enum CapabilitySchemaError {
    /// CBOR serialization failed.
    #[error("capability CBOR encode failed: {0}")]
    Encode(String),
    /// CBOR deserialization failed.
    #[error("capability CBOR decode failed: {0}")]
    Decode(String),
    /// Validation failed.
    #[error(transparent)]
    Validation(#[from] CapabilityValidationError),
    /// Compatibility validation failed.
    #[error("capability compatibility failed: {0}")]
    Compatibility(String),
}

/// Versioned pack capability section payload.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct PackCapabilitySectionV1 {
    /// Schema version for the section.
    pub schema_version: u32,
    /// Canonical capability declaration payload.
    pub declaration: CapabilityDeclaration,
}

impl PackCapabilitySectionV1 {
    /// Creates a new versioned pack capability section.
    pub fn new(declaration: CapabilityDeclaration) -> Self {
        Self {
            schema_version: 1,
            declaration,
        }
    }

    /// Validates the section and wrapped declaration.
    pub fn validate(&self) -> Result<(), CapabilitySchemaError> {
        if self.schema_version != 1 {
            return Err(CapabilitySchemaError::Compatibility(format!(
                "unsupported pack capability schema_version {}",
                self.schema_version
            )));
        }
        self.declaration.validate()?;
        Ok(())
    }
}

/// Component operation exposed in a compatibility check.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityComponentOperationSpec {
    /// Operation name.
    pub name: String,
    /// Input schema for the operation.
    pub input_schema: serde_json::Value,
    /// Output schema for the operation.
    pub output_schema: serde_json::Value,
}

/// Compatibility report for a single offered capability.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityCompatibilityReport {
    /// Offer identifier.
    pub offer_id: String,
    /// Component identifier or ref being checked.
    pub component_ref: String,
    /// Whether the offer can be satisfied by the component.
    pub compatible: bool,
    /// Detailed issues encountered during validation.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub issues: Vec<CapabilityCompatibilityIssue>,
}

/// Detailed compatibility issue.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "snake_case"))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub enum CapabilityCompatibilityIssue {
    /// The provider points at a different component than the self-description.
    ComponentMismatch {
        /// Expected component identifier.
        expected: String,
        /// Actual component identifier.
        found: String,
    },
    /// The component does not advertise the logical capability.
    MissingCapability {
        /// Capability identifier.
        capability: String,
    },
    /// The mapped operation is missing from the component description.
    MissingOperation {
        /// Logical contract operation.
        contract_operation: String,
        /// Concrete component operation.
        component_operation: String,
    },
    /// The mapped operation input schema does not match.
    InputSchemaMismatch {
        /// Logical contract operation.
        contract_operation: String,
        /// Concrete component operation.
        component_operation: String,
    },
    /// The mapped operation output schema does not match.
    OutputSchemaMismatch {
        /// Logical contract operation.
        contract_operation: String,
        /// Concrete component operation.
        component_operation: String,
    },
    /// The provider did not declare an operation map and the legacy operation was missing.
    MissingProviderOperation {
        /// Provider operation name.
        operation: String,
    },
    /// The offer did not declare a provider.
    MissingProvider,
}

/// Reference policy emitted for bundle/setup consumers.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityResolvedReferencePolicyV1 {
    /// Reference name.
    pub reference: String,
    /// Policy string applied to the reference.
    pub policy: String,
}

/// Machine-readable binding emitted to bundle/setup consumers.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityBindingEmissionV1 {
    /// Request identifier.
    pub request_id: String,
    /// Offer identifier selected by the resolver.
    pub offer_id: String,
    /// Capability identifier.
    pub capability: CapabilityId,
    /// Binding kind.
    pub kind: CapabilityBindingKind,
    /// Profile identifier, if any.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub profile: Option<String>,
    /// Provider reference, if any.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub provider: Option<CapabilityProviderRef>,
}

/// Input parameters for creating a resolved target summary.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityResolvedTargetInputV1 {
    /// Filesystem path for the resolved manifest.
    pub path: String,
    /// Tenant name.
    pub tenant: String,
    /// Optional team name.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub team: Option<String>,
    /// Default access policy.
    pub default_policy: String,
    /// Tenant gmap path.
    pub tenant_gmap: String,
    /// Optional team gmap path.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub team_gmap: Option<String>,
    /// App-pack policies.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub app_pack_policies: Vec<CapabilityResolvedReferencePolicyV1>,
}

/// Input parameters for building a bundle/setup resolution artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityBundleResolutionInputV1 {
    /// Bundle identifier.
    pub bundle_id: String,
    /// Bundle name.
    pub bundle_name: String,
    /// Requested mode.
    pub requested_mode: String,
    /// Locale used for generated text.
    pub locale: String,
    /// Artifact extension used by downstream bundle tooling.
    pub artifact_extension: String,
    /// Root resolution report.
    pub root_resolution: CapabilityResolutionReport,
    /// Per-profile resolution reports.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "BTreeMap::is_empty")
    )]
    pub profile_resolutions: BTreeMap<String, CapabilityResolutionReport>,
    /// Resolved target summaries.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub resolved_targets: Vec<CapabilityResolvedTargetV1>,
    /// Compatibility reports to include in the artifact.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub compatibility_reports: Vec<CapabilityCompatibilityReport>,
}

/// Bundle target summary used by bundle/setup tooling.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityResolvedTargetV1 {
    /// Filesystem path for the resolved manifest.
    pub path: String,
    /// Tenant name.
    pub tenant: String,
    /// Optional team name.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub team: Option<String>,
    /// Default access policy.
    pub default_policy: String,
    /// Tenant gmap path.
    pub tenant_gmap: String,
    /// Optional team gmap path.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub team_gmap: Option<String>,
    /// App-pack policies.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub app_pack_policies: Vec<CapabilityResolvedReferencePolicyV1>,
    /// Emitted bindings for the target.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub bindings: Vec<CapabilityBindingEmissionV1>,
}

/// Machine-readable bundle/setup resolution artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityBundleResolutionV1 {
    /// Schema version for the artifact.
    pub schema_version: u32,
    /// Bundle identifier.
    pub bundle_id: String,
    /// Bundle name.
    pub bundle_name: String,
    /// Requested mode.
    pub requested_mode: String,
    /// Locale used for generated text.
    pub locale: String,
    /// Artifact extension used by downstream bundle tooling.
    pub artifact_extension: String,
    /// Generated resolved manifest files.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub generated_resolved_files: Vec<String>,
    /// Generated setup files.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub generated_setup_files: Vec<String>,
    /// App packs consumed by the bundle.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub app_packs: Vec<String>,
    /// Extension providers consumed by the bundle.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub extension_providers: Vec<String>,
    /// Catalog references consumed by the bundle.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub catalogs: Vec<String>,
    /// Hook identifiers.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub hooks: Vec<String>,
    /// Subscription identifiers.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub subscriptions: Vec<String>,
    /// Capability identifiers.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub capabilities: Vec<String>,
    /// Root resolution report.
    pub root_resolution: CapabilityResolutionReport,
    /// Named profile resolution reports.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "BTreeMap::is_empty")
    )]
    pub profile_resolutions: BTreeMap<String, CapabilityResolutionReport>,
    /// Resolved targets.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub resolved_targets: Vec<CapabilityResolvedTargetV1>,
    /// Compatibility reports for pack/component matching.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub compatibility_reports: Vec<CapabilityCompatibilityReport>,
}

/// Returns the JSON schema for capability declarations.
pub fn capability_declaration_schema() -> RootSchema {
    schemars::schema_for!(CapabilityDeclaration)
}

/// Returns the JSON schema for capability profiles.
pub fn capability_profile_schema() -> RootSchema {
    schemars::schema_for!(CapabilityProfile)
}

/// Returns the JSON schema for capability bindings.
pub fn capability_binding_schema() -> RootSchema {
    schemars::schema_for!(CapabilityBinding)
}

/// Returns the JSON schema for capability resolutions.
pub fn capability_resolution_schema() -> RootSchema {
    schemars::schema_for!(CapabilityResolution)
}

/// Returns the JSON schema for the versioned pack capability section.
pub fn pack_capability_section_schema() -> RootSchema {
    schemars::schema_for!(PackCapabilitySectionV1)
}

/// Serializes a declaration to CBOR after validating it.
pub fn encode_capability_declaration_to_cbor(
    declaration: &CapabilityDeclaration,
) -> Result<Vec<u8>, CapabilitySchemaError> {
    declaration.validate()?;
    serde_cbor::to_vec(declaration).map_err(|err| CapabilitySchemaError::Encode(err.to_string()))
}

/// Deserializes a declaration from CBOR and validates it.
pub fn decode_capability_declaration_from_cbor(
    bytes: &[u8],
) -> Result<CapabilityDeclaration, CapabilitySchemaError> {
    let declaration: CapabilityDeclaration = serde_cbor::from_slice(bytes)
        .map_err(|err| CapabilitySchemaError::Decode(err.to_string()))?;
    declaration.validate()?;
    Ok(declaration)
}

/// Serializes a profile to CBOR after validating the containing declaration.
pub fn encode_capability_profile_to_cbor(
    profile: &CapabilityProfile,
) -> Result<Vec<u8>, CapabilitySchemaError> {
    let declaration = CapabilityDeclaration {
        offers: Vec::new(),
        requires: profile.requires.clone(),
        consumes: profile.consumes.clone(),
        profiles: vec![profile.clone()],
    };
    declaration.validate()?;
    serde_cbor::to_vec(profile).map_err(|err| CapabilitySchemaError::Encode(err.to_string()))
}

/// Serializes a resolution to CBOR after validation.
pub fn encode_capability_resolution_to_cbor(
    resolution: &CapabilityResolution,
) -> Result<Vec<u8>, CapabilitySchemaError> {
    resolution.validate()?;
    serde_cbor::to_vec(resolution).map_err(|err| CapabilitySchemaError::Encode(err.to_string()))
}

/// Deserializes a resolution from CBOR and validates it.
pub fn decode_capability_resolution_from_cbor(
    bytes: &[u8],
) -> Result<CapabilityResolution, CapabilitySchemaError> {
    let resolution: CapabilityResolution = serde_cbor::from_slice(bytes)
        .map_err(|err| CapabilitySchemaError::Decode(err.to_string()))?;
    resolution.validate()?;
    Ok(resolution)
}

/// Serializes a binding to CBOR after validation.
pub fn encode_capability_binding_to_cbor(
    binding: &CapabilityBinding,
) -> Result<Vec<u8>, CapabilitySchemaError> {
    binding.validate()?;
    serde_cbor::to_vec(binding).map_err(|err| CapabilitySchemaError::Encode(err.to_string()))
}

/// Serializes a versioned pack capability section to CBOR after validation.
pub fn encode_pack_capability_section_to_cbor(
    section: &PackCapabilitySectionV1,
) -> Result<Vec<u8>, CapabilitySchemaError> {
    section.validate()?;
    serde_cbor::to_vec(section).map_err(|err| CapabilitySchemaError::Encode(err.to_string()))
}

/// Deserializes a versioned pack capability section from CBOR and validates it.
pub fn decode_pack_capability_section_from_cbor(
    bytes: &[u8],
) -> Result<PackCapabilitySectionV1, CapabilitySchemaError> {
    let section: PackCapabilitySectionV1 = serde_cbor::from_slice(bytes)
        .map_err(|err| CapabilitySchemaError::Decode(err.to_string()))?;
    section.validate()?;
    Ok(section)
}

/// Checks an offer against a component self-description.
pub fn check_offer_component_compatibility(
    offer: &CapabilityOffer,
    component: &CapabilityComponentDescriptor,
) -> Result<CapabilityCompatibilityReport, CapabilitySchemaError> {
    check_offer_component_compatibility_inner(offer, component, true)
}

fn check_offer_component_compatibility_inner(
    offer: &CapabilityOffer,
    component: &CapabilityComponentDescriptor,
    validate_component: bool,
) -> Result<CapabilityCompatibilityReport, CapabilitySchemaError> {
    if validate_component {
        validate_component_descriptor(component)?;
    }

    let mut issues = Vec::new();
    match offer.provider.as_ref() {
        Some(provider) => {
            if provider.component_ref != component.component_ref {
                issues.push(CapabilityCompatibilityIssue::ComponentMismatch {
                    expected: provider.component_ref.clone(),
                    found: component.component_ref.clone(),
                });
            }

            if !component
                .capabilities
                .iter()
                .any(|capability| capability == &offer.capability)
            {
                issues.push(CapabilityCompatibilityIssue::MissingCapability {
                    capability: offer.capability.as_str().to_string(),
                });
            }

            if provider.operation_map.is_empty() {
                if component
                    .operations
                    .iter()
                    .all(|candidate| candidate.name != provider.operation)
                {
                    issues.push(CapabilityCompatibilityIssue::MissingProviderOperation {
                        operation: provider.operation.clone(),
                    });
                }
            } else {
                for entry in &provider.operation_map {
                    let Some(operation) = component
                        .operations
                        .iter()
                        .find(|candidate| candidate.name == entry.component_operation)
                    else {
                        issues.push(CapabilityCompatibilityIssue::MissingOperation {
                            contract_operation: entry.contract_operation.clone(),
                            component_operation: entry.component_operation.clone(),
                        });
                        continue;
                    };

                    if operation.input_schema != entry.input_schema {
                        issues.push(CapabilityCompatibilityIssue::InputSchemaMismatch {
                            contract_operation: entry.contract_operation.clone(),
                            component_operation: entry.component_operation.clone(),
                        });
                    }
                    if operation.output_schema != entry.output_schema {
                        issues.push(CapabilityCompatibilityIssue::OutputSchemaMismatch {
                            contract_operation: entry.contract_operation.clone(),
                            component_operation: entry.component_operation.clone(),
                        });
                    }
                }
            }
        }
        None => issues.push(CapabilityCompatibilityIssue::MissingProvider),
    }

    Ok(CapabilityCompatibilityReport {
        offer_id: offer.id.clone(),
        component_ref: component.component_ref.clone(),
        compatible: issues.is_empty(),
        issues,
    })
}

/// Checks a full pack capability section against a component self-description.
pub fn check_pack_capability_compatibility(
    section: &PackCapabilitySectionV1,
    component: &CapabilityComponentDescriptor,
) -> Result<Vec<CapabilityCompatibilityReport>, CapabilitySchemaError> {
    section.validate()?;
    validate_component_descriptor(component)?;

    let mut reports = Vec::with_capacity(section.declaration.offers.len());
    for offer in &section.declaration.offers {
        reports.push(check_offer_component_compatibility_inner(
            offer, component, false,
        )?);
    }
    Ok(reports)
}

/// Converts a resolver binding into a machine-readable emitted binding.
pub fn binding_emission_from_binding(binding: &CapabilityBinding) -> CapabilityBindingEmissionV1 {
    CapabilityBindingEmissionV1 {
        request_id: binding.request_id.clone(),
        offer_id: binding.offer_id.clone(),
        capability: binding.capability.clone(),
        kind: binding.kind,
        profile: binding.profile.clone(),
        provider: binding.provider.clone(),
    }
}

/// Converts a resolver report into a target summary for bundle/setup consumption.
pub fn target_from_resolution_report(
    input: CapabilityResolvedTargetInputV1,
    report: &CapabilityResolutionReport,
) -> CapabilityResolvedTargetV1 {
    CapabilityResolvedTargetV1 {
        path: input.path,
        tenant: input.tenant,
        team: input.team,
        default_policy: input.default_policy,
        tenant_gmap: input.tenant_gmap,
        team_gmap: input.team_gmap,
        app_pack_policies: input.app_pack_policies,
        bindings: report
            .resolution
            .bindings
            .iter()
            .map(binding_emission_from_binding)
            .collect(),
    }
}

/// Builds a bundle/setup artifact from resolution reports and bundle metadata.
pub fn build_bundle_resolution_artifact(
    input: CapabilityBundleResolutionInputV1,
) -> CapabilityBundleResolutionV1 {
    CapabilityBundleResolutionV1 {
        schema_version: 1,
        bundle_id: input.bundle_id,
        bundle_name: input.bundle_name,
        requested_mode: input.requested_mode,
        locale: input.locale,
        artifact_extension: input.artifact_extension,
        generated_resolved_files: input
            .resolved_targets
            .iter()
            .map(|target| target.path.clone())
            .collect(),
        generated_setup_files: Vec::new(),
        app_packs: Vec::new(),
        extension_providers: Vec::new(),
        catalogs: Vec::new(),
        hooks: Vec::new(),
        subscriptions: Vec::new(),
        capabilities: Vec::new(),
        root_resolution: input.root_resolution,
        profile_resolutions: input.profile_resolutions,
        resolved_targets: input.resolved_targets,
        compatibility_reports: input.compatibility_reports,
    }
}

/// Returns a resolved target summary with an appended app-pack policy list.
pub fn target_with_reference_policies(
    mut target: CapabilityResolvedTargetV1,
    app_pack_policies: Vec<CapabilityResolvedReferencePolicyV1>,
) -> CapabilityResolvedTargetV1 {
    target.app_pack_policies = app_pack_policies;
    target
}

/// Convenience helper for constructing a resolved reference policy entry.
pub fn resolved_reference_policy(
    reference: impl Into<String>,
    policy: impl Into<String>,
) -> CapabilityResolvedReferencePolicyV1 {
    CapabilityResolvedReferencePolicyV1 {
        reference: reference.into(),
        policy: policy.into(),
    }
}

/// Returns all bindings emitted for a resolution report.
pub fn emitted_bindings(report: &CapabilityResolutionReport) -> Vec<CapabilityBindingEmissionV1> {
    report
        .resolution
        .bindings
        .iter()
        .map(binding_emission_from_binding)
        .collect()
}

/// Returns the total number of bindings emitted across all reports.
pub fn emitted_binding_count(reports: &[CapabilityResolutionReport]) -> usize {
    reports
        .iter()
        .map(|report| report.resolution.bindings.len())
        .sum()
}

fn validate_component_descriptor(
    component: &CapabilityComponentDescriptor,
) -> Result<(), CapabilitySchemaError> {
    if component.component_ref.trim().is_empty() {
        return Err(CapabilitySchemaError::Compatibility(
            "component_ref must not be empty".to_string(),
        ));
    }
    if component.version.trim().is_empty() {
        return Err(CapabilitySchemaError::Compatibility(
            "version must not be empty".to_string(),
        ));
    }

    let mut seen_operations = BTreeSet::new();
    for operation in &component.operations {
        if operation.name.trim().is_empty() {
            return Err(CapabilitySchemaError::Compatibility(
                "component operations must have a name".to_string(),
            ));
        }
        if !seen_operations.insert(operation.name.clone()) {
            return Err(CapabilitySchemaError::Compatibility(format!(
                "duplicate component operation {}",
                operation.name
            )));
        }
    }

    let mut seen_capabilities = BTreeSet::new();
    for capability in &component.capabilities {
        if !seen_capabilities.insert(capability.as_str().to_string()) {
            return Err(CapabilitySchemaError::Compatibility(format!(
                "duplicate component capability {}",
                capability
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_cap_types::{
        CapabilityComponentDescriptor, CapabilityComponentOperation, CapabilityConsume,
        CapabilityOffer, CapabilityProviderOperationMap, CapabilityProviderRef,
    };
    use std::collections::BTreeMap;

    fn cap(value: &str) -> CapabilityId {
        CapabilityId::new(value).expect("valid capability id")
    }

    fn declaration() -> CapabilityDeclaration {
        let mut declaration = CapabilityDeclaration::new();

        let mut offer = CapabilityOffer::new("offer.memory", cap("cap://memory.short-term"));
        offer.profiles.push("memory-default".to_string());
        offer.provider = Some(CapabilityProviderRef {
            component_ref: "component:redis".to_string(),
            operation: "provide".to_string(),
            operation_map: Vec::new(),
        });
        declaration.offers.push(offer);

        let mut requirement =
            CapabilityRequirement::new("require.memory", cap("cap://memory.short-term"));
        requirement.profiles.push("memory-default".to_string());
        declaration.requires.push(requirement);

        let mut consume = CapabilityConsume::new("consume.memory", cap("cap://memory.short-term"));
        consume.profiles.push("memory-default".to_string());
        declaration.consumes.push(consume);

        declaration.profiles.push(CapabilityProfile {
            id: "memory-default".to_string(),
            description: Some("default memory profile".to_string()),
            requires: vec![],
            consumes: vec![],
        });

        declaration
    }

    fn component() -> CapabilityComponentDescriptor {
        CapabilityComponentDescriptor {
            component_ref: "component:redis".to_string(),
            version: "1.0.0".to_string(),
            operations: vec![CapabilityComponentOperation {
                name: "provide".to_string(),
                input_schema: serde_json::json!({"type": "string"}),
                output_schema: serde_json::json!({"type": "string"}),
            }],
            capabilities: vec![CapabilityId::new("cap://memory.short-term").expect("cap")],
            metadata: CapabilityMetadata::new(),
        }
    }

    fn provider_offer() -> CapabilityOffer {
        let mut offer = CapabilityOffer::new("offer.memory", cap("cap://memory.short-term"));
        offer.profiles.push("memory-default".to_string());
        offer.provider = Some(CapabilityProviderRef {
            component_ref: "component:redis".to_string(),
            operation: "provide".to_string(),
            operation_map: vec![CapabilityProviderOperationMap {
                contract_operation: "read".to_string(),
                component_operation: "provide".to_string(),
                input_schema: serde_json::json!({"type": "string"}),
                output_schema: serde_json::json!({"type": "string"}),
            }],
        });
        offer
    }

    fn declared_section() -> PackCapabilitySectionV1 {
        let mut declaration = declaration();
        declaration.offers[0] = provider_offer();
        PackCapabilitySectionV1::new(declaration)
    }

    #[test]
    fn schema_generation_includes_expected_definitions() {
        let schema = capability_declaration_schema();
        let json = serde_json::to_value(&schema).expect("schema to json");
        assert!(json.get("definitions").is_some() || json.get("$defs").is_some());
    }

    #[test]
    fn declaration_cbor_round_trips() {
        let declaration = declaration();
        let cbor = encode_capability_declaration_to_cbor(&declaration).expect("encode");
        let decoded = decode_capability_declaration_from_cbor(&cbor).expect("decode");
        assert_eq!(declaration, decoded);
    }

    #[test]
    fn resolution_and_binding_schemas_exist() {
        let resolution_schema = capability_resolution_schema();
        let binding_schema = capability_binding_schema();
        let resolution_json = serde_json::to_value(&resolution_schema).expect("resolution json");
        let binding_json = serde_json::to_value(&binding_schema).expect("binding json");
        assert!(resolution_json.is_object());
        assert!(binding_json.is_object());
    }

    #[test]
    fn pack_section_round_trips_and_compares_against_component() {
        let mut declaration = declaration();
        declaration.offers[0].provider = Some(CapabilityProviderRef {
            component_ref: "component:redis".to_string(),
            operation: "provide".to_string(),
            operation_map: vec![CapabilityProviderOperationMap {
                contract_operation: "read".to_string(),
                component_operation: "provide".to_string(),
                input_schema: serde_json::json!({"type": "string"}),
                output_schema: serde_json::json!({"type": "string"}),
            }],
        });
        let section = PackCapabilitySectionV1::new(declaration);
        let cbor = encode_pack_capability_section_to_cbor(&section).expect("encode");
        let decoded = decode_pack_capability_section_from_cbor(&cbor).expect("decode");
        assert_eq!(section, decoded);

        let reports = check_pack_capability_compatibility(&decoded, &component()).expect("compat");
        assert!(reports.iter().all(|report| report.compatible));
    }

    #[test]
    fn bundle_resolution_artifact_emits_bindings_and_target_metadata() {
        let declaration = declared_section().declaration.clone();
        let section = PackCapabilitySectionV1::new(declaration.clone());
        let compatibility_reports =
            check_pack_capability_compatibility(&section, &component()).expect("compat");
        let root_resolution = greentic_cap_resolver::resolve_root(&declaration).expect("resolve");
        let mut profile_resolutions = BTreeMap::new();
        profile_resolutions.insert(
            "memory-default".to_string(),
            greentic_cap_resolver::resolve_profile(&declaration, "memory-default")
                .expect("profile"),
        );

        let target = target_from_resolution_report(
            CapabilityResolvedTargetInputV1 {
                path: "resolved/default.yaml".to_string(),
                tenant: "default".to_string(),
                team: Some("default".to_string()),
                default_policy: "forbidden".to_string(),
                tenant_gmap: "tenants/default/tenant.gmap".to_string(),
                team_gmap: Some("tenants/default/teams/default/team.gmap".to_string()),
                app_pack_policies: vec![resolved_reference_policy("app-pack", "forbidden")],
            },
            &root_resolution,
        );

        assert_eq!(
            target.bindings.len(),
            root_resolution.resolution.bindings.len()
        );
        assert_eq!(
            emitted_binding_count(std::slice::from_ref(&root_resolution)),
            target.bindings.len()
        );
        assert_eq!(target.app_pack_policies[0].reference, "app-pack");

        let artifact = build_bundle_resolution_artifact(CapabilityBundleResolutionInputV1 {
            bundle_id: "bundle.demo".to_string(),
            bundle_name: "Demo Bundle".to_string(),
            requested_mode: "setup".to_string(),
            locale: "en".to_string(),
            artifact_extension: ".yaml".to_string(),
            root_resolution,
            profile_resolutions,
            resolved_targets: vec![target],
            compatibility_reports,
        });

        assert_eq!(artifact.schema_version, 1);
        assert_eq!(artifact.bundle_id, "bundle.demo");
        assert_eq!(artifact.resolved_targets.len(), 1);
        assert_eq!(
            artifact.generated_resolved_files,
            vec!["resolved/default.yaml".to_string()]
        );
        assert_eq!(artifact.compatibility_reports.len(), 1);
    }

    #[test]
    fn pack_section_and_decode_validation_failures_are_reported() {
        let mut section = declared_section();
        section.schema_version = 2;
        assert!(matches!(
            section.validate(),
            Err(CapabilitySchemaError::Compatibility(message)) if message.contains("unsupported pack capability schema_version")
        ));

        assert!(matches!(
            decode_pack_capability_section_from_cbor(&[0xff]),
            Err(CapabilitySchemaError::Decode(_))
        ));

        assert!(matches!(
            decode_capability_declaration_from_cbor(&[0xff]),
            Err(CapabilitySchemaError::Decode(_))
        ));
    }

    #[test]
    fn compatibility_helpers_report_expected_failure_modes() {
        let section = declared_section();
        let offer = &section.declaration.offers[0];

        let component_ref_mismatch = CapabilityComponentDescriptor {
            component_ref: "component:other".to_string(),
            ..component()
        };
        let mismatch_report =
            check_offer_component_compatibility(offer, &component_ref_mismatch).expect("report");
        assert!(mismatch_report.issues.iter().any(|issue| matches!(
            issue,
            CapabilityCompatibilityIssue::ComponentMismatch { .. }
        )));

        let missing_capability = CapabilityComponentDescriptor {
            capabilities: Vec::new(),
            ..component()
        };
        let missing_cap_report =
            check_offer_component_compatibility(offer, &missing_capability).expect("report");
        assert!(missing_cap_report.issues.iter().any(|issue| matches!(
            issue,
            CapabilityCompatibilityIssue::MissingCapability { .. }
        )));

        let missing_operation = CapabilityComponentDescriptor {
            operations: Vec::new(),
            ..component()
        };
        let missing_operation_report =
            check_offer_component_compatibility(offer, &missing_operation).expect("report");
        assert!(
            missing_operation_report.issues.iter().any(|issue| matches!(
                issue,
                CapabilityCompatibilityIssue::MissingOperation { .. }
            ))
        );

        let input_mismatch = CapabilityComponentDescriptor {
            operations: vec![CapabilityComponentOperation {
                name: "provide".to_string(),
                input_schema: serde_json::json!({"type": "number"}),
                output_schema: serde_json::json!({"type": "string"}),
            }],
            ..component()
        };
        let input_mismatch_report =
            check_offer_component_compatibility(offer, &input_mismatch).expect("report");
        assert!(input_mismatch_report.issues.iter().any(|issue| matches!(
            issue,
            CapabilityCompatibilityIssue::InputSchemaMismatch { .. }
        )));

        let output_mismatch = CapabilityComponentDescriptor {
            operations: vec![CapabilityComponentOperation {
                name: "provide".to_string(),
                input_schema: serde_json::json!({"type": "string"}),
                output_schema: serde_json::json!({"type": "number"}),
            }],
            ..component()
        };
        let output_mismatch_report =
            check_offer_component_compatibility(offer, &output_mismatch).expect("report");
        assert!(output_mismatch_report.issues.iter().any(|issue| matches!(
            issue,
            CapabilityCompatibilityIssue::OutputSchemaMismatch { .. }
        )));

        let legacy_offer = {
            let mut offer = CapabilityOffer::new("offer.legacy", cap("cap://runtime.metrics"));
            offer.provider = Some(CapabilityProviderRef {
                component_ref: "component:redis".to_string(),
                operation: "legacy".to_string(),
                operation_map: Vec::new(),
            });
            offer
        };
        let legacy_report =
            check_offer_component_compatibility(&legacy_offer, &component()).expect("report");
        assert!(legacy_report.issues.iter().any(|issue| matches!(
            issue,
            CapabilityCompatibilityIssue::MissingProviderOperation { .. }
        )));

        let missing_provider_offer =
            CapabilityOffer::new("offer.noprovider", cap("cap://memory.short-term"));
        let missing_provider_report =
            check_offer_component_compatibility(&missing_provider_offer, &component())
                .expect("report");
        assert!(
            missing_provider_report
                .issues
                .iter()
                .any(|issue| matches!(issue, CapabilityCompatibilityIssue::MissingProvider))
        );

        let duplicate_operations = CapabilityComponentDescriptor {
            operations: vec![
                CapabilityComponentOperation {
                    name: "provide".to_string(),
                    input_schema: serde_json::json!({"type": "string"}),
                    output_schema: serde_json::json!({"type": "string"}),
                },
                CapabilityComponentOperation {
                    name: "provide".to_string(),
                    input_schema: serde_json::json!({"type": "string"}),
                    output_schema: serde_json::json!({"type": "string"}),
                },
            ],
            ..component()
        };
        assert!(matches!(
            check_pack_capability_compatibility(&section, &duplicate_operations),
            Err(CapabilitySchemaError::Compatibility(message)) if message.contains("duplicate component operation")
        ));

        let empty_component_ref = CapabilityComponentDescriptor {
            component_ref: String::new(),
            ..component()
        };
        assert!(matches!(
            check_pack_capability_compatibility(&section, &empty_component_ref),
            Err(CapabilitySchemaError::Compatibility(message)) if message.contains("component_ref must not be empty")
        ));

        let empty_version = CapabilityComponentDescriptor {
            version: String::new(),
            ..component()
        };
        assert!(matches!(
            check_pack_capability_compatibility(&section, &empty_version),
            Err(CapabilitySchemaError::Compatibility(message)) if message.contains("version must not be empty")
        ));
    }

    #[test]
    fn helper_builders_and_emissions_are_consistent() {
        let declaration = declaration();
        let root_resolution = greentic_cap_resolver::resolve_root(&declaration).expect("resolve");
        let target = target_from_resolution_report(
            CapabilityResolvedTargetInputV1 {
                path: "resolved/default.yaml".to_string(),
                tenant: "default".to_string(),
                team: Some("default".to_string()),
                default_policy: "forbidden".to_string(),
                tenant_gmap: "tenants/default/tenant.gmap".to_string(),
                team_gmap: Some("tenants/default/teams/default/team.gmap".to_string()),
                app_pack_policies: vec![resolved_reference_policy("app-pack", "forbidden")],
            },
            &root_resolution,
        );

        let emitted = emitted_bindings(&root_resolution);
        assert_eq!(target.bindings, emitted);
        assert_eq!(
            emitted_binding_count(std::slice::from_ref(&root_resolution)),
            emitted.len()
        );

        let target = target_with_reference_policies(
            target,
            vec![resolved_reference_policy("app-pack", "allowed")],
        );
        assert_eq!(target.app_pack_policies.len(), 1);
        assert_eq!(target.app_pack_policies[0].policy, "allowed");

        let artifact = build_bundle_resolution_artifact(CapabilityBundleResolutionInputV1 {
            bundle_id: "bundle.demo".to_string(),
            bundle_name: "Demo Bundle".to_string(),
            requested_mode: "setup".to_string(),
            locale: "en".to_string(),
            artifact_extension: ".yaml".to_string(),
            root_resolution,
            profile_resolutions: BTreeMap::new(),
            resolved_targets: vec![target],
            compatibility_reports: Vec::new(),
        });
        assert_eq!(artifact.schema_version, 1);
        assert_eq!(
            artifact.generated_resolved_files,
            vec!["resolved/default.yaml".to_string()]
        );
    }
}
