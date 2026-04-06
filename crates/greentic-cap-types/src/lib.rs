//! Canonical capability data model for Greentic.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[cfg(feature = "schemars")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Capability metadata stored alongside offers, requirements, and consumes.
pub type CapabilityMetadata = BTreeMap<String, serde_json::Value>;

/// Error returned when parsing a capability identifier fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityIdError {
    /// The identifier does not use the `cap://` scheme.
    MissingScheme,
    /// The identifier has no content after `cap://`.
    Empty,
    /// The identifier contains an unsupported character.
    InvalidCharacter { ch: char, index: usize },
}

impl fmt::Display for CapabilityIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingScheme => write!(f, "capability ids must use the cap:// scheme"),
            Self::Empty => write!(f, "capability ids must not be empty"),
            Self::InvalidCharacter { ch, index } => {
                write!(
                    f,
                    "capability id contains invalid character {ch:?} at index {index}"
                )
            }
        }
    }
}

impl std::error::Error for CapabilityIdError {}

/// Canonical capability identifier.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityId(String);

impl CapabilityId {
    /// Creates a validated capability identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, CapabilityIdError> {
        let value = value.into();
        Self::validate(&value)?;
        Ok(Self(value))
    }

    /// Returns the inner capability identifier string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(value: &str) -> Result<(), CapabilityIdError> {
        if value.is_empty() {
            return Err(CapabilityIdError::Empty);
        }
        if !value.starts_with("cap://") {
            return Err(CapabilityIdError::MissingScheme);
        }

        let remainder = &value["cap://".len()..];
        if remainder.is_empty() {
            return Err(CapabilityIdError::Empty);
        }

        for (index, ch) in value.char_indices() {
            if ch.is_ascii_alphanumeric() || matches!(ch, ':' | '/' | '-' | '_' | '.' | '+') {
                continue;
            }
            return Err(CapabilityIdError::InvalidCharacter { ch, index });
        }

        Ok(())
    }
}

impl TryFrom<String> for CapabilityId {
    type Error = CapabilityIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::validate(&value)?;
        Ok(Self(value))
    }
}

impl From<CapabilityId> for String {
    fn from(value: CapabilityId) -> Self {
        value.0
    }
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Optional provider reference for an offered capability.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityProviderRef {
    /// Provider component identifier or component ref.
    pub component_ref: String,
    /// Provider operation used for this capability.
    pub operation: String,
    /// Optional map from logical contract operations to concrete component operations.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub operation_map: Vec<CapabilityProviderOperationMap>,
}

/// Logical capability operation mapped to a concrete component operation.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityProviderOperationMap {
    /// Logical contract operation name.
    pub contract_operation: String,
    /// Concrete component operation name.
    pub component_operation: String,
    /// Input schema expected by the contract.
    pub input_schema: serde_json::Value,
    /// Output schema expected by the contract.
    pub output_schema: serde_json::Value,
}

/// Self-description for a component used by pack capability compatibility checks.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityComponentDescriptor {
    /// Component identifier or component ref.
    pub component_ref: String,
    /// Component version string.
    pub version: String,
    /// Operations exposed by the component.
    #[cfg_attr(feature = "serde", serde(default))]
    pub operations: Vec<CapabilityComponentOperation>,
    /// Capability ids declared by the component.
    #[cfg_attr(feature = "serde", serde(default))]
    pub capabilities: Vec<CapabilityId>,
    /// Free-form metadata.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "BTreeMap::is_empty")
    )]
    pub metadata: CapabilityMetadata,
}

/// A single component operation in the self-description.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityComponentOperation {
    /// Operation name.
    pub name: String,
    /// Input schema for the operation.
    pub input_schema: serde_json::Value,
    /// Output schema for the operation.
    pub output_schema: serde_json::Value,
}

/// Offers a capability from a provider.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityOffer {
    /// Stable offer identifier.
    pub id: String,
    /// Capability being offered.
    pub capability: CapabilityId,
    /// Provider implementation reference.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub provider: Option<CapabilityProviderRef>,
    /// Profiles that this offer belongs to.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub profiles: Vec<String>,
    /// Optional human-readable description.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub description: Option<String>,
    /// Free-form metadata.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "BTreeMap::is_empty")
    )]
    pub metadata: CapabilityMetadata,
}

