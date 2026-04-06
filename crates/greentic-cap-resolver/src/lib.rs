//! Deterministic resolution helpers for Greentic capability profiles.

#![forbid(unsafe_code)]

use greentic_cap_profile::{
    CapabilityProfileExpansion, CapabilityProfileExpansionError, expand_profile, expand_root,
};
use greentic_cap_types::{
    CapabilityBinding, CapabilityBindingKind, CapabilityDeclaration, CapabilityId, CapabilityOffer,
    CapabilityResolution, CapabilityValidationError,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A resolution report that records deterministic bindings and any conflicts encountered.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityResolutionReport {
    /// Profile identifier, or `None` for the root generic view.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub profile_id: Option<String>,
    /// Final bindings chosen by the resolver.
    pub resolution: CapabilityResolution,
    /// Non-fatal resolver issues.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub issues: Vec<CapabilityResolutionIssue>,
}

/// Deterministic resolver warnings and conflicts.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "snake_case"))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub enum CapabilityResolutionIssue {
    /// No matching offers were found for a request.
    UnresolvedRequest {
        /// Profile identifier, or `None` for the root view.
        #[cfg_attr(
            feature = "serde",
            serde(default, skip_serializing_if = "Option::is_none")
        )]
        profile_id: Option<String>,
        /// Request identifier.
        request_id: String,
        /// Requested capability.
        capability: CapabilityId,
        /// Request kind.
        request_kind: CapabilityBindingKind,
    },
    /// Multiple offers tied for the same best score.
    AmbiguousOffer {
        /// Profile identifier, or `None` for the root view.
        #[cfg_attr(
            feature = "serde",
            serde(default, skip_serializing_if = "Option::is_none")
        )]
        profile_id: Option<String>,
        /// Request identifier.
        request_id: String,
        /// Requested capability.
        capability: CapabilityId,
        /// Request kind.
        request_kind: CapabilityBindingKind,
        /// Candidate offer identifiers in deterministic order.
        candidates: Vec<String>,
        /// Chosen offer identifier.
        chosen: String,
    },
}

/// Errors produced while resolving capability profiles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityResolverError {
    /// Underlying capability declaration validation failed.
    Validation(CapabilityValidationError),
    /// Profile expansion failed.
    Expansion(CapabilityProfileExpansionError),
}

impl std::fmt::Display for CapabilityResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(err) => write!(f, "{err}"),
            Self::Expansion(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for CapabilityResolverError {}

impl From<CapabilityValidationError> for CapabilityResolverError {
    fn from(value: CapabilityValidationError) -> Self {
        Self::Validation(value)
    }
}

impl From<CapabilityProfileExpansionError> for CapabilityResolverError {
    fn from(value: CapabilityProfileExpansionError) -> Self {
        Self::Expansion(value)
    }
}

/// Resolves the root generic view of a declaration.
pub fn resolve_root(
    declaration: &CapabilityDeclaration,
) -> Result<CapabilityResolutionReport, CapabilityResolverError> {
    let expansion = expand_root(declaration)?;
    resolve_expansion(declaration, expansion)
}

/// Resolves a specific named profile.
pub fn resolve_profile(
    declaration: &CapabilityDeclaration,
    profile_id: &str,
) -> Result<CapabilityResolutionReport, CapabilityResolverError> {
    let expansion = expand_profile(declaration, profile_id)?;
    resolve_expansion(declaration, expansion)
}

/// Resolves every named profile in a declaration.
pub fn resolve_profiles(
    declaration: &CapabilityDeclaration,
) -> Result<Vec<CapabilityResolutionReport>, CapabilityResolverError> {
    let expansions = greentic_cap_profile::expand_profiles(declaration)?;
    let mut reports = Vec::with_capacity(expansions.len());
    for (_, expansion) in expansions {
        reports.push(resolve_expansion(declaration, expansion)?);
    }
    Ok(reports)
}

fn resolve_expansion(
    declaration: &CapabilityDeclaration,
    expansion: CapabilityProfileExpansion,
) -> Result<CapabilityResolutionReport, CapabilityResolverError> {
    let mut resolution = CapabilityResolution::new(declaration.clone());
    let mut issues = Vec::new();

    for requirement in &expansion.requirements {
        match choose_offer(
            declaration,
            requirement.capability.as_str(),
            expansion.profile_id.as_deref(),
        ) {
            Some(candidate) => {
                if candidate.ambiguous {
                    issues.push(CapabilityResolutionIssue::AmbiguousOffer {
                        profile_id: expansion.profile_id.clone(),
                        request_id: requirement.id.clone(),
                        capability: requirement.capability.clone(),
                        request_kind: CapabilityBindingKind::Requirement,
                        candidates: candidate
                            .candidates
                            .iter()
                            .map(|offer| offer.id.clone())
                            .collect(),
                        chosen: candidate.offer.id.clone(),
                    });
                }
                resolution.bindings.push(binding_for_requirement(
                    requirement,
                    candidate.offer,
                    expansion.profile_id.as_deref(),
                ));
            }
            None => issues.push(CapabilityResolutionIssue::UnresolvedRequest {
                profile_id: expansion.profile_id.clone(),
                request_id: requirement.id.clone(),
                capability: requirement.capability.clone(),
                request_kind: CapabilityBindingKind::Requirement,
            }),
        }
    }

    for consume in &expansion.consumes {
        match choose_offer(
            declaration,
            consume.capability.as_str(),
            expansion.profile_id.as_deref(),
        ) {
            Some(candidate) => {
                if candidate.ambiguous {
                    issues.push(CapabilityResolutionIssue::AmbiguousOffer {
                        profile_id: expansion.profile_id.clone(),
                        request_id: consume.id.clone(),
                        capability: consume.capability.clone(),
                        request_kind: CapabilityBindingKind::Consume,
                        candidates: candidate
                            .candidates
                            .iter()
                            .map(|offer| offer.id.clone())
                            .collect(),
                        chosen: candidate.offer.id.clone(),
                    });
                }
                resolution.bindings.push(binding_for_consume(
                    consume,
                    candidate.offer,
                    expansion.profile_id.as_deref(),
                ));
            }
            None => issues.push(CapabilityResolutionIssue::UnresolvedRequest {
                profile_id: expansion.profile_id.clone(),
                request_id: consume.id.clone(),
                capability: consume.capability.clone(),
                request_kind: CapabilityBindingKind::Consume,
            }),
        }
    }

    resolution.validate()?;
    Ok(CapabilityResolutionReport {
        profile_id: expansion.profile_id,
        resolution,
        issues,
    })
}

struct OfferSelection<'a> {
    offer: &'a CapabilityOffer,
    candidates: Vec<&'a CapabilityOffer>,
    ambiguous: bool,
}

