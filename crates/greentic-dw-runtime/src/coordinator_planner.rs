use greentic_dw_delegation::DelegationProvider;
use greentic_dw_engine::DwEngine;
use thiserror::Error;

use crate::{
    AgenticCoordinatorRun, DwRuntime, RunAgenticCoordinatorRequest, RuntimeError,
    WorkerToolResultProvider,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgenticUserRequest {
    pub run_id: String,
    pub coordinator_agent_id: String,
    pub input: String,
}

pub trait CoordinatorPlanner {
    fn plan_coordinator_flow(
        &self,
        request: AgenticUserRequest,
    ) -> Result<RunAgenticCoordinatorRequest, CoordinatorPlannerError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CoordinatorPlannerError {
    #[error("planner input must not be empty")]
    EmptyInput,
    #[error("planner error: {0}")]
    Provider(String),
}

impl<E: DwEngine> DwRuntime<E> {
    pub fn run_planned_agentic_coordinator_flow(
        &self,
        request: AgenticUserRequest,
        planner: &dyn CoordinatorPlanner,
        delegator: &dyn DelegationProvider,
        worker_tool_results: &dyn WorkerToolResultProvider,
    ) -> Result<AgenticCoordinatorRun, RuntimeError> {
        if request.input.trim().is_empty() {
            return Err(CoordinatorPlannerError::EmptyInput.into());
        }

        let flow_request = planner.plan_coordinator_flow(request)?;
        self.run_agentic_coordinator_flow(flow_request, delegator, worker_tool_results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallableWorkerRegistry, CoordinatorWorkerToolCall, WorkerToolInvocation};
    use greentic_dw_delegation::{
        DelegationDecision, DelegationError, DelegationHandle, DelegationMergeResult,
        DelegationMode, HandoffContextScope, HandoffReturnPolicy, MergePolicy,
        MergeSubtaskResultRequest, StartSubtaskRequest, SubtaskResultEnvelope,
    };
    use greentic_dw_engine::{EngineDecision, StaticEngine};
    use greentic_dw_types::{AgentRoute, CallableWorkerTool, InterAgentRoutingConfig};

    struct StaticPlanner;

    impl CoordinatorPlanner for StaticPlanner {
        fn plan_coordinator_flow(
            &self,
            request: AgenticUserRequest,
        ) -> Result<RunAgenticCoordinatorRequest, CoordinatorPlannerError> {
            Ok(RunAgenticCoordinatorRequest {
                run_id: request.run_id,
                coordinator_agent_id: request.coordinator_agent_id.clone(),
                finalizer_agent_id: request.coordinator_agent_id,
                final_response_content: "Traffic analysis completed.".to_string(),
                tool_calls: vec![CoordinatorWorkerToolCall {
                    tool_id: "traffic_analysis".to_string(),
                    goal: request.input,
                    context_package_ref: "context://traffic".to_string(),
                    context_scope: HandoffContextScope::ParentTaskOnly,
                    permissions_profile: "restricted".to_string(),
                    deadline: "2026-04-16T00:00:00Z".to_string(),
                    return_policy: HandoffReturnPolicy::Sync,
                }],
            })
        }
    }

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

    struct EchoWorkerResults;

    impl WorkerToolResultProvider for EchoWorkerResults {
        fn worker_tool_result(
            &self,
            invocation: &WorkerToolInvocation,
        ) -> Result<SubtaskResultEnvelope, crate::WorkerToolRunError> {
            Ok(SubtaskResultEnvelope {
                subtask_id: invocation.envelope.subtask_id.clone(),
                correlation_id: invocation.envelope.correlation_id.clone(),
                source_agent_id: invocation.envelope.target_agent.clone(),
                target_agent_id: invocation.envelope.source_agent_id.clone(),
                tool_id: invocation.envelope.tool_id.clone(),
                status: "completed".to_string(),
                output_artifact_ref: "artifact://traffic-analysis".to_string(),
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

    #[test]
    fn planned_coordinator_flow_uses_planner_output() {
        let run = runtime()
            .run_planned_agentic_coordinator_flow(
                AgenticUserRequest {
                    run_id: "run-1".to_string(),
                    coordinator_agent_id: "coordinator".to_string(),
                    input: "check traffic".to_string(),
                },
                &StaticPlanner,
                &NoopDelegator,
                &EchoWorkerResults,
            )
            .expect("planned flow should run");

        assert_eq!(run.invocations.len(), 1);
        assert_eq!(run.final_response.content, "Traffic analysis completed.");
    }

    #[test]
    fn planned_coordinator_flow_rejects_empty_input() {
        assert!(matches!(
            runtime().run_planned_agentic_coordinator_flow(
                AgenticUserRequest {
                    run_id: "run-1".to_string(),
                    coordinator_agent_id: "coordinator".to_string(),
                    input: " ".to_string(),
                },
                &StaticPlanner,
                &NoopDelegator,
                &EchoWorkerResults,
            ),
            Err(RuntimeError::CoordinatorPlanner(
                CoordinatorPlannerError::EmptyInput
            ))
        ));
    }
}
