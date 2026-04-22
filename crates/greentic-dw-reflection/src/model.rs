use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Accept,
    Revise,
    Retry,
    Delegate,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewTargetKind {
    PlanStep,
    Artifact,
    Agent,
    FinalOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReviewTarget {
    pub kind: ReviewTargetKind,
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReviewFinding {
    pub code: String,
    pub message: String,
    pub target: ReviewTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SuggestedAction {
    pub action: String,
    pub target: ReviewTarget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReviewOutcome {
    pub verdict: ReviewVerdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
    #[serde(default)]
    pub findings: Vec<ReviewFinding>,
    #[serde(default)]
    pub suggested_actions: Vec<SuggestedAction>,
    #[serde(default)]
    pub binding: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReviewStepRequest {
    pub plan_step_id: String,
    pub output_artifact_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReviewPlanRequest {
    pub plan_id: String,
    pub revision: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReviewFinalRequest {
    pub run_id: String,
    pub output_artifact_ref: String,
}

impl ReviewOutcome {
    pub fn validate(&self) -> Result<(), crate::ReflectionError> {
        if let Some(score) = self.score
            && !(0.0..=1.0).contains(&score)
        {
            return Err(crate::ReflectionError::Validation(
                "review score must be between 0.0 and 1.0".to_string(),
            ));
        }
        for finding in &self.findings {
            if finding.code.trim().is_empty() || finding.message.trim().is_empty() {
                return Err(crate::ReflectionError::Validation(
                    "review findings must include code and message".to_string(),
                ));
            }
            if finding.target.reference.trim().is_empty() {
                return Err(crate::ReflectionError::Validation(
                    "review finding target reference must not be empty".to_string(),
                ));
            }
        }
        for action in &self.suggested_actions {
            if action.action.trim().is_empty() || action.target.reference.trim().is_empty() {
                return Err(crate::ReflectionError::Validation(
                    "suggested actions must include an action and target reference".to_string(),
                ));
            }
        }
        Ok(())
    }
}
