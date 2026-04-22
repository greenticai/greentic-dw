use crate::{DelegationDecision, DelegationError, DelegationMode, SubtaskEnvelope};

pub fn validate_decision(decision: &DelegationDecision) -> Result<(), DelegationError> {
    if decision.rationale.trim().is_empty() {
        return Err(DelegationError::Validation(
            "delegation rationale must not be empty".to_string(),
        ));
    }
    match decision.mode {
        DelegationMode::None => {
            if !decision.target_agents.is_empty() {
                return Err(DelegationError::Validation(
                    "delegation mode `none` cannot include target agents".to_string(),
                ));
            }
        }
        _ => {
            if decision.target_agents.is_empty() {
                return Err(DelegationError::Validation(
                    "delegation target_agents must not be empty".to_string(),
                ));
            }
        }
    }
    Ok(())
}

pub fn validate_subtask_envelope(envelope: &SubtaskEnvelope) -> Result<(), DelegationError> {
    for (label, value) in [
        ("subtask_id", envelope.subtask_id.as_str()),
        ("parent_run_id", envelope.parent_run_id.as_str()),
        ("target_agent", envelope.target_agent.as_str()),
        ("goal", envelope.goal.as_str()),
        ("context_package_ref", envelope.context_package_ref.as_str()),
        (
            "expected_output_schema",
            envelope.expected_output_schema.as_str(),
        ),
        ("permissions_profile", envelope.permissions_profile.as_str()),
        ("deadline", envelope.deadline.as_str()),
        ("return_policy", envelope.return_policy.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(DelegationError::Validation(format!(
                "{label} must not be empty"
            )));
        }
    }
    Ok(())
}
