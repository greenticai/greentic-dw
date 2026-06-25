use greentic_dw_delegation::{
    DelegationProvider, HandoffContextScope, HandoffReturnPolicy, SubtaskResultEnvelope,
};
use greentic_dw_engine::DwEngine;
use thiserror::Error;

pub const EVENT_AGENTIC_COORDINATOR_STARTED: &str = "agentic.coordinator.started";
pub const EVENT_AGENTIC_WORKER_TOOL_HANDOFF_STARTED: &str = "agentic.worker_tool.handoff_started";
pub const EVENT_AGENTIC_WORKER_TOOL_RESULT_RECEIVED: &str = "agentic.worker_tool.result_received";
pub const EVENT_AGENTIC_FINAL_RESPONSE_CREATED: &str = "agentic.final_response.created";
pub const EVENT_AGENTIC_COORDINATOR_COMPLETED: &str = "agentic.coordinator.completed";

use crate::{
    AgenticFinalResponse, ComposeFinalResponseRequest, DwRuntime, RuntimeError,
    WorkerToolInvocation, WorkerToolInvocationRequest,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinatorWorkerToolCall {
    pub tool_id: String,
    pub goal: String,
    pub context_package_ref: String,
    pub context_scope: HandoffContextScope,
    pub permissions_profile: String,
    pub deadline: String,
    pub return_policy: HandoffReturnPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunAgenticCoordinatorRequest {
    pub run_id: String,
    pub coordinator_agent_id: String,
    pub finalizer_agent_id: String,
    pub final_response_content: String,
    pub tool_calls: Vec<CoordinatorWorkerToolCall>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgenticCoordinatorRun {
    pub invocations: Vec<WorkerToolInvocation>,
    pub worker_results: Vec<SubtaskResultEnvelope>,
    pub final_response: AgenticFinalResponse,
}

pub trait WorkerToolResultProvider {
    fn worker_tool_result(
        &self,
        invocation: &WorkerToolInvocation,
    ) -> Result<SubtaskResultEnvelope, WorkerToolRunError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CoordinatorFlowError {
    #[error("coordinator flow must include at least one worker-tool call")]
    NoToolCalls,
    #[error("coordinator run id must not be empty")]
    EmptyRunId,
    #[error("coordinator agent id must not be empty")]
    EmptyCoordinatorAgentId,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorkerToolRunError {
    #[error("worker tool provider error: {0}")]
    Provider(String),
}

impl<E: DwEngine> DwRuntime<E> {
    pub fn run_agentic_coordinator_flow(
        &self,
        request: RunAgenticCoordinatorRequest,
        delegator: &dyn DelegationProvider,
        worker_tool_results: &dyn WorkerToolResultProvider,
    ) -> Result<AgenticCoordinatorRun, RuntimeError> {
        if request.run_id.trim().is_empty() {
            return Err(CoordinatorFlowError::EmptyRunId.into());
        }
        if request.coordinator_agent_id.trim().is_empty() {
            return Err(CoordinatorFlowError::EmptyCoordinatorAgentId.into());
        }
        if request.tool_calls.is_empty() {
            return Err(CoordinatorFlowError::NoToolCalls.into());
        }

        self.emit_observer_event(
            request.run_id.clone(),
            request.coordinator_agent_id.clone(),
            EVENT_AGENTIC_COORDINATOR_STARTED,
        );

        let mut invocations = Vec::with_capacity(request.tool_calls.len());
        let mut worker_results = Vec::with_capacity(request.tool_calls.len());
        let mut final_pairs = Vec::with_capacity(request.tool_calls.len());

        for (index, call) in request.tool_calls.into_iter().enumerate() {
            let invocation = self.start_worker_tool_handoff(
                WorkerToolInvocationRequest {
                    parent_run_id: request.run_id.clone(),
                    correlation_id: format!("{}::{}::{}", request.run_id, call.tool_id, index),
                    source_agent_id: request.coordinator_agent_id.clone(),
                    tool_id: call.tool_id,
                    goal: call.goal,
                    context_package_ref: call.context_package_ref,
                    context_scope: call.context_scope,
                    permissions_profile: call.permissions_profile,
                    deadline: call.deadline,
                    return_policy: call.return_policy,
                },
                delegator,
            )?;
            self.emit_observer_event(
                request.run_id.clone(),
                invocation.envelope.source_agent_id.clone(),
                EVENT_AGENTIC_WORKER_TOOL_HANDOFF_STARTED,
            );
            let result = worker_tool_results.worker_tool_result(&invocation)?;
            self.validate_worker_tool_result(&invocation.envelope, &result)?;
            self.emit_observer_event(
                request.run_id.clone(),
                result.source_agent_id.clone(),
                EVENT_AGENTIC_WORKER_TOOL_RESULT_RECEIVED,
            );

            final_pairs.push((invocation.envelope.clone(), result.clone()));
            invocations.push(invocation);
            worker_results.push(result);
        }

        let final_response = self.compose_final_response(ComposeFinalResponseRequest {
            run_id: request.run_id.clone(),
            finalizer_agent_id: request.finalizer_agent_id.clone(),
            content: request.final_response_content,
            worker_results: final_pairs,
        })?;
        self.emit_observer_event(
            request.run_id.clone(),
            request.finalizer_agent_id.clone(),
            EVENT_AGENTIC_FINAL_RESPONSE_CREATED,
        );
        self.emit_observer_event(
            request.run_id,
            request.finalizer_agent_id,
            EVENT_AGENTIC_COORDINATOR_COMPLETED,
        );

        Ok(AgenticCoordinatorRun {
            invocations,
            worker_results,
            final_response,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallableWorkerRegistry, WorkerToolResultValidationError};
    use greentic_dw_core::RuntimeEvent;
    use greentic_dw_delegation::{
        DelegationDecision, DelegationError, DelegationHandle, DelegationMergeResult,
        DelegationMode, MergePolicy, MergeSubtaskResultRequest, StartSubtaskRequest,
    };
    use greentic_dw_engine::{EngineDecision, StaticEngine};
    use greentic_dw_pack::{ObserverSub, PackIntegration};
    use greentic_dw_types::{AgentRoute, CallableWorkerTool, InterAgentRoutingConfig};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct RecordingDelegator {
        subtask_ids: Mutex<Vec<String>>,
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
            let mut subtask_ids = self.subtask_ids.lock().expect("recording lock");
            subtask_ids.push(req.envelope.subtask_id.clone());
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

    struct RecordingObserver {
        events: Arc<Mutex<Vec<String>>>,
    }

    impl ObserverSub for RecordingObserver {
        fn on_operation(&self, event: &RuntimeEvent) {
            let mut events = self.events.lock().expect("event lock");
            events.push(event.operation.name().to_string());
        }
    }

    struct EchoWorkerResults;
    impl WorkerToolResultProvider for EchoWorkerResults {
        fn worker_tool_result(
            &self,
            invocation: &WorkerToolInvocation,
        ) -> Result<SubtaskResultEnvelope, WorkerToolRunError> {
            Ok(SubtaskResultEnvelope {
                subtask_id: invocation.envelope.subtask_id.clone(),
                correlation_id: invocation.envelope.correlation_id.clone(),
                source_agent_id: invocation.envelope.target_agent.clone(),
                target_agent_id: invocation.envelope.source_agent_id.clone(),
                tool_id: invocation.envelope.tool_id.clone(),
                status: "completed".to_string(),
                output_artifact_ref: format!("artifact://{}", invocation.envelope.tool_id),
                output_schema_ref: invocation.envelope.expected_output_schema.clone(),
                notes: vec![],
            })
        }
    }

    struct WrongToolResult;

    impl WorkerToolResultProvider for WrongToolResult {
        fn worker_tool_result(
            &self,
            invocation: &WorkerToolInvocation,
        ) -> Result<SubtaskResultEnvelope, WorkerToolRunError> {
            Ok(SubtaskResultEnvelope {
                subtask_id: invocation.envelope.subtask_id.clone(),
                correlation_id: invocation.envelope.correlation_id.clone(),
                source_agent_id: invocation.envelope.target_agent.clone(),
                target_agent_id: invocation.envelope.source_agent_id.clone(),
                tool_id: "wrong_tool".to_string(),
                status: "completed".to_string(),
                output_artifact_ref: "artifact://wrong".to_string(),
                output_schema_ref: invocation.envelope.expected_output_schema.clone(),
                notes: vec![],
            })
        }
    }

    fn runtime() -> DwRuntime<StaticEngine> {
        let routing = InterAgentRoutingConfig {
            allowed_routes: Vec::new(),
            coordinator_agent_id: Some("coordinator".to_string()),
            finalizer_agent_id: Some("coordinator".to_string()),
            routes: vec![
                AgentRoute {
                    from_agent_id: "coordinator".to_string(),
                    to_agent_id: "traffic-specialist".to_string(),
                    allowed: true,
                },
                AgentRoute {
                    from_agent_id: "coordinator".to_string(),
                    to_agent_id: "bgp-specialist".to_string(),
                    allowed: true,
                },
            ],
            callable_workers: vec![
                CallableWorkerTool {
                    tool_id: "traffic_analysis".to_string(),
                    target_agent_id: "traffic-specialist".to_string(),
                    description: "Analyze traffic".to_string(),
                    input_schema_ref: "schema://telco-x/traffic-analysis-request.v1".to_string(),
                    output_schema_ref: "schema://telco-x/traffic-analysis-result.v1".to_string(),
                },
                CallableWorkerTool {
                    tool_id: "bgp_analysis".to_string(),
                    target_agent_id: "bgp-specialist".to_string(),
                    description: "Analyze BGP".to_string(),
                    input_schema_ref: "schema://telco-x/bgp-analysis-request.v1".to_string(),
                    output_schema_ref: "schema://telco-x/bgp-analysis-result.v1".to_string(),
                },
            ],
            shared_context_policy: None,
        };
        let registry = CallableWorkerRegistry::from_routing(&routing).expect("registry");
        DwRuntime::new(StaticEngine::new(EngineDecision::Noop))
            .with_callable_worker_registry(registry)
    }

    fn request() -> RunAgenticCoordinatorRequest {
        RunAgenticCoordinatorRequest {
            run_id: "run-1".to_string(),
            coordinator_agent_id: "coordinator".to_string(),
            finalizer_agent_id: "coordinator".to_string(),
            final_response_content: "Traffic and BGP checks completed.".to_string(),
            tool_calls: vec![
                CoordinatorWorkerToolCall {
                    tool_id: "traffic_analysis".to_string(),
                    goal: "check traffic".to_string(),
                    context_package_ref: "context://traffic".to_string(),
                    context_scope: HandoffContextScope::ParentTaskOnly,
                    permissions_profile: "restricted".to_string(),
                    deadline: "2026-04-16T00:00:00Z".to_string(),
                    return_policy: HandoffReturnPolicy::Sync,
                },
                CoordinatorWorkerToolCall {
                    tool_id: "bgp_analysis".to_string(),
                    goal: "check bgp".to_string(),
                    context_package_ref: "context://bgp".to_string(),
                    context_scope: HandoffContextScope::ParentTaskOnly,
                    permissions_profile: "restricted".to_string(),
                    deadline: "2026-04-16T00:00:00Z".to_string(),
                    return_policy: HandoffReturnPolicy::Sync,
                },
            ],
        }
    }

    #[test]
    fn coordinator_flow_runs_worker_tools_and_composes_final_response() {
        let runtime = runtime();
        let delegator = RecordingDelegator::default();

        let run = runtime
            .run_agentic_coordinator_flow(request(), &delegator, &EchoWorkerResults)
            .expect("coordinator flow should run");

        assert_eq!(run.invocations.len(), 2);
        assert_eq!(run.worker_results.len(), 2);
        assert_eq!(run.final_response.sources.len(), 2);
        assert_eq!(run.final_response.finalizer_agent_id, "coordinator");
        let subtask_ids = delegator.subtask_ids.lock().expect("recorded subtasks");
        assert_eq!(subtask_ids.len(), 2);
    }

    #[test]
    fn coordinator_flow_rejects_empty_tool_calls() {
        let runtime = runtime();
        let mut request = request();
        request.tool_calls.clear();

        assert!(matches!(
            runtime.run_agentic_coordinator_flow(
                request,
                &RecordingDelegator::default(),
                &EchoWorkerResults,
            ),
            Err(RuntimeError::CoordinatorFlow(
                CoordinatorFlowError::NoToolCalls
            ))
        ));
    }

    #[test]
    fn coordinator_flow_rejects_invalid_worker_result() {
        let runtime = runtime();

        assert!(matches!(
            runtime.run_agentic_coordinator_flow(
                request(),
                &RecordingDelegator::default(),
                &WrongToolResult,
            ),
            Err(RuntimeError::WorkerToolResult(
                WorkerToolResultValidationError::ToolIdMismatch { .. }
            ))
        ));
    }

    #[test]
    fn coordinator_flow_emits_agentic_observability_events() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let packs = PackIntegration::new().with_observer_sub(RecordingObserver {
            events: Arc::clone(&events),
        });
        let runtime = runtime().with_packs(packs);

        runtime
            .run_agentic_coordinator_flow(
                request(),
                &RecordingDelegator::default(),
                &EchoWorkerResults,
            )
            .expect("coordinator flow should run");

        let events = events.lock().expect("events").clone();
        assert_eq!(
            events,
            vec![
                EVENT_AGENTIC_COORDINATOR_STARTED,
                EVENT_AGENTIC_WORKER_TOOL_HANDOFF_STARTED,
                EVENT_AGENTIC_WORKER_TOOL_RESULT_RECEIVED,
                EVENT_AGENTIC_WORKER_TOOL_HANDOFF_STARTED,
                EVENT_AGENTIC_WORKER_TOOL_RESULT_RECEIVED,
                EVENT_AGENTIC_FINAL_RESPONSE_CREATED,
                EVENT_AGENTIC_COORDINATOR_COMPLETED,
            ]
        );
    }
}
