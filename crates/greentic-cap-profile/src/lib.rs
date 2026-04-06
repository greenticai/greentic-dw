//! Profile expansion helpers for Greentic capability declarations.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use greentic_cap_types::{
    CapabilityConsume, CapabilityDeclaration, CapabilityProfile, CapabilityRequirement,
    CapabilityValidationError,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Expanded capability view for a single profile or the root declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityProfileExpansion {
    /// Profile identifier, or `None` for the root generic view.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub profile_id: Option<String>,
    /// Effective requirements after expansion.
    #[cfg_attr(feature = "serde", serde(default))]
    pub requirements: Vec<CapabilityRequirement>,
    /// Effective consumes after expansion.
    #[cfg_attr(feature = "serde", serde(default))]
    pub consumes: Vec<CapabilityConsume>,
}

impl CapabilityProfileExpansion {
    /// Creates a new expansion result.
    pub fn new(profile_id: Option<String>) -> Self {
        Self {
            profile_id,
            requirements: Vec::new(),
            consumes: Vec::new(),
        }
    }
}

/// Errors produced while expanding capability profiles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityProfileExpansionError {
    /// Underlying capability declaration validation failed.
    Validation(CapabilityValidationError),
    /// The requested profile does not exist.
    UnknownProfile { profile_id: String },
    /// Conflicting requirement entries were encountered while merging.
    ConflictingRequirement {
        /// Profile being expanded.
        profile_id: Option<String>,
        /// Requirement identifier.
        id: String,
    },
    /// Conflicting consume entries were encountered while merging.
    ConflictingConsume {
        /// Profile being expanded.
        profile_id: Option<String>,
        /// Consume identifier.
        id: String,
    },
}

impl std::fmt::Display for CapabilityProfileExpansionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(err) => write!(f, "{err}"),
            Self::UnknownProfile { profile_id } => {
                write!(f, "unknown capability profile {profile_id:?}")
            }
            Self::ConflictingRequirement { profile_id, id } => {
                write!(
                    f,
                    "profile {profile_id:?} contains conflicting requirement {id:?}"
                )
            }
            Self::ConflictingConsume { profile_id, id } => {
                write!(
                    f,
                    "profile {profile_id:?} contains conflicting consume {id:?}"
                )
            }
        }
    }
}

impl std::error::Error for CapabilityProfileExpansionError {}

impl From<CapabilityValidationError> for CapabilityProfileExpansionError {
    fn from(value: CapabilityValidationError) -> Self {
        Self::Validation(value)
    }
}

/// Expands the root view of a declaration, collecting generic requirements and consumes.
pub fn expand_root(
    declaration: &CapabilityDeclaration,
) -> Result<CapabilityProfileExpansion, CapabilityProfileExpansionError> {
    declaration.validate()?;
    expand_generic(declaration, None)
}

/// Expands a named profile within a declaration.
pub fn expand_profile(
    declaration: &CapabilityDeclaration,
    profile_id: &str,
) -> Result<CapabilityProfileExpansion, CapabilityProfileExpansionError> {
    declaration.validate()?;

    let profile = declaration
        .profiles
        .iter()
        .find(|candidate| candidate.id == profile_id)
        .ok_or_else(|| CapabilityProfileExpansionError::UnknownProfile {
            profile_id: profile_id.to_owned(),
        })?;

    let mut expansion = expand_generic(declaration, Some(profile_id))?;
    merge_profile_items(&mut expansion, profile)?;
    Ok(expansion)
}

/// Expands every named profile in a declaration.
pub fn expand_profiles(
    declaration: &CapabilityDeclaration,
) -> Result<BTreeMap<String, CapabilityProfileExpansion>, CapabilityProfileExpansionError> {
    declaration.validate()?;

    let mut expansions = BTreeMap::new();
    for profile in &declaration.profiles {
        let expansion = expand_profile(declaration, profile.id.as_str())?;
        expansions.insert(profile.id.clone(), expansion);
    }
    Ok(expansions)
}

fn expand_generic(
    declaration: &CapabilityDeclaration,
    profile_id: Option<&str>,
) -> Result<CapabilityProfileExpansion, CapabilityProfileExpansionError> {
    let mut requirements = BTreeMap::new();
    let mut consumes = BTreeMap::new();

    for requirement in &declaration.requires {
        if applies_to_profile(&requirement.profiles, profile_id) {
            insert_requirement(&mut requirements, requirement, profile_id)?;
        }
    }

    for consume in &declaration.consumes {
        if applies_to_profile(&consume.profiles, profile_id) {
            insert_consume(&mut consumes, consume, profile_id)?;
        }
    }

    Ok(CapabilityProfileExpansion {
        profile_id: profile_id.map(str::to_owned),
        requirements: requirements.into_values().collect(),
        consumes: consumes.into_values().collect(),
    })
}

