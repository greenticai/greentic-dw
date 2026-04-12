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

/// Visibility policy for one assembled question block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ModeVisibilityPolicy {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub visible_in_default_mode: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub visible_in_personalised_mode: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required_in_default_mode: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_when_dependency_required: bool,
}

/// Filter settings for the compact default wizard mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DefaultModeFilter {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_only_required_without_defaults: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_dependency_required_questions: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_provider_defaults: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_setup_questions: bool,
}

/// Filter settings for the expanded personalised wizard mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PersonalisedModeFilter {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_optional_sections: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_provider_overrides: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_advanced_sections: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_packaging_options: bool,
}

/// One assembled question block in the wizard flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwWizardQuestionBlock {
    pub block_id: String,
    pub source: QuestionSource,
    pub visibility: ModeVisibilityPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<TemplateSourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// Canonical assembled question graph for both default and personalised wizard modes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DwWizardQuestionAssembly {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<DwWizardQuestionBlock>,
    #[serde(default)]
    pub default_mode_filter: DefaultModeFilter,
    #[serde(default)]
    pub personalised_mode_filter: PersonalisedModeFilter,
}
