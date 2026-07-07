use crate::{
    PlanEdge, PlanStepStatus, PlanningError, basic_plan_fixture, plan_document_schema_json,
    validate_plan, validate_step_transition,
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
fn rejects_empty_plan_id() {
    let mut plan = basic_plan_fixture();
    plan.plan_id = "   ".to_string();

    let err = validate_plan(&plan).expect_err("empty plan_id should fail");
    assert_eq!(
        err,
        PlanningError::Validation("plan_id must not be empty".to_string())
    );
}

#[test]
fn rejects_empty_goal() {
    let mut plan = basic_plan_fixture();
    plan.goal = "".to_string();

    let err = validate_plan(&plan).expect_err("empty goal should fail");
    assert_eq!(
        err,
        PlanningError::Validation("goal must not be empty".to_string())
    );
}

#[test]
fn rejects_empty_success_criteria() {
    let mut plan = basic_plan_fixture();
    plan.success_criteria.clear();

    let err = validate_plan(&plan).expect_err("empty success_criteria should fail");
    assert_eq!(
        err,
        PlanningError::Validation("success_criteria must not be empty".to_string())
    );
}

#[test]
fn rejects_empty_step_id() {
    let mut plan = basic_plan_fixture();
    plan.steps[0].step_id = " ".to_string();

    let err = validate_plan(&plan).expect_err("empty step_id should fail");
    assert_eq!(
        err,
        PlanningError::Validation("step_id must not be empty".to_string())
    );
}

#[test]
fn rejects_empty_step_title() {
    let mut plan = basic_plan_fixture();
    plan.steps[0].title = "".to_string();

    let err = validate_plan(&plan).expect_err("empty title should fail");
    assert_eq!(
        err,
        PlanningError::Validation("step `research` title must not be empty".to_string())
    );
}

#[test]
fn rejects_empty_dependency() {
    let mut plan = basic_plan_fixture();
    plan.steps[1].depends_on = vec!["".to_string()];

    let err = validate_plan(&plan).expect_err("empty dependency should fail");
    assert_eq!(
        err,
        PlanningError::Validation("step `review` has an empty dependency".to_string())
    );
}

#[test]
fn rejects_unknown_dependency() {
    let mut plan = basic_plan_fixture();
    plan.steps[1].depends_on = vec!["missing".to_string()];

    let err = validate_plan(&plan).expect_err("unknown dependency should fail");
    assert_eq!(
        err,
        PlanningError::Validation("step `review` depends on unknown step `missing`".to_string())
    );
}

#[test]
fn rejects_self_dependency() {
    let mut plan = basic_plan_fixture();
    plan.steps[1].depends_on = vec!["review".to_string()];

    let err = validate_plan(&plan).expect_err("self-dependency should fail");
    assert_eq!(
        err,
        PlanningError::Validation("step `review` cannot depend on itself".to_string())
    );
}

#[test]
fn rejects_edge_with_unknown_from_step() {
    let mut plan = basic_plan_fixture();
    plan.edges = vec![PlanEdge {
        from_step_id: "ghost".to_string(),
        to_step_id: "review".to_string(),
        condition: None,
    }];

    let err = validate_plan(&plan).expect_err("edge with unknown from should fail");
    assert_eq!(
        err,
        PlanningError::Validation("edge references unknown from_step_id `ghost`".to_string())
    );
}

#[test]
fn rejects_edge_with_unknown_to_step() {
    let mut plan = basic_plan_fixture();
    plan.edges = vec![PlanEdge {
        from_step_id: "research".to_string(),
        to_step_id: "ghost".to_string(),
        condition: None,
    }];

    let err = validate_plan(&plan).expect_err("edge with unknown to should fail");
    assert_eq!(
        err,
        PlanningError::Validation("edge references unknown to_step_id `ghost`".to_string())
    );
}

#[test]
fn rejects_self_edge() {
    let mut plan = basic_plan_fixture();
    plan.edges = vec![PlanEdge {
        from_step_id: "research".to_string(),
        to_step_id: "research".to_string(),
        condition: None,
    }];

    let err = validate_plan(&plan).expect_err("self-edge should fail");
    assert_eq!(
        err,
        PlanningError::Validation("edge cannot connect a step to itself".to_string())
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
fn accepts_all_documented_step_transitions() {
    let allowed = [
        (PlanStepStatus::Pending, PlanStepStatus::Ready),
        (PlanStepStatus::Ready, PlanStepStatus::Running),
        (PlanStepStatus::Running, PlanStepStatus::Completed),
        (PlanStepStatus::Running, PlanStepStatus::Failed),
        (PlanStepStatus::Running, PlanStepStatus::Blocked),
        (PlanStepStatus::Blocked, PlanStepStatus::Ready),
        (PlanStepStatus::Pending, PlanStepStatus::Skipped),
        (PlanStepStatus::Ready, PlanStepStatus::Skipped),
        (PlanStepStatus::Running, PlanStepStatus::Running),
        (PlanStepStatus::Completed, PlanStepStatus::Completed),
        (PlanStepStatus::Failed, PlanStepStatus::Failed),
        (PlanStepStatus::Skipped, PlanStepStatus::Skipped),
    ];
    for (prev, next) in allowed {
        validate_step_transition(&prev, &next)
            .unwrap_or_else(|err| panic!("{prev:?} -> {next:?} should pass: {err}"));
    }
}

#[test]
fn rejects_illegal_step_transitions() {
    let illegal = [
        (PlanStepStatus::Pending, PlanStepStatus::Running),
        (PlanStepStatus::Skipped, PlanStepStatus::Running),
        (PlanStepStatus::Failed, PlanStepStatus::Completed),
    ];
    for (prev, next) in illegal {
        let err = validate_step_transition(&prev, &next)
            .expect_err(&format!("{prev:?} -> {next:?} should be rejected"));
        assert!(matches!(err, PlanningError::Validation(_)));
    }
}

#[test]
fn exports_plan_schema() {
    let schema = plan_document_schema_json();
    assert!(schema.contains("PlanDocument"));
}
