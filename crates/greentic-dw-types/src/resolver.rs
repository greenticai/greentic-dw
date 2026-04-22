use crate::{
    BehaviorConfig, BundleInclusionPlan, CapabilityBinding, CompositionSourceProvenance,
    DigitalWorkerTemplate, DwAgentComposition, DwCompositionApplicationMetadata,
    DwCompositionDocument, DwCompositionOutputPlan, DwProviderCatalog, PackDependencyRef,
    ProviderBinding, SetupRequirement, SetupRequirementStatus, TemplateCatalogEntry,
};
use greentic_cap_types::CapabilityId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// Resolution mode used when deriving behavior config from template scaffolds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DwResolutionMode {
    #[serde(alias = "default")]
    Recommended,
    #[serde(alias = "personalised")]
    ReviewAll,
}

/// Input answers and override data for a single agent composition resolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DwAgentResolveRequest {
    pub agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub template: DigitalWorkerTemplate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_template: Option<TemplateCatalogEntry>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub answers: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_overrides: BTreeMap<CapabilityId, String>,
}

/// Application-level input for composition resolution.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DwCompositionResolveRequest {
    pub application_id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<DwAgentResolveRequest>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub shared_provider_overrides: BTreeMap<CapabilityId, String>,
    #[serde(default)]
    pub mode: Option<DwResolutionMode>,
}

#[derive(Debug, Error)]
pub enum DwCompositionResolveError {
    #[error("composition must contain at least one agent")]
    NoAgents,
}

