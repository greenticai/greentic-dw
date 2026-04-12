use crate::{
    ApplicationPackLayoutHints, DwCompositionDocument, PackDependencyRef, SetupRequirement,
    TemplateSourceRef,
};
use greentic_cap_types::CapabilityId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Metadata for the generated DW application pack handed to `greentic-pack`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwApplicationPackMetadata {
    pub pack_id: String,
    pub application_id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multi_agent: bool,
}

/// One agent included in the generated application pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwApplicationPackAgent {
    pub agent_id: String,
    pub display_name: String,
    pub template_id: String,
    pub asset_root: String,
}

/// Asset classification for generated DW application packs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DwApplicationPackAssetKind {
    Config,
    Flow,
    Prompt,
    I18n,
    Generic,
}

/// Generic asset emitted into the DW application pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwApplicationPackAsset {
    pub asset_id: String,
    pub path: String,
    pub kind: DwApplicationPackAssetKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applies_to_agents: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<TemplateSourceRef>,
}

/// Generated configuration asset with a concrete serialization format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwGeneratedConfigAsset {
    pub asset_id: String,
    pub path: String,
    pub format: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applies_to_agents: Vec<String>,
}

/// Generated flow asset placeholder for future workflow export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwGeneratedFlowAsset {
    pub asset_id: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applies_to_agents: Vec<String>,
}

/// Generated prompt asset placeholder for prompt-pack style exports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwGeneratedPromptAsset {
    pub asset_id: String,
    pub path: String,
    pub prompt_kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applies_to_agents: Vec<String>,
}

/// Required capability or setup contract exposed by the generated application pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwApplicationPackRequirement {
    pub requirement_id: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_id: Option<CapabilityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applies_to_agents: Vec<String>,
}

/// Pack layout instructions for `greentic-pack`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwApplicationPackLayout {
    pub app_root: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_asset_roots: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_hint: Option<ApplicationPackLayoutHints>,
}

/// Formal handoff contract from `greentic-dw` to `greentic-pack`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwApplicationPackSpec {
    pub metadata: DwApplicationPackMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<DwApplicationPackAgent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<DwApplicationPackAsset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generated_configs: Vec<DwGeneratedConfigAsset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generated_flows: Vec<DwGeneratedFlowAsset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generated_prompts: Vec<DwGeneratedPromptAsset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<DwApplicationPackRequirement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependency_pack_refs: Vec<PackDependencyRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setup_requirements: Vec<SetupRequirement>,
    pub layout: DwApplicationPackLayout,
}

#[derive(Debug, Error)]
pub enum DwApplicationPackMaterializationError {
    #[error("composition must contain at least one agent")]
    NoAgents,
}

impl DwCompositionDocument {
    /// Materialize a resolved composition into a `greentic-pack` handoff contract.
    pub fn to_application_pack_spec(
        &self,
    ) -> Result<DwApplicationPackSpec, DwApplicationPackMaterializationError> {
        if self.agents.is_empty() {
            return Err(DwApplicationPackMaterializationError::NoAgents);
        }

        let multi_agent = self.agents.len() > 1 || self.output_plan.supports_multi_agent_app_pack;
        let pack_id = self
            .output_plan
            .generated_pack_id
            .clone()
            .unwrap_or_else(|| format!("pack.generated.{}", self.application.application_id));

        let mut dependency_pack_refs = self.shared_pack_dependencies.clone();
        dependency_pack_refs.sort_by(|left, right| left.pack_id.cmp(&right.pack_id));

        let agents = self
            .agents
            .iter()
            .map(|agent| DwApplicationPackAgent {
                agent_id: agent.agent_id.clone(),
                display_name: agent.display_name.clone(),
                template_id: agent.template_id.clone(),
                asset_root: format!("agents/{}", agent.agent_id),
            })
            .collect::<Vec<_>>();

        let mut assets = Vec::new();
        let mut generated_configs = Vec::new();
        let mut requirements = Vec::new();

        for agent in &self.agents {
            let applies_to_agents = vec![agent.agent_id.clone()];
            let config_path = format!("agents/{}/config.json", agent.agent_id);

            assets.push(DwApplicationPackAsset {
                asset_id: format!("asset.config.{}", agent.agent_id),
                path: config_path.clone(),
                kind: DwApplicationPackAssetKind::Config,
                content_type: Some("application/json".to_string()),
                applies_to_agents: applies_to_agents.clone(),
                source_ref: agent.source_provenance.template_source_ref.clone(),
            });

            generated_configs.push(DwGeneratedConfigAsset {
                asset_id: format!("generated.config.{}", agent.agent_id),
                path: config_path,
                format: "json".to_string(),
                applies_to_agents: applies_to_agents.clone(),
            });

            for binding in &agent.capability_bindings {
                requirements.push(DwApplicationPackRequirement {
                    requirement_id: format!(
                        "requirement.{}.{}",
                        agent.agent_id,
                        binding.capability_id.as_str()
                    ),
                    summary: format!(
                        "Capability `{}` bound to provider `{}`",
                        binding.capability_id.as_str(),
                        binding.provider_binding.provider_id
                    ),
                    capability_id: Some(binding.capability_id.clone()),
                    provider_id: Some(binding.provider_binding.provider_id.clone()),
                    applies_to_agents: applies_to_agents.clone(),
                });
            }
        }

        if multi_agent {
            assets.push(DwApplicationPackAsset {
                asset_id: "asset.config.application".to_string(),
                path: "shared/application.json".to_string(),
                kind: DwApplicationPackAssetKind::Config,
                content_type: Some("application/json".to_string()),
                applies_to_agents: self
                    .agents
                    .iter()
                    .map(|agent| agent.agent_id.clone())
                    .collect(),
                source_ref: None,
            });

            generated_configs.push(DwGeneratedConfigAsset {
                asset_id: "generated.config.application".to_string(),
                path: "shared/application.json".to_string(),
                format: "json".to_string(),
                applies_to_agents: self
                    .agents
                    .iter()
                    .map(|agent| agent.agent_id.clone())
                    .collect(),
            });
        }

        Ok(DwApplicationPackSpec {
            metadata: DwApplicationPackMetadata {
                pack_id,
                application_id: self.application.application_id.clone(),
                display_name: self.application.display_name.clone(),
                version: self.application.version.clone(),
                multi_agent,
            },
            agents,
            assets,
            generated_configs,
            generated_flows: Vec::new(),
            generated_prompts: Vec::new(),
            requirements,
            dependency_pack_refs,
            setup_requirements: self.unresolved_setup_items.clone(),
            layout: DwApplicationPackLayout {
                app_root: format!("{}.pack", self.application.application_id),
                shared_asset_roots: if multi_agent {
                    vec!["shared".to_string()]
                } else {
                    Vec::new()
                },
                layout_hint: Some(if multi_agent {
                    ApplicationPackLayoutHints::MultiAgentSharedProviders
                } else {
                    ApplicationPackLayoutHints::SingleAgentPack
                }),
            },
        })
    }
}
