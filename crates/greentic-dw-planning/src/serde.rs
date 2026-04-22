use crate::PlanDocument;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanSchemaSnapshot {
    pub schema_json: String,
}

pub fn plan_document_schema_json() -> String {
    serde_json::to_string_pretty(&schema_for!(PlanDocument))
        .expect("plan document schema should serialize")
}
