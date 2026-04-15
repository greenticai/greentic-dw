use crate::{
    PlanStepStatus, PlanningError, basic_plan_fixture, plan_document_schema_json, validate_plan,
    validate_step_transition,
};

#[test]
fn validates_basic_fixture() {
    validate_plan(&basic_plan_fixture()).expect("fixture should validate");
}

#[test]
fn rejects_duplicate_step_ids() {
    let mut plan = basic_plan_fixture();
    plan.steps[1].step_id = "research".to_string();

    let err = validate_plan(&plan).expect_err("duplicate step ids should fail");
    assert_eq!(
        err,
        PlanningError::Validation("duplicate step_id `research`".to_string())
    );
}

#[test]
fn validates_step_transition_rules() {
    validate_step_transition(&PlanStepStatus::Ready, &PlanStepStatus::Running)
        .expect("ready -> running should pass");
    let err = validate_step_transition(&PlanStepStatus::Completed, &PlanStepStatus::Running)
        .expect_err("completed -> running should fail");
    assert!(matches!(err, PlanningError::Validation(_)));
}

#[test]
fn exports_plan_schema() {
    let schema = plan_document_schema_json();
    assert!(schema.contains("PlanDocument"));
}
