use crate::{
    DelegationDecision, DelegationMode, HandoffContextScope, HandoffReturnPolicy, MergePolicy,
    SubtaskEnvelope, SubtaskResultEnvelope,
};

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
        correlation_id: "corr-1".to_string(),
        source_agent_id: "coordinator".to_string(),
        target_agent: "researcher".to_string(),
        tool_id: "research_evidence".to_string(),
        goal: "collect evidence".to_string(),
        context_package_ref: "artifact://context-1".to_string(),
        context_scope: HandoffContextScope::ParentTaskOnly,
        expected_output_schema: "schema://evidence".to_string(),
        permissions_profile: "restricted".to_string(),
        deadline: "2026-04-15T01:00:00Z".to_string(),
        return_policy: HandoffReturnPolicy::Sync,
    }
}

pub fn subtask_result_envelope_fixture() -> SubtaskResultEnvelope {
    SubtaskResultEnvelope {
        subtask_id: "subtask-1".to_string(),
        correlation_id: "corr-1".to_string(),
        source_agent_id: "researcher".to_string(),
        target_agent_id: "coordinator".to_string(),
        tool_id: "research_evidence".to_string(),
        status: "completed".to_string(),
        output_artifact_ref: "artifact://evidence-1".to_string(),
        output_schema_ref: "schema://evidence".to_string(),
        notes: vec!["evidence collected".to_string()],
    }
}
