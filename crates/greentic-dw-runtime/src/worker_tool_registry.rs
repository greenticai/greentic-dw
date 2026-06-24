use greentic_dw_delegation::SubtaskEnvelope;
use greentic_dw_types::{CallableWorkerTool, DwApplicationPackSpec, InterAgentRoutingConfig};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallableWorkerRegistry {
    coordinator_agent_id: Option<String>,
    finalizer_agent_id: Option<String>,
    tools_by_id: BTreeMap<String, CallableWorkerTool>,
    allowed_routes: BTreeSet<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CallableWorkerRegistryError {
    #[error("callable worker registry is not configured")]
    NotConfigured,
    #[error("callable worker registry requires at least one callable worker")]
    NoCallableWorkers,
    #[error("duplicate callable worker tool id `{0}`")]
    DuplicateToolId(String),
    #[error("unknown callable worker tool `{0}`")]
    UnknownTool(String),
    #[error("worker tool `{tool_id}` targets `{expected}`, but handoff targets `{actual}`")]
    TargetMismatch {
        tool_id: String,
        expected: String,
        actual: String,
    },
    #[error("agent `{source_agent_id}` is not allowed to call `{target_agent_id}`")]
    RouteNotAllowed {
        source_agent_id: String,
        target_agent_id: String,
    },
    #[error("worker tool `{tool_id}` output schema `{expected}`, but handoff expects `{actual}`")]
    OutputSchemaMismatch {
        tool_id: String,
        expected: String,
        actual: String,
    },
}

impl CallableWorkerRegistry {
    pub fn from_pack_spec(
        spec: &DwApplicationPackSpec,
    ) -> Result<Option<Self>, CallableWorkerRegistryError> {
        spec.routing.as_ref().map(Self::from_routing).transpose()
    }

    pub fn from_routing(
        routing: &InterAgentRoutingConfig,
    ) -> Result<Self, CallableWorkerRegistryError> {
        if routing.callable_workers.is_empty() {
            return Err(CallableWorkerRegistryError::NoCallableWorkers);
        }

        let mut tools_by_id = BTreeMap::new();
        for tool in &routing.callable_workers {
            if tools_by_id
                .insert(tool.tool_id.clone(), tool.clone())
                .is_some()
            {
                return Err(CallableWorkerRegistryError::DuplicateToolId(
                    tool.tool_id.clone(),
                ));
            }
        }

        let mut allowed_routes = BTreeSet::new();
        for route in &routing.routes {
            if route.allowed {
                allowed_routes.insert((route.from_agent_id.clone(), route.to_agent_id.clone()));
            }
        }
        for route in &routing.allowed_routes {
            if let Some((source, target)) = route.split_once("->") {
                allowed_routes.insert((source.to_string(), target.to_string()));
            }
        }

        Ok(Self {
            coordinator_agent_id: routing.coordinator_agent_id.clone(),
            finalizer_agent_id: routing.finalizer_agent_id.clone(),
            tools_by_id,
            allowed_routes,
        })
    }

    pub fn callable_worker(&self, tool_id: &str) -> Option<&CallableWorkerTool> {
        self.tools_by_id.get(tool_id)
    }

    pub fn coordinator_agent_id(&self) -> Option<&str> {
        self.coordinator_agent_id.as_deref()
    }

    pub fn finalizer_agent_id(&self) -> Option<&str> {
        self.finalizer_agent_id.as_deref()
    }

    pub fn validate_handoff(
        &self,
        envelope: &SubtaskEnvelope,
    ) -> Result<&CallableWorkerTool, CallableWorkerRegistryError> {
        let tool = self
            .tools_by_id
            .get(&envelope.tool_id)
            .ok_or_else(|| CallableWorkerRegistryError::UnknownTool(envelope.tool_id.clone()))?;

        if tool.target_agent_id != envelope.target_agent {
            return Err(CallableWorkerRegistryError::TargetMismatch {
                tool_id: envelope.tool_id.clone(),
                expected: tool.target_agent_id.clone(),
                actual: envelope.target_agent.clone(),
            });
        }

        if !self.is_route_allowed(&envelope.source_agent_id, &envelope.target_agent) {
            return Err(CallableWorkerRegistryError::RouteNotAllowed {
                source_agent_id: envelope.source_agent_id.clone(),
                target_agent_id: envelope.target_agent.clone(),
            });
        }

        if tool.output_schema_ref != envelope.expected_output_schema {
            return Err(CallableWorkerRegistryError::OutputSchemaMismatch {
                tool_id: envelope.tool_id.clone(),
                expected: tool.output_schema_ref.clone(),
                actual: envelope.expected_output_schema.clone(),
            });
        }

        Ok(tool)
    }

    fn is_route_allowed(&self, source_agent_id: &str, target_agent_id: &str) -> bool {
        if self
            .allowed_routes
            .contains(&(source_agent_id.to_string(), target_agent_id.to_string()))
        {
            return true;
        }

        self.allowed_routes.is_empty()
            && self
                .coordinator_agent_id
                .as_deref()
                .is_some_and(|coordinator| coordinator == source_agent_id)
            && self
                .tools_by_id
                .values()
                .any(|tool| tool.target_agent_id == target_agent_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_dw_delegation::{
        HandoffContextScope, HandoffReturnPolicy, SubtaskEnvelope, validate_subtask_envelope,
    };
    use greentic_dw_types::{AgentRoute, CallableWorkerTool, InterAgentRoutingConfig};

    fn registry() -> CallableWorkerRegistry {
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
        CallableWorkerRegistry::from_routing(&routing).expect("registry")
    }

    fn handoff() -> SubtaskEnvelope {
        SubtaskEnvelope {
            subtask_id: "subtask-1".to_string(),
            parent_run_id: "run-1".to_string(),
            correlation_id: "corr-1".to_string(),
            source_agent_id: "coordinator".to_string(),
            target_agent: "traffic-specialist".to_string(),
            tool_id: "traffic_analysis".to_string(),
            goal: "check traffic".to_string(),
            context_package_ref: "context://traffic".to_string(),
            context_scope: HandoffContextScope::ParentTaskOnly,
            expected_output_schema: "schema://telco-x/traffic-analysis-result.v1".to_string(),
            permissions_profile: "restricted".to_string(),
            deadline: "2026-04-16T00:00:00Z".to_string(),
            return_policy: HandoffReturnPolicy::Sync,
        }
    }

    #[test]
    fn validates_allowed_worker_tool_handoff() {
        let registry = registry();
        let envelope = handoff();
        validate_subtask_envelope(&envelope).expect("envelope validates");

        let tool = registry
            .validate_handoff(&envelope)
            .expect("handoff is allowed");
        assert_eq!(tool.target_agent_id, "traffic-specialist");
    }

    #[test]
    fn rejects_unknown_tool_id() {
        let registry = registry();
        let mut envelope = handoff();
        envelope.tool_id = "bgp_analysis".to_string();

        assert_eq!(
            registry.validate_handoff(&envelope),
            Err(CallableWorkerRegistryError::UnknownTool(
                "bgp_analysis".to_string()
            ))
        );
    }

    #[test]
    fn rejects_target_mismatch() {
        let registry = registry();
        let mut envelope = handoff();
        envelope.target_agent = "bgp-specialist".to_string();

        assert!(matches!(
            registry.validate_handoff(&envelope),
            Err(CallableWorkerRegistryError::TargetMismatch { .. })
        ));
    }

    #[test]
    fn rejects_disallowed_route() {
        let registry = registry();
        let mut envelope = handoff();
        envelope.source_agent_id = "bgp-specialist".to_string();

        assert!(matches!(
            registry.validate_handoff(&envelope),
            Err(CallableWorkerRegistryError::RouteNotAllowed { .. })
        ));
    }

    #[test]
    fn rejects_output_schema_mismatch() {
        let registry = registry();
        let mut envelope = handoff();
        envelope.expected_output_schema = "schema://wrong".to_string();

        assert!(matches!(
            registry.validate_handoff(&envelope),
            Err(CallableWorkerRegistryError::OutputSchemaMismatch { .. })
        ));
    }
}
