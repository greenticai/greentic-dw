use crate::{DelegationDecision, DelegationMode, MergePolicy, SubtaskEnvelope};

pub fn delegation_decision_fixture() -> DelegationDecision {
    DelegationDecision {
        mode: DelegationMode::Single,
        target_agents: vec!["researcher".to_string()],
        merge_policy: MergePolicy::FirstSuccess,
        rationale: "specialist has the right context".to_string(),
    }
}

pub fn subtask_envelope_fixture() -> SubtaskEnvelope {
    SubtaskEnvelope {
        subtask_id: "subtask-1".to_string(),
        parent_run_id: "run-1".to_string(),
        target_agent: "researcher".to_string(),
        goal: "collect evidence".to_string(),
        context_package_ref: "artifact://context-1".to_string(),
        expected_output_schema: "schema://evidence".to_string(),
        permissions_profile: "restricted".to_string(),
        deadline: "2026-04-15T01:00:00Z".to_string(),
        return_policy: "sync".to_string(),
    }
}