fn choose_offer<'a>(
    declaration: &'a CapabilityDeclaration,
    capability: &str,
    profile_id: Option<&str>,
) -> Option<OfferSelection<'a>> {
    let mut best_score: Option<(u8, u8)> = None;
    let mut candidates: Vec<&CapabilityOffer> = Vec::new();

    for offer in declaration
        .offers
        .iter()
        .filter(|offer| offer.capability.as_str() == capability)
        .filter(|offer| offer_matches_profile(offer, profile_id))
    {
        let score = selection_score(offer, profile_id);
        match best_score {
            Some(current) if score < current => continue,
            Some(current) if score == current => candidates.push(offer),
            _ => {
                best_score = Some(score);
                candidates.clear();
                candidates.push(offer);
            }
        }
    }

    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by(|left, right| left.id.cmp(&right.id));
    let chosen = candidates[0];
    let ambiguous = candidates.len() > 1;

    Some(OfferSelection {
        offer: chosen,
        candidates,
        ambiguous,
    })
}

fn offer_matches_profile(offer: &CapabilityOffer, profile_id: Option<&str>) -> bool {
    match profile_id {
        Some(profile_id) => {
            offer.profiles.is_empty()
                || offer
                    .profiles
                    .iter()
                    .any(|candidate| candidate == profile_id)
        }
        None => offer.profiles.is_empty(),
    }
}

fn selection_score(offer: &CapabilityOffer, profile_id: Option<&str>) -> (u8, u8) {
    let profile_score = match profile_id {
        Some(profile_id)
            if offer
                .profiles
                .iter()
                .any(|candidate| candidate == profile_id) =>
        {
            2
        }
        Some(_) => 1,
        None if offer.profiles.is_empty() => 2,
        None => 0,
    };
    let provider_score = u8::from(offer.provider.is_some());
    (profile_score, provider_score)
}

fn binding_for_requirement(
    requirement: &greentic_cap_types::CapabilityRequirement,
    offer: &CapabilityOffer,
    profile_id: Option<&str>,
) -> CapabilityBinding {
    let mut binding = CapabilityBinding::new(
        CapabilityBindingKind::Requirement,
        requirement.id.clone(),
        offer.id.clone(),
        requirement.capability.clone(),
    );
    binding.provider = offer.provider.clone();
    binding.profile = profile_id.map(str::to_owned);
    binding
}

