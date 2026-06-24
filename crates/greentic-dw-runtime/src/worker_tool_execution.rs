use greentic_dw_delegation::{
    DelegationHandle, DelegationProvider, HandoffContextScope, HandoffReturnPolicy,
    StartSubtaskRequest, SubtaskEnvelope, SubtaskResultEnvelope, validate_subtask_envelope,
    validate_subtask_result_envelope,
};
use thiserror::Error;

use crate::{CallableWorkerRegistryError, DwRuntime, RuntimeError};
use greentic_dw_engine::DwEngine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerToolInvocationRequest {
    pub parent_run_id: String,
    pub correlation_id: String,
    pub source_agent_id: String,
    pub tool_id: String,
    pub goal: String,
    pub context_package_ref: String,
    pub context_scope: HandoffContextScope,
    pub permissions_profile: String,
    pub deadline: String,
    pub return_policy: HandoffReturnPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerToolInvocation {
    pub envelope: SubtaskEnvelope,
    pub handle: DelegationHandle,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorkerToolResultValidationError {
    #[error("worker result subtask id `{actual}` does not match handoff `{expected}`")]
    SubtaskIdMismatch { expected: String, actual: String },
    #[error("worker result correlation id `{actual}` does not match handoff `{expected}`")]
    CorrelationIdMismatch { expected: String, actual: String },
    #[error("worker result source agent `{actual}` does not match handoff target `{expected}`")]
    SourceAgentMismatch { expected: String, actual: String },
    #[error("worker result target agent `{actual}` does not match handoff source `{expected}`")]
    TargetAgentMismatch { expected: String, actual: String },
    #[error("worker result tool id `{actual}` does not match handoff `{expected}`")]
    ToolIdMismatch { expected: String, actual: String },
    #[error("worker result output schema `{actual}` does not match handoff `{expected}`")]
    OutputSchemaMismatch { expected: String, actual: String },
}

impl<E: DwEngine> DwRuntime<E> {
    pub fn start_worker_tool_handoff(
        &self,
        request: WorkerToolInvocationRequest,
        delegator: &dyn DelegationProvider,
    ) -> Result<WorkerToolInvocation, RuntimeError> {
        let registry = self
            .callable_worker_registry()
            .ok_or(CallableWorkerRegistryError::NotConfigured)?;
        let tool = registry
            .callable_worker(&request.tool_id)
            .ok_or_else(|| CallableWorkerRegistryError::UnknownTool(request.tool_id.clone()))?;

        let envelope = SubtaskEnvelope {
            subtask_id: format!("{}::{}", request.parent_run_id, request.tool_id),
            parent_run_id: request.parent_run_id,
            correlation_id: request.correlation_id,
            source_agent_id: request.source_agent_id,
            target_agent: tool.target_agent_id.clone(),
            tool_id: request.tool_id,
            goal: request.goal,
            context_package_ref: request.context_package_ref,
            context_scope: request.context_scope,
            expected_output_schema: tool.output_schema_ref.clone(),
            permissions_profile: request.permissions_profile,
            deadline: request.deadline,
            return_policy: request.return_policy,
        };

        validate_subtask_envelope(&envelope)?;
        self.validate_worker_tool_handoff(&envelope)?;
        let handle = delegator.start_subtask(StartSubtaskRequest {
            envelope: envelope.clone(),
        })?;

        Ok(WorkerToolInvocation { envelope, handle })
    }

    pub fn validate_worker_tool_result(
        &self,
        handoff: &SubtaskEnvelope,
        result: &SubtaskResultEnvelope,
    ) -> Result<(), RuntimeError> {
        validate_subtask_result_envelope(result)?;

        if result.subtask_id != handoff.subtask_id {
            return Err(WorkerToolResultValidationError::SubtaskIdMismatch {
                expected: handoff.subtask_id.clone(),
                actual: result.subtask_id.clone(),
            }
            .into());
        }
        if result.correlation_id != handoff.correlation_id {
            return Err(WorkerToolResultValidationError::CorrelationIdMismatch {
                expected: handoff.correlation_id.clone(),
                actual: result.correlation_id.clone(),
            }
            .into());
        }
        if result.source_agent_id != handoff.target_agent {
            return Err(WorkerToolResultValidationError::SourceAgentMismatch {
                expected: handoff.target_agent.clone(),
                actual: result.source_agent_id.clone(),
            }
            .into());
        }
        if result.target_agent_id != handoff.source_agent_id {
            return Err(WorkerToolResultValidationError::TargetAgentMismatch {
                expected: handoff.source_agent_id.clone(),
                actual: result.target_agent_id.clone(),
            }
            .into());
        }
        if result.tool_id != handoff.tool_id {
            return Err(WorkerToolResultValidationError::ToolIdMismatch {
                expected: handoff.tool_id.clone(),
                actual: result.tool_id.clone(),
            }
            .into());
        }
        if result.output_schema_ref != handoff.expected_output_schema {
            return Err(WorkerToolResultValidationError::OutputSchemaMismatch {
                expected: handoff.expected_output_schema.clone(),
                actual: result.output_schema_ref.clone(),
            }
            .into());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CallableWorkerRegistry;
    use greentic_dw_delegation::{
        DelegationDecision, DelegationError, DelegationMergeResult, DelegationMode, MergePolicy,
        MergeSubtaskResultRequest, SubtaskResultEnvelope,
    };
    use greentic_dw_engine::{EngineDecision, StaticEngine};
    use greentic_dw_types::{AgentRoute, CallableWorkerTool, InterAgentRoutingConfig};
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingDelegator {
        envelopes: Mutex<Vec<SubtaskEnvelope>>,
    }

    impl DelegationProvider for RecordingDelegator {
        fn choose_delegate(
            &self,
            _req: greentic_dw_delegation::DelegationRequest,
        ) -> Result<DelegationDecision, DelegationError> {
            Ok(DelegationDecision {
                mode: DelegationMode::Single,
                target_agents: vec!["traffic-specialist".to_string()],
                merge_policy: MergePolicy::FirstSuccess,
                rationale: "test".to_string(),
            })
        }

        fn start_subtask(
            &self,
            req: StartSubtaskRequest,
        ) -> Result<DelegationHandle, DelegationError> {
            let mut envelopes = self.envelopes.lock().expect("recording lock");
            envelopes.push(req.envelope.clone());
            Ok(DelegationHandle {
                subtask_id: req.envelope.subtask_id,
                target_agent: req.envelope.target_agent,
            })
        }

        fn merge_result(
            &self,
            _req: MergeSubtaskResultRequest,
        ) -> Result<DelegationMergeResult, DelegationError> {
            Ok(DelegationMergeResult {
                accepted_artifact_refs: vec![],
                summary: String::new(),
            })
        }
    }

    fn runtime() -> DwRuntime<StaticEngine> {
        let routing = InterAgentRoutingConfig {
            allowed_routes: Vec::new(),
            coordinator_agent_id: Some("coordinator".to_string()),
            finalizer_agent_id: Some("coordinator".to_string()),
            routes: vec![AgentRoute {
                from_agent_id: "coordinator".to_string(),
                to_agent_id: "traffic-specialist".to_string(),
                allowed: true,
            }],
            callable_workers: vec![CallableWorkerTool {
                tool_id: "traffic_analysis".to_string(),
                target_agent_id: "traffic-specialist".to_string(),
                description: "Analyze traffic".to_string(),
                input_schema_ref: "schema://telco-x/traffic-analysis-request.v1".to_string(),
                output_schema_ref: "schema://telco-x/traffic-analysis-result.v1".to_string(),
            }],
            shared_context_policy: None,
        };
        let registry = CallableWorkerRegistry::from_routing(&routing).expect("registry");
        DwRuntime::new(StaticEngine::new(EngineDecision::Noop))
            .with_callable_worker_registry(registry)
    }

    fn invocation_request() -> WorkerToolInvocationRequest {
        WorkerToolInvocationRequest {
            parent_run_id: "run-1".to_string(),
            correlation_id: "corr-1".to_string(),
            source_agent_id: "coordinator".to_string(),
            tool_id: "traffic_analysis".to_string(),
            goal: "check traffic".to_string(),
            context_package_ref: "context://traffic".to_string(),
            context_scope: HandoffContextScope::ParentTaskOnly,
            permissions_profile: "restricted".to_string(),
            deadline: "2026-04-16T00:00:00Z".to_string(),
            return_policy: HandoffReturnPolicy::Sync,
        }
    }

    #[test]
    fn starts_worker_tool_handoff_with_registry_derived_target_and_schema() {
        let runtime = runtime();
        let delegator = RecordingDelegator::default();

        let invocation = runtime
            .start_worker_tool_handoff(invocation_request(), &delegator)
            .expect("handoff should start");

        assert_eq!(invocation.handle.target_agent, "traffic-specialist");
        assert_eq!(invocation.envelope.subtask_id, "run-1::traffic_analysis");
        assert_eq!(invocation.envelope.target_agent, "traffic-specialist");
        assert_eq!(
            invocation.envelope.expected_output_schema,
            "schema://telco-x/traffic-analysis-result.v1"
        );
        let recorded = delegator.envelopes.lock().expect("recorded envelopes");
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0], invocation.envelope);
    }

    #[test]
    fn validates_worker_tool_result_against_handoff() {
        let runtime = runtime();
        let delegator = RecordingDelegator::default();
        let invocation = runtime
            .start_worker_tool_handoff(invocation_request(), &delegator)
            .expect("handoff should start");
        let result = SubtaskResultEnvelope {
            subtask_id: invocation.envelope.subtask_id.clone(),
            correlation_id: invocation.envelope.correlation_id.clone(),
            source_agent_id: invocation.envelope.target_agent.clone(),
            target_agent_id: invocation.envelope.source_agent_id.clone(),
            tool_id: invocation.envelope.tool_id.clone(),
            status: "completed".to_string(),
            output_artifact_ref: "artifact://traffic-result".to_string(),
            output_schema_ref: invocation.envelope.expected_output_schema.clone(),
            notes: vec![],
        };

        runtime
            .validate_worker_tool_result(&invocation.envelope, &result)
            .expect("result should validate");
    }

    #[test]
    fn rejects_worker_tool_result_with_wrong_tool_id() {
        let runtime = runtime();
        let delegator = RecordingDelegator::default();
        let invocation = runtime
            .start_worker_tool_handoff(invocation_request(), &delegator)
            .expect("handoff should start");
        let result = SubtaskResultEnvelope {
            subtask_id: invocation.envelope.subtask_id.clone(),
            correlation_id: invocation.envelope.correlation_id.clone(),
            source_agent_id: invocation.envelope.target_agent.clone(),
            target_agent_id: invocation.envelope.source_agent_id.clone(),
            tool_id: "bgp_analysis".to_string(),
            status: "completed".to_string(),
            output_artifact_ref: "artifact://traffic-result".to_string(),
            output_schema_ref: invocation.envelope.expected_output_schema.clone(),
            notes: vec![],
        };

        assert!(matches!(
            runtime.validate_worker_tool_result(&invocation.envelope, &result),
            Err(RuntimeError::WorkerToolResult(
                WorkerToolResultValidationError::ToolIdMismatch { .. }
            ))
        ));
    }
}