impl CapabilityOffer {
    /// Creates a new capability offer.
    pub fn new(id: impl Into<String>, capability: CapabilityId) -> Self {
        Self {
            id: id.into(),
            capability,
            provider: None,
            profiles: Vec::new(),
            description: None,
            metadata: BTreeMap::new(),
        }
    }
}

/// Describes a required capability.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityRequirement {
    /// Stable requirement identifier.
    pub id: String,
    /// Required capability.
    pub capability: CapabilityId,
    /// Profiles that this requirement belongs to.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub profiles: Vec<String>,
    /// Whether the requirement is optional.
    #[cfg_attr(feature = "serde", serde(default))]
    pub optional: bool,
    /// Optional human-readable description.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub description: Option<String>,
    /// Free-form metadata.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "BTreeMap::is_empty")
    )]
    pub metadata: CapabilityMetadata,
}

impl CapabilityRequirement {
    /// Creates a new capability requirement.
    pub fn new(id: impl Into<String>, capability: CapabilityId) -> Self {
        Self {
            id: id.into(),
            capability,
            profiles: Vec::new(),
            optional: false,
            description: None,
            metadata: BTreeMap::new(),
        }
    }
}

/// How a capability is consumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub enum CapabilityConsumeMode {
    /// The consumer reads from the capability.
    #[default]
    Shared,
    /// The consumer mutates the capability.
    Exclusive,
    /// The consumer requires ephemeral use.
    Ephemeral,
}

/// Describes a capability that is consumed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityConsume {
    /// Stable consume identifier.
    pub id: String,
    /// Consumed capability.
    pub capability: CapabilityId,
    /// Profiles that this consume belongs to.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub profiles: Vec<String>,
    /// How the capability is consumed.
    #[cfg_attr(feature = "serde", serde(default))]
    pub mode: CapabilityConsumeMode,
    /// Optional human-readable description.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub description: Option<String>,
    /// Free-form metadata.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "BTreeMap::is_empty")
    )]
    pub metadata: CapabilityMetadata,
}

impl CapabilityConsume {
    /// Creates a new capability consume declaration.
    pub fn new(id: impl Into<String>, capability: CapabilityId) -> Self {
        Self {
            id: id.into(),
            capability,
            profiles: Vec::new(),
            mode: CapabilityConsumeMode::Shared,
            description: None,
            metadata: BTreeMap::new(),
        }
    }
}

/// A named bundle of capability requirements and consumes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityProfile {
    /// Profile identifier.
    pub id: String,
    /// Optional human-readable description.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub description: Option<String>,
    /// Requirements included in the profile.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub requires: Vec<CapabilityRequirement>,
    /// Consumes included in the profile.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub consumes: Vec<CapabilityConsume>,
}

impl CapabilityProfile {
    /// Creates a new profile.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: None,
            requires: Vec::new(),
            consumes: Vec::new(),
        }
    }
}

/// Top-level capability declaration payload.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityDeclaration {
    /// Capabilities offered by the pack/component.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub offers: Vec<CapabilityOffer>,
    /// Top-level requirements.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub requires: Vec<CapabilityRequirement>,
    /// Top-level consumes.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub consumes: Vec<CapabilityConsume>,
    /// Optional named profiles.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub profiles: Vec<CapabilityProfile>,
}

impl CapabilityDeclaration {
    /// Creates an empty capability declaration.
    pub fn new() -> Self {
        Self {
            offers: Vec::new(),
            requires: Vec::new(),
            consumes: Vec::new(),
            profiles: Vec::new(),
        }
    }

    /// Validates the declaration for duplicate ids and bad profile references.
    pub fn validate(&self) -> Result<(), CapabilityValidationError> {
        validate_profile_collection(&self.profiles)?;

        let profile_ids: BTreeSet<&str> = self
            .profiles
            .iter()
            .map(|profile| profile.id.as_str())
            .collect();

        validate_section(
            "offers.id",
            self.offers.iter().map(|offer| offer.id.as_str()),
        )?;
        validate_section(
            "requires.id",
            self.requires
                .iter()
                .map(|requirement| requirement.id.as_str()),
        )?;
        validate_section(
            "consumes.id",
            self.consumes.iter().map(|consume| consume.id.as_str()),
        )?;

        for offer in &self.offers {
            validate_profile_refs("offers", &offer.id, &offer.profiles, &profile_ids)?;
            validate_named_id("offers", &offer.id)?;
        }

        for requirement in &self.requires {
            validate_requirement(requirement, &profile_ids)?;
        }

        for consume in &self.consumes {
            validate_consume(consume, &profile_ids)?;
        }

        for profile in &self.profiles {
            profile.validate(&profile_ids)?;
        }

        Ok(())
    }
}

