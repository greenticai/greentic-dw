//! Digital Worker manifest contracts and validation.

use greentic_cap_types::{CapabilityDeclaration, CapabilityValidationError};
use greentic_dw_planning::PlanStepKind;
use greentic_dw_types::{
    LocaleContext, LocalePropagation, OutputLocaleGuidance, TaskEnvelope, TenantScope,
    WorkerLocalePolicy,
};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Supported DW manifest schema version.
pub const MANIFEST_SCHEMA_VERSION: &str = "0.2";
pub const CAPABILITY_FAMILY_PLANNING: &str = "greentic.cap.planning.plan";
pub const CAPABILITY_FAMILY_WORKSPACE: &str = "greentic.cap.workspace.artifacts";
pub const CAPABILITY_FAMILY_DELEGATION: &str = "greentic.cap.delegation.route";
pub const CAPABILITY_FAMILY_REFLECTION: &str = "greentic.cap.reflection.review";
pub const CAPABILITY_FAMILY_CONTEXT: &str = "greentic.cap.context.compose";

fn default_manifest_schema_version() -> String {
    MANIFEST_SCHEMA_VERSION.to_string()
}

/// Public manifest for a Digital Worker definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DigitalWorkerManifest {
    /// Schema version for the manifest contract itself.
    #[serde(default = "default_manifest_schema_version")]
    pub version: String,
    pub id: String,
    pub display_name: String,
    /// Worker/package version associated with this manifest instance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_version: Option<String>,
    /// Shared capability declaration reused from the capability workspace.
    #[serde(default)]
    #[schemars(skip)]
    pub capabilities: CapabilityDeclaration,
    pub tenancy: TenancyContract,
    pub locale: LocaleContract,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deep_agent: Option<DeepAgentConfig>,
}

/// Legacy v0.1-style manifest shape used for migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LegacyDigitalWorkerManifest {
    pub id: String,
    pub display_name: String,
    pub version: String,
    pub tenancy: TenancyContract,
    pub locale: LocaleContract,
}

/// Tenant + optional team scope contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TenancyContract {
    /// Tenant is required.
    pub tenant: String,
    /// Team policy is optional and controls inheritance/override behavior.
    pub team_policy: TeamPolicy,
}

/// Team policy for request scope behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TeamPolicy {
    /// Team scope disabled; team is always `None`.
    Disabled,
    /// Team may be inherited from request and/or defaulted.
    Optional {
        default_team: Option<String>,
        allow_request_override: bool,
    },
}

/// Locale contract for a worker manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LocaleContract {
    pub worker_default_locale: String,
    pub policy: WorkerLocalePolicy,
    pub propagation: LocalePropagation,
    pub output: OutputLocaleGuidance,
}

/// Opt-in deep-agent configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeepAgentConfig {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planning_capability: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_capability: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation_capability: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reflection_capability: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_capability: Option<String>,
    #[serde(default)]
    pub plan_step_kinds: Vec<PlanStepKind>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reflection_policy_mandatory: bool,
}

/// Incoming request scope used to derive effective runtime scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RequestScope {
    pub tenant: String,
    pub team: Option<String>,
}

/// Effective scope after tenant/team contract resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResolvedScope {
    pub tenant: String,
    pub team: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestValidationError {
    #[error("manifest schema version must not be empty")]
    EmptyVersion,
    #[error("manifest schema version '{found}' is not supported")]
    UnsupportedVersion { found: String },
    #[error("manifest id must not be empty")]
    EmptyId,
    #[error("manifest display_name must not be empty")]
    EmptyDisplayName,
    #[error("manifest worker version must not be empty when present")]
    EmptyWorkerVersion,
    #[error("tenant must not be empty")]
    EmptyTenant,
    #[error("team value must not be empty when present")]
    EmptyTeam,
    #[error("worker_default_locale must not be empty")]
    EmptyDefaultLocale,
    #[error(transparent)]
    Capability(#[from] CapabilityValidationError),
    #[error(
        "strict_requested locale policy requires output guidance that preserves requested locale"
    )]
    StrictRequestedOutputMismatch,
    #[error("deep-agent mode requires both planning and context capabilities")]
    MissingDeepLoopCoreCapabilities,
    #[error("deep-agent mode with delegate steps requires a delegation capability")]
    MissingDelegationCapability,
    #[error("mandatory reflection policy requires a reflection capability")]
    MissingReflectionCapability,
    #[error("request tenant '{request_tenant}' does not match manifest tenant '{manifest_tenant}'")]
    TenantMismatch {
        request_tenant: String,
        manifest_tenant: String,
    },
}