fn merge_profile_items(
    expansion: &mut CapabilityProfileExpansion,
    profile: &CapabilityProfile,
) -> Result<(), CapabilityProfileExpansionError> {
    let mut requirements: BTreeMap<String, CapabilityRequirement> = expansion
        .requirements
        .drain(..)
        .map(|item| (item.id.clone(), item))
        .collect();
    let mut consumes: BTreeMap<String, CapabilityConsume> = expansion
        .consumes
        .drain(..)
        .map(|item| (item.id.clone(), item))
        .collect();

    for requirement in &profile.requires {
        insert_requirement(&mut requirements, requirement, Some(profile.id.as_str()))?;
    }

    for consume in &profile.consumes {
        insert_consume(&mut consumes, consume, Some(profile.id.as_str()))?;
    }

    expansion.requirements = requirements.into_values().collect();
    expansion.consumes = consumes.into_values().collect();
    Ok(())
}

fn applies_to_profile(profiles: &[String], profile_id: Option<&str>) -> bool {
    match profile_id {
        Some(profile_id) => profiles.is_empty() || profiles.iter().any(|entry| entry == profile_id),
        None => profiles.is_empty(),
    }
}

fn insert_requirement(
    collection: &mut BTreeMap<String, CapabilityRequirement>,
    requirement: &CapabilityRequirement,
    profile_id: Option<&str>,
) -> Result<(), CapabilityProfileExpansionError> {
    match collection.get(&requirement.id) {
        Some(existing) if existing != requirement => {
            Err(CapabilityProfileExpansionError::ConflictingRequirement {
                profile_id: profile_id.map(str::to_owned),
                id: requirement.id.clone(),
            })
        }
        Some(_) => Ok(()),
        None => {
            collection.insert(requirement.id.clone(), requirement.clone());
            Ok(())
        }
    }
}

fn insert_consume(
    collection: &mut BTreeMap<String, CapabilityConsume>,
    consume: &CapabilityConsume,
    profile_id: Option<&str>,
) -> Result<(), CapabilityProfileExpansionError> {
    match collection.get(&consume.id) {
        Some(existing) if existing != consume => {
            Err(CapabilityProfileExpansionError::ConflictingConsume {
                profile_id: profile_id.map(str::to_owned),
                id: consume.id.clone(),
            })
        }
        Some(_) => Ok(()),
        None => {
            collection.insert(consume.id.clone(), consume.clone());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_cap_types::{CapabilityConsume, CapabilityId, CapabilityOffer, CapabilityProfile};

    fn cap(value: &str) -> CapabilityId {
        CapabilityId::new(value).expect("valid capability id")
    }

    fn declaration() -> CapabilityDeclaration {
        let mut declaration = CapabilityDeclaration::new();

        let mut root_requirement =
            CapabilityRequirement::new("require.root", cap("cap://runtime.metrics"));
        root_requirement.description = Some("global requirement".to_string());
        declaration.requires.push(root_requirement);

        let mut root_consume = CapabilityConsume::new("consume.root", cap("cap://runtime.metrics"));
        root_consume.description = Some("global consume".to_string());
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
            description: Some("default memory profile".to_string()),
            requires: vec![CapabilityRequirement::new(
                "require.profile-local",
                cap("cap://memory.short-term"),
            )],
            consumes: vec![CapabilityConsume::new(
                "consume.profile-local",
                cap("cap://memory.short-term"),
            )],
        });

        declaration.offers.push(CapabilityOffer::new(
            "offer.memory",
            cap("cap://memory.short-term"),
        ));

        declaration
    }

    #[test]
    fn expand_root_only_includes_generic_items() {
        let declaration = declaration();
        let expansion = expand_root(&declaration).expect("expand");
        assert!(expansion.profile_id.is_none());
        assert_eq!(expansion.requirements.len(), 1);
        assert_eq!(expansion.consumes.len(), 1);
        assert_eq!(expansion.requirements[0].id, "require.root");
        assert_eq!(expansion.consumes[0].id, "consume.root");
    }

    #[test]
    fn expand_profile_merges_profile_and_generic_items() {
        let declaration = declaration();
        let expansion = expand_profile(&declaration, "memory-default").expect("expand");
        assert_eq!(expansion.profile_id.as_deref(), Some("memory-default"));
        assert_eq!(expansion.requirements.len(), 3);
        assert_eq!(expansion.consumes.len(), 3);
        assert_eq!(
            expansion
                .requirements
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["require.profile", "require.profile-local", "require.root"]
        );
        assert_eq!(
            expansion
                .consumes
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["consume.profile", "consume.profile-local", "consume.root"]
        );
    }

    #[test]
    fn expand_profiles_returns_all_named_profiles() {
        let declaration = declaration();
        let profiles = expand_profiles(&declaration).expect("expand");
        assert!(profiles.contains_key("memory-default"));
    }

    #[test]
    fn unknown_profile_is_rejected() {
        let declaration = declaration();
        let err = expand_profile(&declaration, "missing").unwrap_err();
        assert_eq!(
            err,
            CapabilityProfileExpansionError::UnknownProfile {
                profile_id: "missing".to_string(),
            }
        );
    }
}
