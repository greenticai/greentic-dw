use crate::{PlanDocument, PlanEdge, PlanStatus, PlanStep, PlanStepKind, PlanStepStatus};
use std::collections::BTreeMap;

pub fn basic_plan_fixture() -> PlanDocument {
    PlanDocument {
        plan_id: "plan-basic".to_string(),
        goal: "Investigate and summarize an issue".to_string(),
        status: PlanStatus::Active,
        revision: 1,
        assumptions: vec!["logs are available".to_string()],
        constraints: vec!["stay deterministic".to_string()],
        success_criteria: vec!["deliver a summary".to_string()],
        steps: vec![
            PlanStep {
                step_id: "research".to_string(),
                title: "Collect evidence".to_string(),
                kind: PlanStepKind::Research,
                status: PlanStepStatus::Ready,
                depends_on: vec![],
                assigned_agent: None,
                inputs_schema_ref: None,
                output_schema_ref: Some("schema://evidence".to_string()),
                retry_count: 0,
            },
            PlanStep {
                step_id: "review".to_string(),
                title: "Review findings".to_string(),
                kind: PlanStepKind::Review,
                status: PlanStepStatus::Pending,
                depends_on: vec!["research".to_string()],
                assigned_agent: Some("reviewer".to_string()),
                inputs_schema_ref: Some("schema://evidence".to_string()),
                output_schema_ref: Some("schema://report".to_string()),
                retry_count: 0,
            },
        ],
        edges: vec![PlanEdge {
            from_step_id: "research".to_string(),
            to_step_id: "review".to_string(),
            condition: None,
        }],
        metadata: BTreeMap::new(),
    }
}