impl Default for CapabilityDeclaration {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityProfile {
    fn validate(&self, known_profiles: &BTreeSet<&str>) -> Result<(), CapabilityValidationError> {
        validate_named_id("profiles.id", &self.id)?;
        for requirement in &self.requires {
            validate_requirement(requirement, known_profiles)?;
        }
        for consume in &self.consumes {
            validate_consume(consume, known_profiles)?;
        }
        Ok(())
    }
}

/// Stable binding between a request and a selected offer.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityBinding {
    /// Binding kind.
    pub kind: CapabilityBindingKind,
    /// The request or consume item that was bound.
    pub request_id: String,
    /// The selected offer identifier.
    pub offer_id: String,
    /// The resolved capability.
    pub capability: CapabilityId,
    /// The selected provider reference, if known.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub provider: Option<CapabilityProviderRef>,
    /// Optional profile association for the binding.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub profile: Option<String>,
}

impl CapabilityBinding {
    /// Creates a binding for a requirement or consume.
    pub fn new(
        kind: CapabilityBindingKind,
        request_id: impl Into<String>,
        offer_id: impl Into<String>,
        capability: CapabilityId,
    ) -> Self {
        Self {
            kind,
            request_id: request_id.into(),
            offer_id: offer_id.into(),
            capability,
            provider: None,
            profile: None,
        }
    }

    /// Validates the binding identifiers.
    pub fn validate(&self) -> Result<(), CapabilityValidationError> {
        validate_named_id("bindings.request_id", &self.request_id)?;
        validate_named_id("bindings.offer_id", &self.offer_id)?;
        Ok(())
    }
}

/// Binding origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub enum CapabilityBindingKind {
    /// Binding for a required capability.
    Requirement,
    /// Binding for a consumed capability.
    Consume,
}

/// Machine-readable capability resolution result.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(JsonSchema))]
pub struct CapabilityResolution {
    /// Original declaration that was resolved.
    pub declaration: CapabilityDeclaration,
    /// Stable bindings chosen by the resolver.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub bindings: Vec<CapabilityBinding>,
}

impl CapabilityResolution {
    /// Creates an empty resolution result for a declaration.
    pub fn new(declaration: CapabilityDeclaration) -> Self {
        Self {
            declaration,
            bindings: Vec::new(),
        }
    }

    /// Validates the underlying declaration and the binding set.
    pub fn validate(&self) -> Result<(), CapabilityValidationError> {
        self.declaration.validate()?;
        validate_section(
            "bindings",
            self.bindings
                .iter()
                .map(|binding| binding.request_id.as_str()),
        )?;
        for binding in &self.bindings {
            binding.validate()?;
        }
        Ok(())
    }
}

/// Validation error for capability declarations and resolutions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityValidationError {
    /// Empty or malformed non-capability identifier.
    InvalidIdentifier {
        /// Logical collection name.
        section: &'static str,
        /// Offending identifier.
        id: String,
    },
    /// Invalid capability identifier.
    InvalidCapabilityId {
        /// Context for the invalid field.
        field: &'static str,
        /// Offending value.
        value: String,
        /// Low-level parse error.
        source: CapabilityIdError,
    },
    /// Duplicate identifier within a collection.
    DuplicateId {
        /// Logical collection name.
        section: &'static str,
        /// Offending identifier.
        id: String,
    },
    /// Empty or malformed profile identifier.
    InvalidProfileId {
        /// Logical collection name.
        section: &'static str,
        /// Offending profile id.
        id: String,
    },
    /// Reference to a non-existent profile.
    UnknownProfileReference {
        /// Logical collection name.
        section: &'static str,
        /// Offending profile reference.
        reference: String,
    },
}

