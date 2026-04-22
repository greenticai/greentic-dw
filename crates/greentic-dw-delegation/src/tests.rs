use crate::{
    DelegationDecision, DelegationError, DelegationMode, MergePolicy, delegation_decision_fixture,
    subtask_envelope_fixture, validate_decision, validate_subtask_envelope,
};

#[test]
fn validates_decision_fixture() {
    validate_decision(&delegation_decision_fixture()).expect("fixture should validate");
}

#[test]
fn validates_subtask_envelope_fixture() {
    validate_subtask_envelope(&subtask_envelope_fixture()).expect("fixture should validate");
}

#[test]
fn rejects_missing_targets_for_active_mode() {
    let decision = DelegationDecision {
        mode: DelegationMode::Parallel,
        target_agents: vec![],
        merge_policy: MergePolicy::CollectAll,
        rationale: "fan out".to_string(),
    };
    let err = validate_decision(&decision).expect_err("missing targets should fail");
    assert_eq!(
        err,
        DelegationError::Validation("delegation target_agents must not be empty".to_string())
    );
}
