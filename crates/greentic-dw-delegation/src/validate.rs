use crate::{
    DelegationDecision, DelegationError, DelegationMode, SubtaskEnvelope, SubtaskResultEnvelope,
};

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
        ("correlation_id", envelope.correlation_id.as_str()),
        ("source_agent_id", envelope.source_agent_id.as_str()),
        ("target_agent", envelope.target_agent.as_str()),
        ("tool_id", envelope.tool_id.as_str()),
        ("goal", envelope.goal.as_str()),
        ("context_package_ref", envelope.context_package_ref.as_str()),
        (
            "expected_output_schema",
            envelope.expected_output_schema.as_str(),
        ),
        ("permissions_profile", envelope.permissions_profile.as_str()),
        ("deadline", envelope.deadline.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(DelegationError::Validation(format!(
                "{label} must not be empty"
            )));
        }
    }
    Ok(())
}

pub fn validate_subtask_result_envelope(
    envelope: &SubtaskResultEnvelope,
) -> Result<(), DelegationError> {
    for (label, value) in [
        ("subtask_id", envelope.subtask_id.as_str()),
        ("correlation_id", envelope.correlation_id.as_str()),
        ("source_agent_id", envelope.source_agent_id.as_str()),
        ("target_agent_id", envelope.target_agent_id.as_str()),
        ("tool_id", envelope.tool_id.as_str()),
        ("status", envelope.status.as_str()),
        ("output_artifact_ref", envelope.output_artifact_ref.as_str()),
        ("output_schema_ref", envelope.output_schema_ref.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(DelegationError::Validation(format!(
                "{label} must not be empty"
            )));
        }
    }
    Ok(())
}