impl DigitalWorkerManifest {
    /// Converts a legacy manifest shape into the current v0.2 contract.
    pub fn from_legacy(legacy: LegacyDigitalWorkerManifest) -> Self {
        Self {
            version: MANIFEST_SCHEMA_VERSION.to_string(),
            id: legacy.id,
            display_name: legacy.display_name,
            worker_version: Some(legacy.version),
            capabilities: CapabilityDeclaration::default(),
            tenancy: legacy.tenancy,
            locale: legacy.locale,
            deep_agent: None,
        }
    }

    /// Validate manifest contract rules.
    pub fn validate(&self) -> Result<(), ManifestValidationError> {
        if self.version.trim().is_empty() {
            return Err(ManifestValidationError::EmptyVersion);
        }

        if self.version != MANIFEST_SCHEMA_VERSION {
            return Err(ManifestValidationError::UnsupportedVersion {
                found: self.version.clone(),
            });
        }

        if self.id.trim().is_empty() {
            return Err(ManifestValidationError::EmptyId);
        }

        if self.display_name.trim().is_empty() {
            return Err(ManifestValidationError::EmptyDisplayName);
        }

        if let Some(version) = &self.worker_version
            && version.trim().is_empty()
        {
            return Err(ManifestValidationError::EmptyWorkerVersion);
        }

        if self.tenancy.tenant.trim().is_empty() {
            return Err(ManifestValidationError::EmptyTenant);
        }

        match &self.tenancy.team_policy {
            TeamPolicy::Disabled => {}
            TeamPolicy::Optional { default_team, .. } => {
                if let Some(team) = default_team
                    && team.trim().is_empty()
                {
                    return Err(ManifestValidationError::EmptyTeam);
                }
            }
        }

        self.capabilities.validate()?;

        if self.locale.worker_default_locale.trim().is_empty() {
            return Err(ManifestValidationError::EmptyDefaultLocale);
        }

        if matches!(self.locale.policy, WorkerLocalePolicy::StrictRequested)
            && !matches!(
                self.locale.output,
                OutputLocaleGuidance::MatchRequested | OutputLocaleGuidance::Explicit(_)
            )
        {
            return Err(ManifestValidationError::StrictRequestedOutputMismatch);
        }

        if let Some(deep_agent) = &self.deep_agent
            && deep_agent.enabled
        {
            if deep_agent.planning_capability.is_none() || deep_agent.context_capability.is_none() {
                return Err(ManifestValidationError::MissingDeepLoopCoreCapabilities);
            }
            if deep_agent
                .plan_step_kinds
                .iter()
                .any(|kind| matches!(kind, PlanStepKind::Delegate))
                && deep_agent.delegation_capability.is_none()
            {
                return Err(ManifestValidationError::MissingDelegationCapability);
            }
            if deep_agent.reflection_policy_mandatory && deep_agent.reflection_capability.is_none()
            {
                return Err(ManifestValidationError::MissingReflectionCapability);
            }
        }

        Ok(())
    }

    /// Resolve effective tenant/team scope according to inheritance/override semantics.
    pub fn resolve_scope(
        &self,
        request_scope: &RequestScope,
    ) -> Result<ResolvedScope, ManifestValidationError> {
        self.validate()?;

        if request_scope.tenant != self.tenancy.tenant {
            return Err(ManifestValidationError::TenantMismatch {
                request_tenant: request_scope.tenant.clone(),
                manifest_tenant: self.tenancy.tenant.clone(),
            });
        }

        let resolved_team = match &self.tenancy.team_policy {
            TeamPolicy::Disabled => None,
            TeamPolicy::Optional {
                default_team,
                allow_request_override,
            } => {
                if *allow_request_override {
                    request_scope.team.clone().or(default_team.clone())
                } else {
                    default_team.clone()
                }
            }
        };

        Ok(ResolvedScope {
            tenant: self.tenancy.tenant.clone(),
            team: resolved_team,
        })
    }

