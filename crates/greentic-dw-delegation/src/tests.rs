use crate::{
    DelegationDecision, DelegationError, DelegationMode, MergePolicy, delegation_decision_fixture,
    subtask_envelope_fixture, subtask_result_envelope_fixture, validate_decision,
    validate_subtask_envelope, validate_subtask_result_envelope,
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
fn validates_subtask_result_envelope_fixture() {
    validate_subtask_result_envelope(&subtask_result_envelope_fixture())
        .expect("result fixture should validate");
}

#[test]
fn rejects_missing_tool_id_for_worker_handoff() {
    let mut envelope = subtask_envelope_fixture();
    envelope.tool_id.clear();

    let err = validate_subtask_envelope(&envelope).expect_err("missing tool id should fail");
    assert_eq!(
        err,
        DelegationError::Validation("tool_id must not be empty".to_string())
    );
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
