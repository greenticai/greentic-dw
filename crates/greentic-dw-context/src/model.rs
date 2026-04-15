use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextFragmentKind {
    MemoryItem,
    WorkspaceArtifact,
    PlanStep,
    RuntimeMetadata,
    PromptFragment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContextFragment {
    pub fragment_id: String,
    pub kind: ContextFragmentKind,
    pub content_ref: String,
    pub provenance: String,
    pub ordinal: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContextBudget {
    pub max_fragments: u32,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContextPackage {
    pub package_id: String,
    #[serde(default)]
    pub fragments: Vec<ContextFragment>,
    pub budget: ContextBudget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CompressedContext {
    pub source_package_id: String,
    pub compressed_artifact_ref: String,
    pub fragment_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SummaryArtifactRef {
    pub artifact_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BuildContextRequest {
    #[serde(default)]
    pub fragment_refs: Vec<String>,
    pub budget: ContextBudget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CompressContextRequest {
    pub package: ContextPackage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SummarizeContextRequest {
    pub package: ContextPackage,
}
