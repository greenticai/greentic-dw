use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Note,
    Draft,
    Evidence,
    ToolOutput,
    PromptFragment,
    Table,
    ReportSection,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceScope {
    pub tenant: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
    pub session: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub run: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactRef {
    pub artifact_id: String,
    pub kind: ArtifactKind,
    pub scope: WorkspaceScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactMetadata {
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactVersion {
    pub artifact_id: String,
    pub version: u32,
    pub checksum: String,
    pub created_at: String,
    #[serde(default)]
    pub derived_from: Vec<ArtifactRef>,
    #[serde(default)]
    pub provenance: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactContent {
    pub artifact: ArtifactRef,
    pub metadata: ArtifactMetadata,
    pub version: ArtifactVersion,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactSummary {
    pub artifact: ArtifactRef,
    pub latest_version: ArtifactVersion,
    pub metadata: ArtifactMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateArtifactRequest {
    pub artifact: ArtifactRef,
    pub metadata: ArtifactMetadata,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReadArtifactRequest {
    pub artifact_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateArtifactRequest {
    pub artifact_id: String,
    pub body: String,
    #[serde(default)]
    pub derived_from: Vec<ArtifactRef>,
    #[serde(default)]
    pub provenance: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListArtifactsRequest {
    pub scope: WorkspaceScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LinkArtifactsRequest {
    pub from_artifact_id: String,
    pub to_artifact_id: String,
    pub relation: String,
}