impl DwCompositionResolveRequest {
    /// Resolve templates, answers, and provider selections into a composition document.
    pub fn resolve(
        &self,
        provider_catalog: &DwProviderCatalog,
    ) -> Result<DwCompositionDocument, DwCompositionResolveError> {
        if self.agents.is_empty() {
            return Err(DwCompositionResolveError::NoAgents);
        }

        let mode = self.mode.unwrap_or(DwResolutionMode::Recommended);
        let mut agents = Vec::new();
        let mut shared_pack_dependencies: Vec<PackDependencyRef> = Vec::new();
        let mut bundle_plan: Vec<BundleInclusionPlan> = Vec::new();
        let mut unresolved_setup_items = Vec::new();
        let mut provider_dependency_index: BTreeMap<String, usize> = BTreeMap::new();
        let mut support_pack_index: BTreeMap<String, usize> = BTreeMap::new();
        let mut provenance = CompositionSourceProvenance::default();
        let mut supports_multi_agent_app_pack = false;

        for agent in &self.agents {
            let mut effective_values = agent.template.defaults.values.clone();
            for (key, value) in &agent.answers {
                effective_values.insert(key.clone(), value.clone());
            }

            let display_name = agent
                .display_name
                .clone()
                .or_else(|| {
                    effective_values
                        .get("display_name")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_else(|| agent.template.metadata.name.clone());

            let behavior_mode = match mode {
                DwResolutionMode::Recommended => {
                    &agent.template.behavior_scaffold.default_mode_behavior
                }
                DwResolutionMode::ReviewAll => {
                    &agent.template.behavior_scaffold.personalised_mode_behavior
                }
            };

            let mut capability_bindings = Vec::new();
            let mut agent_unresolved = Vec::new();
            let mut agent_provider_refs = Vec::new();

            let mut capability_specs = Vec::new();
            capability_specs.extend(
                agent
                    .template
                    .capability_plan
                    .required_capabilities
                    .iter()
                    .cloned()
                    .map(|capability_id| (capability_id, false)),
            );
            capability_specs.extend(
                agent
                    .template
                    .capability_plan
                    .optional_capabilities
                    .iter()
                    .cloned()
                    .map(|capability_id| (capability_id, true)),
            );

            for (capability_id, optional) in capability_specs {
                let provider_id = agent
                    .provider_overrides
                    .get(&capability_id)
                    .cloned()
                    .or_else(|| self.shared_provider_overrides.get(&capability_id).cloned())
                    .or_else(|| {
                        agent
                            .template
                            .capability_plan
                            .default_provider_ids
                            .get(&capability_id)
                            .cloned()
                    });

                if let Some(provider_id) = provider_id {
                    if let Some(provider) = provider_catalog.find(&provider_id) {
                        let provider_binding = ProviderBinding {
                            provider_id: provider.provider_id.clone(),
                            source_ref: provider.source_ref.clone(),
                            version: provider.version.clone(),
                            channel: provider.channel.clone(),
                            provider_family: Some(provider.family.clone()),
                            provider_category: Some(provider.category.clone()),
                        };

                        if !agent_provider_refs.contains(&provider_binding.source_ref) {
                            agent_provider_refs.push(provider_binding.source_ref.clone());
                        }

                        let pack_capability_id = provider
                            .capability_profile
                            .pack_capability_ids
                            .first()
                            .cloned();
                        capability_bindings.push(CapabilityBinding {
                            capability_id: capability_id.clone(),
                            provider_binding: provider_binding.clone(),
                            optional,
                            pack_capability_id: pack_capability_id.clone(),
                        });

                        let dependency_key = provider.provider_id.clone();
                        if let Some(index) = provider_dependency_index.get(&dependency_key).copied()
                        {
                            let dependency = &mut shared_pack_dependencies[index];
                            if !dependency.applies_to_agents.contains(&agent.agent_id) {
                                dependency.applies_to_agents.push(agent.agent_id.clone());
                            }
                        } else {
                            provider_dependency_index
                                .insert(dependency_key.clone(), shared_pack_dependencies.len());
                            shared_pack_dependencies.push(PackDependencyRef {
                                pack_id: dependency_key,
                                source_ref: provider.source_ref.source.clone(),
                                version: provider.version.clone(),
                                provider_id: Some(provider.provider_id.clone()),
                                applies_to_agents: vec![agent.agent_id.clone()],
                            });
                        }

                        for setup_ref in &provider.required_setup_schema_refs {
                            agent_unresolved.push(SetupRequirement {
                                requirement_id: format!(
                                    "setup.{}.{}",
                                    agent.agent_id, provider.provider_id
                                ),
                                status: SetupRequirementStatus::Required,
                                summary: format!(
                                    "Setup required for provider `{}`",
                                    provider.provider_id
                                ),
                                provider_id: Some(provider.provider_id.clone()),
                                setup_schema_ref: Some(setup_ref.clone()),
                                question_block_id: provider
                                    .required_question_block_ids
                                    .first()
                                    .cloned(),
                                applies_to_agents: vec![agent.agent_id.clone()],
                            });
                        }
                    } else {
                        agent_unresolved.push(SetupRequirement {
                            requirement_id: format!(
                                "provider.missing.{}.{}",
                                agent.agent_id,
                                capability_id.as_str()
                            ),
                            status: SetupRequirementStatus::Required,
                            summary: format!(
                                "Provider `{provider_id}` for capability `{}` is not present in the provider catalog",
                                capability_id.as_str()
                            ),
                            provider_id: Some(provider_id),
                            setup_schema_ref: None,
                            question_block_id: None,
                            applies_to_agents: vec![agent.agent_id.clone()],
                        });
                    }
                } else if !optional {
                    agent_unresolved.push(SetupRequirement {
                        requirement_id: format!(
                            "provider.unresolved.{}.{}",
                            agent.agent_id,
                            capability_id.as_str()
                        ),
                        status: SetupRequirementStatus::Required,
                        summary: format!(
                            "No provider selected for required capability `{}`",
                            capability_id.as_str()
                        ),
                        provider_id: None,
                        setup_schema_ref: None,
                        question_block_id: None,
                        applies_to_agents: vec![agent.agent_id.clone()],
                    });
                }
            }

            for support_pack in &agent.template.packaging_hints.support_pack_refs {
                let dependency_key = support_pack.source.raw_ref.clone();
                if let Some(index) = support_pack_index.get(&dependency_key).copied() {
                    let dependency = &mut shared_pack_dependencies[index];
                    if !dependency.applies_to_agents.contains(&agent.agent_id) {
                        dependency.applies_to_agents.push(agent.agent_id.clone());
                    }
                } else {
                    support_pack_index
                        .insert(dependency_key.clone(), shared_pack_dependencies.len());
                    shared_pack_dependencies.push(PackDependencyRef {
                        pack_id: dependency_key.clone(),
                        source_ref: support_pack.clone(),
                        version: None,
                        provider_id: None,
                        applies_to_agents: vec![agent.agent_id.clone()],
                    });
                    bundle_plan.push(BundleInclusionPlan {
                        pack_id: dependency_key,
                        source_ref: support_pack.clone(),
                        applies_to_agents: vec![agent.agent_id.clone()],
                        rationale: Some("Template packaging support pack".to_string()),
                    });
                }
            }

            if agent.template.supports_multi_agent_app_pack {
                supports_multi_agent_app_pack = true;
            }

            for provider_ref in &agent_provider_refs {
                if !provenance.provider_source_refs.contains(provider_ref) {
                    provenance.provider_source_refs.push(provider_ref.clone());
                }
            }

            let agent_provenance = CompositionSourceProvenance {
                template_source_ref: agent
                    .selected_template
                    .as_ref()
                    .map(|entry| entry.source_ref.clone()),
                provider_source_refs: agent_provider_refs,
            };

            agents.push(DwAgentComposition {
                agent_id: agent.agent_id.clone(),
                display_name,
                template_id: agent.template.metadata.id.clone(),
                selected_template: agent.selected_template.clone(),
                capability_bindings,
                behavior_config: BehaviorConfig {
                    enabled_question_block_ids: behavior_mode.question_block_ids.clone(),
                    values: effective_values,
                },
                source_provenance: agent_provenance,
            });

            unresolved_setup_items.extend(agent_unresolved);
        }

        if self.agents.len() > 1 {
            supports_multi_agent_app_pack = true;
        }

        Ok(DwCompositionDocument {
            application: DwCompositionApplicationMetadata {
                application_id: self.application_id.clone(),
                display_name: self.display_name.clone(),
                version: self.version.clone(),
                tenant: self.tenant.clone(),
                tags: self.tags.clone(),
            },
            agents,
            shared_pack_dependencies,
            bundle_plan,
            unresolved_setup_items,
            output_plan: DwCompositionOutputPlan {
                generated_pack_id: None,
                generated_bundle_id: None,
                supports_multi_agent_app_pack,
            },
            source_provenance: Some(provenance),
        })
    }
}
