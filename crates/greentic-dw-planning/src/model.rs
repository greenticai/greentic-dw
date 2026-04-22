use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanStepKind {
    Research,
    ToolCall,
    Delegate,
    Review,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanStepStatus {
    Pending,
    Ready,
    Running,
    Blocked,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Draft,
    Active,
    Revising,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompletionState {
    Incomplete,
    Satisfied,
    Unsatisfied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanStep {
    pub step_id: String,
    pub title: String,
    pub kind: PlanStepKind,
    pub status: PlanStepStatus,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs_schema_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema_ref: Option<String>,
    #[serde(default)]
    pub retry_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanEdge {
    pub from_step_id: String,
    pub to_step_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanRevision {
    pub revision: u32,
    pub reason: String,
    #[serde(default)]
    pub changed_step_ids: Vec<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlannedAction {
    pub step_id: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanDocument {
    pub plan_id: String,
    pub goal: String,
    pub status: PlanStatus,
    pub revision: u32,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub success_criteria: Vec<String>,
    #[serde(default)]
    pub steps: Vec<PlanStep>,
    #[serde(default)]
    pub edges: Vec<PlanEdge>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreatePlanRequest {
    pub goal: String,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub success_criteria: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RevisePlanRequest {
    pub plan: PlanDocument,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NextActionsRequest {
    pub plan: PlanDocument,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StepResultRequest {
    pub plan: PlanDocument,
    pub step_id: String,
    pub status: PlanStepStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CompletionCheckRequest {
    pub plan: PlanDocument,
}