impl fmt::Display for CapabilityValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentifier { section, id } => {
                write!(f, "{section} contains invalid identifier {id:?}")
            }
            Self::InvalidCapabilityId {
                field,
                value,
                source,
            } => {
                write!(
                    f,
                    "{field} contains invalid capability id {value:?}: {source}"
                )
            }
            Self::DuplicateId { section, id } => {
                write!(f, "{section} contains duplicate identifier {id:?}")
            }
            Self::InvalidProfileId { section, id } => {
                write!(f, "{section} contains invalid profile id {id:?}")
            }
            Self::UnknownProfileReference { section, reference } => {
                write!(f, "{section} references unknown profile {reference:?}")
            }
        }
    }
}

impl std::error::Error for CapabilityValidationError {}

fn validate_id_field(field: &'static str, value: &str) -> Result<(), CapabilityValidationError> {
    if value.trim().is_empty() {
        return Err(CapabilityValidationError::InvalidIdentifier {
            section: field,
            id: value.to_owned(),
        });
    }
    CapabilityId::validate(value).map_err(|source| CapabilityValidationError::InvalidCapabilityId {
        field,
        value: value.to_owned(),
        source,
    })
}

fn validate_named_id(section: &'static str, value: &str) -> Result<(), CapabilityValidationError> {
    if value.trim().is_empty() || value.chars().any(char::is_whitespace) {
        return Err(CapabilityValidationError::InvalidIdentifier {
            section,
            id: value.to_owned(),
        });
    }
    Ok(())
}

fn validate_section<'a, I>(section: &'static str, ids: I) -> Result<(), CapabilityValidationError>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut seen = BTreeSet::new();
    for id in ids {
        validate_named_id(section, id)?;
        if !seen.insert(id.to_owned()) {
            return Err(CapabilityValidationError::DuplicateId {
                section,
                id: id.to_owned(),
            });
        }
    }
    Ok(())
}

fn validate_profile_collection(
    profiles: &[CapabilityProfile],
) -> Result<(), CapabilityValidationError> {
    let mut seen = BTreeSet::new();
    for profile in profiles {
        if profile.id.trim().is_empty() || profile.id.chars().any(char::is_whitespace) {
            return Err(CapabilityValidationError::InvalidProfileId {
                section: "profiles",
                id: profile.id.clone(),
            });
        }
        if !seen.insert(profile.id.clone()) {
            return Err(CapabilityValidationError::DuplicateId {
                section: "profiles",
                id: profile.id.clone(),
            });
        }
    }
    Ok(())
}

fn validate_profile_refs(
    section: &'static str,
    owner: &str,
    refs: &[String],
    known_profiles: &BTreeSet<&str>,
) -> Result<(), CapabilityValidationError> {
    let mut seen = BTreeSet::new();
    for reference in refs {
        if reference.trim().is_empty() || reference.chars().any(char::is_whitespace) {
            return Err(CapabilityValidationError::InvalidProfileId {
                section,
                id: reference.clone(),
            });
        }
        if !seen.insert(reference.clone()) {
            return Err(CapabilityValidationError::DuplicateId {
                section,
                id: format!("{owner}:{reference}"),
            });
        }
        if !known_profiles.contains(reference.as_str()) {
            return Err(CapabilityValidationError::UnknownProfileReference {
                section,
                reference: reference.clone(),
            });
        }
    }
    Ok(())
}

fn validate_requirement(
    requirement: &CapabilityRequirement,
    known_profiles: &BTreeSet<&str>,
) -> Result<(), CapabilityValidationError> {
    validate_id_field("requires.capability", requirement.capability.as_str())?;
    validate_named_id("requires.id", &requirement.id)?;
    validate_profile_refs(
        "requires.profiles",
        &requirement.id,
        &requirement.profiles,
        known_profiles,
    )
}

