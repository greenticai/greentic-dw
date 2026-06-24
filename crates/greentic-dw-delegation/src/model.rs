use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DelegationMode {
    None,
    Single,
    Parallel,
    MapReduce,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MergePolicy {
    FirstSuccess,
    CollectAll,
    MajorityVote,
    WeightedMerge,
    ReducerArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DelegationDecision {
    pub mode: DelegationMode,
    #[serde(default)]
    pub target_agents: Vec<String>,
    pub merge_policy: MergePolicy,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandoffContextScope {
    #[default]
    ExplicitPackageOnly,
    ParentTaskOnly,
    SharedMemoryAndArtifacts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandoffReturnPolicy {
    #[default]
    Sync,
    FirstReturn,
    CollectAll,
    Async,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SubtaskEnvelope {
    pub subtask_id: String,
    pub parent_run_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub correlation_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_agent_id: String,
    pub target_agent: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tool_id: String,
    pub goal: String,
    pub context_package_ref: String,
    #[serde(default)]
    pub context_scope: HandoffContextScope,
    pub expected_output_schema: String,
    pub permissions_profile: String,
    pub deadline: String,
    #[serde(default)]
    pub return_policy: HandoffReturnPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SubtaskResultEnvelope {
    pub subtask_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub correlation_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_agent_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target_agent_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tool_id: String,
    pub status: String,
    pub output_artifact_ref: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub output_schema_ref: String,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DelegationHandle {
    pub subtask_id: String,
    pub target_agent: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DelegationMergeResult {
    pub accepted_artifact_refs: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DelegationRequest {
    pub goal: String,
    #[serde(default)]
    pub candidate_agents: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StartSubtaskRequest {
    pub envelope: SubtaskEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MergeSubtaskResultRequest {
    pub merge_policy: MergePolicy,
    #[serde(default)]
    pub results: Vec<SubtaskResultEnvelope>,
}
