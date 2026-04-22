use crate::TemplateSourceRef;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Source that contributed a question block into the assembled wizard flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuestionSource {
    DwCore,
    Template { template_id: String },
    Provider { provider_id: String },
    Composition,
    Packaging,
}

/// Lifecycle phase in which a question becomes meaningful.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuestionPhase {
    Design,
    Setup,
    Runtime,
}

/// Hosted design-flow depth selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuestionDepthMode {
    Recommended,
    ReviewAll,
}

/// Visibility policy for an assembled question block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuestionVisibility {
    Required,
    Optional,
    ReviewAll,
    HiddenUnlessNeeded,
}

/// Scope to which a question block applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QuestionScope {
    Application,
    SharedComposition,
    Agent {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_id: Option<String>,
    },
    Provider {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_id: Option<String>,
    },
}

impl QuestionVisibility {
    pub fn visible_in(self, depth: QuestionDepthMode, dependency_required: bool) -> bool {
        match self {
            Self::Required => true,
            Self::Optional => matches!(depth, QuestionDepthMode::ReviewAll),
            Self::ReviewAll => matches!(depth, QuestionDepthMode::ReviewAll),
            Self::HiddenUnlessNeeded => dependency_required,
        }
    }
}

/// One assembled question block in the wizard flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwWizardQuestionBlock {
    pub block_id: String,
    pub source: QuestionSource,
    pub owner: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    pub scope: QuestionScope,
    pub phase: QuestionPhase,
    pub visibility: QuestionVisibility,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<TemplateSourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

impl DwWizardQuestionBlock {
    pub fn is_visible_in(
        &self,
        phase: QuestionPhase,
        depth: QuestionDepthMode,
        dependency_required: bool,
    ) -> bool {
        self.phase == phase && self.visibility.visible_in(depth, dependency_required)
    }
}

/// Canonical assembled question graph for the hosted design flow and later setup/runtime consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwWizardQuestionAssembly {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<DwWizardQuestionBlock>,
}

impl DwWizardQuestionAssembly {
    pub fn blocks_for(
        &self,
        phase: QuestionPhase,
        depth: QuestionDepthMode,
        dependency_required: bool,
    ) -> Vec<&DwWizardQuestionBlock> {
        self.blocks
            .iter()
            .filter(|block| block.is_visible_in(phase, depth, dependency_required))
            .collect()
    }
}
