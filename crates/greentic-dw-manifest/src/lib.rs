//! Digital Worker manifest contracts and validation.

use greentic_dw_types::{
    LocaleContext, LocalePropagation, OutputLocaleGuidance, TaskEnvelope, TenantScope,
    WorkerLocalePolicy,
};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Public manifest for a Digital Worker definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DigitalWorkerManifest {
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
    #[error("manifest id must not be empty")]
    EmptyId,
    #[error("manifest display_name must not be empty")]
    EmptyDisplayName,
    #[error("manifest version must not be empty")]
    EmptyVersion,
    #[error("tenant must not be empty")]
    EmptyTenant,
    #[error("team value must not be empty when present")]
    EmptyTeam,
    #[error("worker_default_locale must not be empty")]
    EmptyDefaultLocale,
    #[error(
        "strict_requested locale policy requires output guidance that preserves requested locale"
    )]
    StrictRequestedOutputMismatch,
    #[error("request tenant '{request_tenant}' does not match manifest tenant '{manifest_tenant}'")]
    TenantMismatch {
        request_tenant: String,
        manifest_tenant: String,
    },
}

impl DigitalWorkerManifest {
    /// Validate manifest contract rules.
    pub fn validate(&self) -> Result<(), ManifestValidationError> {
        if self.id.trim().is_empty() {
            return Err(ManifestValidationError::EmptyId);
        }

        if self.display_name.trim().is_empty() {
            return Err(ManifestValidationError::EmptyDisplayName);
        }

        if self.version.trim().is_empty() {
            return Err(ManifestValidationError::EmptyVersion);
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
    pub fn json_schema() -> schemars::schema::RootSchema {
        schema_for!(DigitalWorkerManifest)
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
            version: "0.1.0".to_string(),
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
}
