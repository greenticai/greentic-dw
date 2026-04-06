//! Core orchestration helpers for Greentic capability profile resolution.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use greentic_cap_profile::{CapabilityProfileExpansionError, expand_profiles, expand_root};
use greentic_cap_resolver::{
    CapabilityResolutionReport, CapabilityResolverError, resolve_profile, resolve_root,
};
use greentic_cap_types::CapabilityDeclaration;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Full orchestration output for a capability declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityOrchestrationReport {
    /// Root generic resolution result.
    pub root: CapabilityResolutionReport,
    /// Per-profile resolution results keyed by profile id.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "BTreeMap::is_empty")
    )]
    pub profiles: BTreeMap<String, CapabilityResolutionReport>,
}

/// Errors produced while orchestrating capability resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityCoreError {
    /// Profile expansion failed.
    Expansion(CapabilityProfileExpansionError),
    /// Resolver failed.
    Resolver(CapabilityResolverError),
}

impl std::fmt::Display for CapabilityCoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Expansion(err) => write!(f, "{err}"),
            Self::Resolver(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for CapabilityCoreError {}

impl From<CapabilityProfileExpansionError> for CapabilityCoreError {
    fn from(value: CapabilityProfileExpansionError) -> Self {
        Self::Expansion(value)
    }
}

impl From<CapabilityResolverError> for CapabilityCoreError {
    fn from(value: CapabilityResolverError) -> Self {
        Self::Resolver(value)
    }
}

/// Expands and resolves a declaration in one pass.
pub fn orchestrate_capability_declaration(
    declaration: &CapabilityDeclaration,
) -> Result<CapabilityOrchestrationReport, CapabilityCoreError> {
    let root = resolve_root(declaration)?;
    let profile_expansions = expand_profiles(declaration)?;
    let mut profiles = BTreeMap::new();

    for (profile_id, _) in profile_expansions {
        let report = resolve_profile(declaration, profile_id.as_str())?;
        profiles.insert(profile_id, report);
    }

    Ok(CapabilityOrchestrationReport { root, profiles })
}

/// Returns the generic root expansion without resolving bindings.
pub fn expand_capability_root(
    declaration: &CapabilityDeclaration,
) -> Result<greentic_cap_profile::CapabilityProfileExpansion, CapabilityCoreError> {
    Ok(expand_root(declaration)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_cap_types::{
        CapabilityConsume, CapabilityId, CapabilityOffer, CapabilityProfile, CapabilityProviderRef,
        CapabilityRequirement,
    };

    fn cap(value: &str) -> CapabilityId {
        CapabilityId::new(value).expect("valid capability id")
    }

    fn declaration() -> CapabilityDeclaration {
        let mut declaration = CapabilityDeclaration::new();

        declaration.requires.push(CapabilityRequirement::new(
            "require.root",
            cap("cap://runtime.metrics"),
        ));

        let mut profile_requirement =
            CapabilityRequirement::new("require.profile", cap("cap://memory.short-term"));
        profile_requirement
            .profiles
            .push("memory-default".to_string());
        declaration.requires.push(profile_requirement);

        declaration.consumes.push(CapabilityConsume::new(
            "consume.root",
            cap("cap://runtime.metrics"),
        ));

        declaration.profiles.push(CapabilityProfile {
            id: "memory-default".to_string(),
            description: None,
            requires: vec![],
            consumes: vec![],
        });

        let mut root_offer = CapabilityOffer::new("offer.metrics", cap("cap://runtime.metrics"));
        root_offer.provider = Some(CapabilityProviderRef {
            component_ref: "component:metrics".to_string(),
            operation: "export".to_string(),
            operation_map: Vec::new(),
        });
        declaration.offers.push(root_offer);

        let mut profile_offer =
            CapabilityOffer::new("offer.memory", cap("cap://memory.short-term"));
        profile_offer.profiles.push("memory-default".to_string());
        profile_offer.provider = Some(CapabilityProviderRef {
            component_ref: "component:memory".to_string(),
            operation: "provide".to_string(),
            operation_map: Vec::new(),
        });
        declaration.offers.push(profile_offer);

        declaration
    }

    #[test]
    fn orchestration_produces_root_and_profile_reports() {
        let declaration = declaration();
        let report = orchestrate_capability_declaration(&declaration).expect("orchestrate");
        assert!(report.root.issues.is_empty());
        assert!(report.profiles.contains_key("memory-default"));
        assert!(
            report
                .profiles
                .get("memory-default")
                .expect("profile report")
                .resolution
                .bindings
                .iter()
                .any(|binding| binding.request_id == "require.profile")
        );
    }
}