    /// Build a canonical `TaskEnvelope` from this manifest and an incoming request scope.
    pub fn to_task_envelope(
        &self,
        task_id: impl Into<String>,
        worker_id: impl Into<String>,
        request_scope: &RequestScope,
        requested_locale: Option<String>,
        human_locale: Option<String>,
    ) -> Result<TaskEnvelope, ManifestValidationError> {
        use greentic_dw_types::TaskLifecycleState;

        let scope = self.resolve_scope(request_scope)?;
        let locale = LocaleContext {
            worker_default_locale: self.locale.worker_default_locale.clone(),
            requested_locale,
            human_locale,
            policy: self.locale.policy,
            propagation: self.locale.propagation,
            output: self.locale.output.clone(),
        };

        Ok(TaskEnvelope {
            task_id: task_id.into(),
            worker_id: worker_id.into(),
            state: TaskLifecycleState::Created,
            scope: TenantScope {
                tenant: scope.tenant,
                team: scope.team,
            },
            locale,
        })
    }

    /// Export JSON Schema for this manifest contract.
    pub fn json_schema() -> schemars::Schema {
        schema_for!(DigitalWorkerManifest)
    }

    pub fn deep_agent_capability_families() -> [&'static str; 5] {
        [
            CAPABILITY_FAMILY_PLANNING,
            CAPABILITY_FAMILY_WORKSPACE,
            CAPABILITY_FAMILY_DELEGATION,
            CAPABILITY_FAMILY_REFLECTION,
            CAPABILITY_FAMILY_CONTEXT,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn sample_manifest() -> DigitalWorkerManifest {
        DigitalWorkerManifest {
            id: "dw.support.bot".to_string(),
            display_name: "Support Bot".to_string(),
            version: MANIFEST_SCHEMA_VERSION.to_string(),
            worker_version: Some("0.5".to_string()),
            capabilities: CapabilityDeclaration::new(),
            tenancy: TenancyContract {
                tenant: "tenant-a".to_string(),
                team_policy: TeamPolicy::Optional {
                    default_team: Some("team-default".to_string()),
                    allow_request_override: true,
                },
            },
            locale: LocaleContract {
                worker_default_locale: "en-US".to_string(),
                policy: WorkerLocalePolicy::PreferRequested,
                propagation: LocalePropagation::PropagateToDelegates,
                output: OutputLocaleGuidance::MatchRequested,
            },
            deep_agent: None,
        }
    }

    #[test]
    fn validate_rejects_empty_tenant() {
        let mut manifest = sample_manifest();
        manifest.tenancy.tenant = " ".to_string();

        let err = manifest
            .validate()
            .expect_err("expected empty tenant error");
        assert_eq!(err, ManifestValidationError::EmptyTenant);
    }

    #[test]
    fn resolve_scope_inherits_request_team_when_allowed() {
        let manifest = sample_manifest();
        let request = RequestScope {
            tenant: "tenant-a".to_string(),
            team: Some("team-request".to_string()),
        };

        let resolved = manifest
            .resolve_scope(&request)
            .expect("scope should resolve");
        assert_eq!(resolved.team.as_deref(), Some("team-request"));
    }

    #[test]
    fn resolve_scope_uses_default_when_override_disabled() {
        let mut manifest = sample_manifest();
        manifest.tenancy.team_policy = TeamPolicy::Optional {
            default_team: Some("team-fixed".to_string()),
            allow_request_override: false,
        };

        let request = RequestScope {
            tenant: "tenant-a".to_string(),
            team: Some("team-request".to_string()),
        };

        let resolved = manifest
            .resolve_scope(&request)
            .expect("scope should resolve");
        assert_eq!(resolved.team.as_deref(), Some("team-fixed"));
    }

    #[test]
    fn schema_version_defaults_when_missing() {
        let manifest = sample_manifest();
        let mut instance = serde_json::to_value(manifest).expect("manifest serialization");
        instance
            .as_object_mut()
            .expect("manifest object")
            .remove("version");

        let parsed: DigitalWorkerManifest =
            serde_json::from_value(instance).expect("schema shape still parseable");

        assert_eq!(parsed.version, MANIFEST_SCHEMA_VERSION);
    }

    #[test]
    fn capabilities_default_when_missing() {
        let manifest = sample_manifest();
        let mut instance = serde_json::to_value(manifest).expect("manifest serialization");
        instance
            .as_object_mut()
            .expect("manifest object")
            .remove("capabilities");

        let parsed: DigitalWorkerManifest =
            serde_json::from_value(instance).expect("schema shape still parseable");

        assert!(parsed.capabilities.offers.is_empty());
        assert!(parsed.capabilities.requires.is_empty());
        assert!(parsed.capabilities.consumes.is_empty());
        assert!(parsed.capabilities.profiles.is_empty());
    }

    #[test]
    fn legacy_manifest_can_be_normalized() {
        let legacy = LegacyDigitalWorkerManifest {
            id: "dw.legacy".to_string(),
            display_name: "Legacy Worker".to_string(),
            version: "0.1.0".to_string(),
            tenancy: TenancyContract {
                tenant: "tenant-a".to_string(),
                team_policy: TeamPolicy::Disabled,
            },
            locale: LocaleContract {
                worker_default_locale: "en-US".to_string(),
                policy: WorkerLocalePolicy::WorkerDefault,
                propagation: LocalePropagation::CurrentTaskOnly,
                output: OutputLocaleGuidance::WorkerDefault,
            },
        };

        let current = DigitalWorkerManifest::from_legacy(legacy);
        assert_eq!(current.version, MANIFEST_SCHEMA_VERSION);
        assert_eq!(current.worker_version.as_deref(), Some("0.1.0"));
        assert!(current.capabilities.offers.is_empty());
    }

    #[test]
    fn validate_rejects_invalid_worker_version_when_present() {
        let mut manifest = sample_manifest();
        manifest.worker_version = Some(" ".to_string());

        let err = manifest
            .validate()
            .expect_err("expected empty worker version error");
        assert_eq!(err, ManifestValidationError::EmptyWorkerVersion);
    }

    #[test]
    fn schema_contains_required_tenancy_and_locale_fields() {
        let schema_value = serde_json::to_value(DigitalWorkerManifest::json_schema())
            .expect("schema serialization");

        let required = schema_value
            .pointer("/required")
            .and_then(Value::as_array)
            .expect("required array in schema");

        let required_values: Vec<&str> = required.iter().filter_map(Value::as_str).collect();
        assert!(required_values.contains(&"tenancy"));
        assert!(required_values.contains(&"locale"));
    }

    #[test]
    fn schema_and_validation_reject_empty_tenant_in_instance() {
        let manifest = sample_manifest();
        let mut instance = serde_json::to_value(manifest).expect("manifest serialization");
        instance["tenancy"]["tenant"] = json!("");

        let parsed: DigitalWorkerManifest =
            serde_json::from_value(instance).expect("schema shape still parseable");

        let err = parsed.validate().expect_err("expected validation error");
        assert_eq!(err, ManifestValidationError::EmptyTenant);
    }

    #[test]
    fn deep_agent_requires_planning_and_context() {
        let mut manifest = sample_manifest();
        manifest.deep_agent = Some(DeepAgentConfig {
            enabled: true,
            planning_capability: None,
            workspace_capability: Some(CAPABILITY_FAMILY_WORKSPACE.to_string()),
            delegation_capability: None,
            reflection_capability: None,
            context_capability: None,
            plan_step_kinds: vec![],
            reflection_policy_mandatory: false,
        });

        let err = manifest
            .validate()
            .expect_err("deep agent should require core capabilities");
        assert_eq!(
            err,
            ManifestValidationError::MissingDeepLoopCoreCapabilities
        );
    }

    #[test]
    fn deep_agent_delegate_steps_require_delegation_capability() {
        let mut manifest = sample_manifest();
        manifest.deep_agent = Some(DeepAgentConfig {
            enabled: true,
            planning_capability: Some(CAPABILITY_FAMILY_PLANNING.to_string()),
            workspace_capability: Some(CAPABILITY_FAMILY_WORKSPACE.to_string()),
            delegation_capability: None,
            reflection_capability: Some(CAPABILITY_FAMILY_REFLECTION.to_string()),
            context_capability: Some(CAPABILITY_FAMILY_CONTEXT.to_string()),
            plan_step_kinds: vec![PlanStepKind::Delegate],
            reflection_policy_mandatory: false,
        });

        let err = manifest
            .validate()
            .expect_err("delegate steps should require delegation capability");
        assert_eq!(err, ManifestValidationError::MissingDelegationCapability);
    }

    #[test]
    fn deep_agent_capability_family_constants_are_exposed() {
        let families = DigitalWorkerManifest::deep_agent_capability_families();
        assert!(families.contains(&CAPABILITY_FAMILY_PLANNING));
        assert!(families.contains(&CAPABILITY_FAMILY_CONTEXT));
    }
}