fn binding_for_consume(
    consume: &greentic_cap_types::CapabilityConsume,
    offer: &CapabilityOffer,
    profile_id: Option<&str>,
) -> CapabilityBinding {
    let mut binding = CapabilityBinding::new(
        CapabilityBindingKind::Consume,
        consume.id.clone(),
        offer.id.clone(),
        consume.capability.clone(),
    );
    binding.provider = offer.provider.clone();
    binding.profile = profile_id.map(str::to_owned);
    binding
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_cap_profile::expand_profile;
    use greentic_cap_types::{
        CapabilityConsume, CapabilityOffer, CapabilityProfile, CapabilityRequirement,
    };

    fn cap(value: &str) -> CapabilityId {
        CapabilityId::new(value).expect("valid capability id")
    }

    fn declaration() -> CapabilityDeclaration {
        let mut declaration = CapabilityDeclaration::new();

        let root_requirement =
            CapabilityRequirement::new("require.root", cap("cap://runtime.metrics"));
        declaration.requires.push(root_requirement);

        let root_consume = CapabilityConsume::new("consume.root", cap("cap://runtime.metrics"));
        declaration.consumes.push(root_consume);

        let mut profile_requirement =
            CapabilityRequirement::new("require.profile", cap("cap://memory.short-term"));
        profile_requirement
            .profiles
            .push("memory-default".to_string());
        declaration.requires.push(profile_requirement);

        let mut profile_consume =
            CapabilityConsume::new("consume.profile", cap("cap://memory.short-term"));
        profile_consume.profiles.push("memory-default".to_string());
        declaration.consumes.push(profile_consume);

        declaration.profiles.push(CapabilityProfile {
            id: "memory-default".to_string(),
            description: None,
            requires: vec![],
            consumes: vec![],
        });

        let mut root_offer = CapabilityOffer::new("offer.metrics", cap("cap://runtime.metrics"));
        root_offer.provider = Some(greentic_cap_types::CapabilityProviderRef {
            component_ref: "component:metrics".to_string(),
            operation: "export".to_string(),
            operation_map: Vec::new(),
        });
        declaration.offers.push(root_offer);

        let mut generic_offer =
            CapabilityOffer::new("offer.memory.generic", cap("cap://memory.short-term"));
        generic_offer.provider = Some(greentic_cap_types::CapabilityProviderRef {
            component_ref: "component:memory-generic".to_string(),
            operation: "provide".to_string(),
            operation_map: Vec::new(),
        });
        declaration.offers.push(generic_offer);

        let mut profile_offer =
            CapabilityOffer::new("offer.memory.profile", cap("cap://memory.short-term"));
        profile_offer.profiles.push("memory-default".to_string());
        profile_offer.provider = Some(greentic_cap_types::CapabilityProviderRef {
            component_ref: "component:memory-profile".to_string(),
            operation: "provide".to_string(),
            operation_map: Vec::new(),
        });
        declaration.offers.push(profile_offer);

        declaration
    }

    #[test]
    fn resolves_root_view_deterministically() {
        let declaration = declaration();
        let report = resolve_root(&declaration).expect("resolve");
        assert_eq!(report.profile_id, None);
        assert_eq!(report.resolution.bindings.len(), 2);
        assert!(report.issues.is_empty());
        assert_eq!(report.resolution.bindings[0].offer_id, "offer.metrics");
    }

    #[test]
    fn profile_resolution_prefers_profile_specific_offer() {
        let declaration = declaration();
        let report = resolve_profile(&declaration, "memory-default").expect("resolve");
        assert_eq!(report.profile_id.as_deref(), Some("memory-default"));
        assert_eq!(report.resolution.bindings.len(), 4);
        assert!(report.issues.is_empty());
        let selected = report
            .resolution
            .bindings
            .iter()
            .find(|binding| binding.request_id == "require.profile")
            .expect("binding");
        assert_eq!(selected.offer_id, "offer.memory.profile");
    }

    #[test]
    fn ambiguous_offers_are_reported() {
        let mut declaration = declaration();
        let mut duplicate =
            CapabilityOffer::new("offer.memory.profile-2", cap("cap://memory.short-term"));
        duplicate.profiles.push("memory-default".to_string());
        duplicate.provider = Some(greentic_cap_types::CapabilityProviderRef {
            component_ref: "component:memory-profile-2".to_string(),
            operation: "provide".to_string(),
            operation_map: Vec::new(),
        });
        declaration.offers.push(duplicate);

        let expansion = expand_profile(&declaration, "memory-default").expect("expand");
        let report = resolve_expansion(&declaration, expansion).expect("resolve");
        assert!(report
            .issues
            .iter()
            .any(|issue| matches!(issue, CapabilityResolutionIssue::AmbiguousOffer { request_id, .. } if request_id == "require.profile")));
    }
}
