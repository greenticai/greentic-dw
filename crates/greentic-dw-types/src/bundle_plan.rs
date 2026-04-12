use crate::{DwApplicationPackMaterializationError, DwCompositionDocument, PackSourceRef};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// Coarse inclusion category used by bundle plans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BundlePackKind {
    GeneratedApplicationPack,
    ProviderPack,
    SupportPack,
}

/// Normalized source and version information used by `greentic-bundle`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BundleSourceResolution {
    pub source_ref: PackSourceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
}

/// Reference to the generated DW application pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GeneratedAppPackRef {
    pub pack_id: String,
    pub resolution: BundleSourceResolution,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applies_to_agents: Vec<String>,
}

/// Reference to a provider pack selected during composition or materialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProviderPackRef {
    pub pack_id: String,
    pub provider_id: String,
    pub resolution: BundleSourceResolution,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applies_to_agents: Vec<String>,
}

/// Reference to a support pack required by templates or shared app layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SupportPackRef {
    pub pack_id: String,
    pub resolution: BundleSourceResolution,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applies_to_agents: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

/// One concrete inclusion item to hand to `greentic-bundle`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BundlePackInclusion {
    pub inclusion_id: String,
    pub pack_id: String,
    pub kind: BundlePackKind,
    pub resolution: BundleSourceResolution,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applies_to_agents: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

/// Formal handoff contract from DW planning/materialization into `greentic-bundle`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwBundlePlan {
    pub application_id: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multi_agent: bool,
    pub generated_app_pack: GeneratedAppPackRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_packs: Vec<ProviderPackRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub support_packs: Vec<SupportPackRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inclusions: Vec<BundlePackInclusion>,
}

#[derive(Debug, Error)]
pub enum DwBundlePlanGenerationError {
    #[error(transparent)]
    PackMaterialization(#[from] DwApplicationPackMaterializationError),
    #[error(transparent)]
    SourceRef(#[from] crate::SourceRefError),
}

impl DwCompositionDocument {
    /// Generate the final `greentic-bundle` pack list from the resolved composition.
    pub fn to_bundle_plan(&self) -> Result<DwBundlePlan, DwBundlePlanGenerationError> {
        let pack_spec = self.to_application_pack_spec()?;

        let generated_app_pack = GeneratedAppPackRef {
            pack_id: pack_spec.metadata.pack_id.clone(),
            resolution: BundleSourceResolution {
                source_ref: PackSourceRef::from_raw(format!("./{}", pack_spec.layout.app_root))?,
                version: pack_spec.metadata.version.clone(),
                channel: None,
            },
            applies_to_agents: sorted_unique(
                pack_spec
                    .agents
                    .iter()
                    .map(|agent| agent.agent_id.clone())
                    .collect(),
            ),
        };

        let mut provider_packs_by_id: BTreeMap<String, ProviderPackRef> = BTreeMap::new();
        let mut support_packs_by_id: BTreeMap<String, SupportPackRef> = BTreeMap::new();
        let support_rationale_by_id = self
            .bundle_plan
            .iter()
            .map(|entry| (entry.pack_id.clone(), entry.rationale.clone()))
            .collect::<BTreeMap<_, _>>();

        for dependency in &pack_spec.dependency_pack_refs {
            if let Some(provider_id) = &dependency.provider_id {
                let entry = provider_packs_by_id
                    .entry(dependency.pack_id.clone())
                    .or_insert_with(|| ProviderPackRef {
                        pack_id: dependency.pack_id.clone(),
                        provider_id: provider_id.clone(),
                        resolution: BundleSourceResolution {
                            source_ref: dependency.source_ref.clone(),
                            version: dependency.version.clone(),
                            channel: None,
                        },
                        applies_to_agents: Vec::new(),
                    });
                entry
                    .applies_to_agents
                    .extend(dependency.applies_to_agents.clone());
                entry.applies_to_agents =
                    sorted_unique(std::mem::take(&mut entry.applies_to_agents));
            } else {
                let entry = support_packs_by_id
                    .entry(dependency.pack_id.clone())
                    .or_insert_with(|| SupportPackRef {
                        pack_id: dependency.pack_id.clone(),
                        resolution: BundleSourceResolution {
                            source_ref: dependency.source_ref.clone(),
                            version: dependency.version.clone(),
                            channel: None,
                        },
                        applies_to_agents: Vec::new(),
                        rationale: support_rationale_by_id
                            .get(&dependency.pack_id)
                            .cloned()
                            .flatten(),
                    });
                entry
                    .applies_to_agents
                    .extend(dependency.applies_to_agents.clone());
                entry.applies_to_agents =
                    sorted_unique(std::mem::take(&mut entry.applies_to_agents));
            }
        }

        let provider_packs = provider_packs_by_id.into_values().collect::<Vec<_>>();
        let support_packs = support_packs_by_id.into_values().collect::<Vec<_>>();

        let mut inclusions = Vec::new();
        inclusions.push(BundlePackInclusion {
            inclusion_id: "include.generated_app_pack".to_string(),
            pack_id: generated_app_pack.pack_id.clone(),
            kind: BundlePackKind::GeneratedApplicationPack,
            resolution: generated_app_pack.resolution.clone(),
            applies_to_agents: generated_app_pack.applies_to_agents.clone(),
            rationale: Some("Generated application pack".to_string()),
        });
        inclusions.extend(provider_packs.iter().map(|provider| BundlePackInclusion {
            inclusion_id: format!("include.provider.{}", provider.pack_id),
            pack_id: provider.pack_id.clone(),
            kind: BundlePackKind::ProviderPack,
            resolution: provider.resolution.clone(),
            applies_to_agents: provider.applies_to_agents.clone(),
            rationale: Some(format!("Provider dependency `{}`", provider.provider_id)),
        }));
        inclusions.extend(support_packs.iter().map(|support| BundlePackInclusion {
            inclusion_id: format!("include.support.{}", support.pack_id),
            pack_id: support.pack_id.clone(),
            kind: BundlePackKind::SupportPack,
            resolution: support.resolution.clone(),
            applies_to_agents: support.applies_to_agents.clone(),
            rationale: support.rationale.clone(),
        }));

        Ok(DwBundlePlan {
            application_id: self.application.application_id.clone(),
            multi_agent: pack_spec.metadata.multi_agent,
            generated_app_pack,
            provider_packs,
            support_packs,
            inclusions,
        })
    }
}

fn sorted_unique(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}