fn validate_consume(
    consume: &CapabilityConsume,
    known_profiles: &BTreeSet<&str>,
) -> Result<(), CapabilityValidationError> {
    validate_id_field("consumes.capability", consume.capability.as_str())?;
    validate_named_id("consumes.id", &consume.id)?;
    validate_profile_refs(
        "consumes.profiles",
        &consume.id,
        &consume.profiles,
        known_profiles,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap(value: &str) -> CapabilityId {
        CapabilityId::new(value).expect("valid capability id")
    }

    #[test]
    fn capability_id_requires_cap_scheme() {
        let err = CapabilityId::new("memory.short-term").unwrap_err();
        assert_eq!(err, CapabilityIdError::MissingScheme);
    }

    #[test]
    fn declaration_round_trips_json_and_cbor() {
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

        declaration.validate().expect("valid declaration");

        let json = serde_json::to_string_pretty(&declaration).expect("json encode");
        let decoded_json: CapabilityDeclaration = serde_json::from_str(&json).expect("json decode");
        assert_eq!(declaration, decoded_json);

        let cbor = serde_cbor::to_vec(&declaration).expect("cbor encode");
        let decoded_cbor: CapabilityDeclaration =
            serde_cbor::from_slice(&cbor).expect("cbor decode");
        assert_eq!(declaration, decoded_cbor);
    }

    #[test]
    fn declaration_rejects_duplicate_offers() {
        let mut declaration = CapabilityDeclaration::new();
        declaration.offers.push(CapabilityOffer::new(
            "offer.memory",
            cap("cap://memory.short-term"),
        ));
        declaration.offers.push(CapabilityOffer::new(
            "offer.memory",
            cap("cap://memory.short-term"),
        ));

        let err = declaration.validate().unwrap_err();
        assert_eq!(
            err,
            CapabilityValidationError::DuplicateId {
                section: "offers.id",
                id: "offer.memory".to_string(),
            }
        );
    }

    #[test]
    fn declaration_rejects_unknown_profile_reference() {
        let mut declaration = CapabilityDeclaration::new();
        let mut offer = CapabilityOffer::new("offer.memory", cap("cap://memory.short-term"));
        offer.profiles.push("memory-default".to_string());
        declaration.offers.push(offer);

        let err = declaration.validate().unwrap_err();
        assert_eq!(
            err,
            CapabilityValidationError::UnknownProfileReference {
                section: "offers",
                reference: "memory-default".to_string(),
            }
        );
    }

    #[test]
    fn declaration_rejects_malformed_profile_ids() {
        let mut declaration = CapabilityDeclaration::new();
        declaration.profiles.push(CapabilityProfile::new(" "));
        let err = declaration.validate().unwrap_err();
        assert_eq!(
            err,
            CapabilityValidationError::InvalidProfileId {
                section: "profiles",
                id: " ".to_string(),
            }
        );
    }

    #[test]
    fn declaration_rejects_empty_requirement_ids() {
        let mut declaration = CapabilityDeclaration::new();
        declaration.requires.push(CapabilityRequirement::new(
            "",
            cap("cap://memory.short-term"),
        ));
        let err = declaration.validate().unwrap_err();
        assert_eq!(
            err,
            CapabilityValidationError::InvalidIdentifier {
                section: "requires.id",
                id: String::new(),
            }
        );
    }

    #[test]
    fn resolution_allows_multiple_requests_to_share_an_offer() {
        let mut declaration = CapabilityDeclaration::new();
        declaration.offers.push(CapabilityOffer::new(
            "offer.memory",
            cap("cap://memory.short-term"),
        ));

        declaration.requires.push(CapabilityRequirement::new(
            "require.one",
            cap("cap://memory.short-term"),
        ));
        declaration.requires.push(CapabilityRequirement::new(
            "require.two",
            cap("cap://memory.short-term"),
        ));

        let mut resolution = CapabilityResolution::new(declaration);
        resolution.bindings.push(CapabilityBinding::new(
            CapabilityBindingKind::Requirement,
            "require.one",
            "offer.memory",
            cap("cap://memory.short-term"),
        ));
        resolution.bindings.push(CapabilityBinding::new(
            CapabilityBindingKind::Requirement,
            "require.two",
            "offer.memory",
            cap("cap://memory.short-term"),
        ));

        resolution.validate().expect("shared offer binding");
    }

    #[test]
    fn provider_operation_map_is_serialized() {
        let provider = CapabilityProviderRef {
            component_ref: "component:redis".to_string(),
            operation: "provide".to_string(),
            operation_map: vec![CapabilityProviderOperationMap {
                contract_operation: "read".to_string(),
                component_operation: "provide".to_string(),
                input_schema: serde_json::json!({"type": "string"}),
                output_schema: serde_json::json!({"type": "string"}),
            }],
        };

        let json = serde_json::to_value(&provider).expect("json");
        assert!(json.get("operation_map").is_some());
    }
}
