use greentic_dw_delegation::{SubtaskEnvelope, SubtaskResultEnvelope};
use greentic_dw_engine::DwEngine;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{CallableWorkerRegistryError, DwRuntime, RuntimeError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgenticFinalResponseSource {
    pub tool_id: String,
    pub subtask_id: String,
    pub output_artifact_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgenticFinalResponse {
    pub run_id: String,
    pub finalizer_agent_id: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<AgenticFinalResponseSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeFinalResponseRequest {
    pub run_id: String,
    pub finalizer_agent_id: String,
    pub content: String,
    pub worker_results: Vec<(SubtaskEnvelope, SubtaskResultEnvelope)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FinalResponseValidationError {
    #[error("final response content must not be empty")]
    EmptyContent,
    #[error("finalizer `{actual}` does not match expected `{expected}`")]
    FinalizerMismatch { expected: String, actual: String },
    #[error("final response must include at least one validated worker result")]
    NoWorkerResults,
    #[error("duplicate final response source for subtask `{0}`")]
    DuplicateSource(String),
}

impl<E: DwEngine> DwRuntime<E> {
    pub fn compose_final_response(
        &self,
        request: ComposeFinalResponseRequest,
    ) -> Result<AgenticFinalResponse, RuntimeError> {
        if request.content.trim().is_empty() {
            return Err(FinalResponseValidationError::EmptyContent.into());
        }
        if request.worker_results.is_empty() {
            return Err(FinalResponseValidationError::NoWorkerResults.into());
        }

        let registry = self
            .callable_worker_registry()
            .ok_or(CallableWorkerRegistryError::NotConfigured)?;
        let expected_finalizer = registry
            .finalizer_agent_id()
            .or_else(|| registry.coordinator_agent_id())
            .ok_or(CallableWorkerRegistryError::NotConfigured)?;
        if request.finalizer_agent_id != expected_finalizer {
            return Err(FinalResponseValidationError::FinalizerMismatch {
                expected: expected_finalizer.to_string(),
                actual: request.finalizer_agent_id,
            }
            .into());
        }

        let mut sources = Vec::with_capacity(request.worker_results.len());
        for (handoff, result) in request.worker_results {
            self.validate_worker_tool_handoff(&handoff)?;
            self.validate_worker_tool_result(&handoff, &result)?;
            if sources
                .iter()
                .any(|source: &AgenticFinalResponseSource| source.subtask_id == result.subtask_id)
            {
                return Err(
                    FinalResponseValidationError::DuplicateSource(result.subtask_id).into(),
                );
            }
            sources.push(AgenticFinalResponseSource {
                tool_id: result.tool_id,
                subtask_id: result.subtask_id,
                output_artifact_ref: result.output_artifact_ref,
            });
        }

        Ok(AgenticFinalResponse {
            run_id: request.run_id,
            finalizer_agent_id: request.finalizer_agent_id,
            content: request.content,
            sources,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallableWorkerRegistry, WorkerToolInvocationRequest};
    use greentic_dw_delegation::{
        DelegationDecision, DelegationError, DelegationHandle, DelegationMergeResult,
        DelegationMode, DelegationProvider, HandoffContextScope, HandoffReturnPolicy, MergePolicy,
        MergeSubtaskResultRequest, StartSubtaskRequest,
    };
    use greentic_dw_engine::{EngineDecision, StaticEngine};
    use greentic_dw_types::{AgentRoute, CallableWorkerTool, InterAgentRoutingConfig};

    struct NoopDelegator;

    impl DelegationProvider for NoopDelegator {
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

    fn worker_result_pair(
        runtime: &DwRuntime<StaticEngine>,
    ) -> (SubtaskEnvelope, SubtaskResultEnvelope) {
        let invocation = runtime
            .start_worker_tool_handoff(
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
                },
                &NoopDelegator,
            )
            .expect("handoff starts");

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
        (invocation.envelope, result)
    }

    #[test]
    fn composes_final_response_from_validated_worker_results() {
        let runtime = runtime();
        let pair = worker_result_pair(&runtime);

        let response = runtime
            .compose_final_response(ComposeFinalResponseRequest {
                run_id: "run-1".to_string(),
                finalizer_agent_id: "coordinator".to_string(),
                content: "Traffic is within expected thresholds.".to_string(),
                worker_results: vec![pair],
            })
            .expect("final response should compose");

        assert_eq!(response.finalizer_agent_id, "coordinator");
        assert_eq!(response.sources.len(), 1);
        assert_eq!(response.sources[0].tool_id, "traffic_analysis");
    }

    #[test]
    fn rejects_final_response_from_wrong_finalizer() {
        let runtime = runtime();
        let pair = worker_result_pair(&runtime);

        assert!(matches!(
            runtime.compose_final_response(ComposeFinalResponseRequest {
                run_id: "run-1".to_string(),
                finalizer_agent_id: "traffic-specialist".to_string(),
                content: "Traffic is fine.".to_string(),
                worker_results: vec![pair],
            }),
            Err(RuntimeError::FinalResponse(
                FinalResponseValidationError::FinalizerMismatch { .. }
            ))
        ));
    }

    #[test]
    fn rejects_final_response_without_worker_results() {
        let runtime = runtime();

        assert!(matches!(
            runtime.compose_final_response(ComposeFinalResponseRequest {
                run_id: "run-1".to_string(),
                finalizer_agent_id: "coordinator".to_string(),
                content: "No evidence.".to_string(),
                worker_results: vec![],
            }),
            Err(RuntimeError::FinalResponse(
                FinalResponseValidationError::NoWorkerResults
            ))
        ));
    }
}
