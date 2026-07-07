use crate::{BehaviorConfig, PackSourceRef, ProviderBinding};
use greentic_cap_types::CapabilityId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Layout hints for the eventual application pack structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationPackLayoutHints {
    SingleAgentPack,
    MultiAgentSharedProviders,
    MultiAgentIsolatedAssets,
}

/// Shared capability binding applied at the application level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SharedCapabilityBinding {
    pub binding_id: String,
    pub capability_id: CapabilityId,
    pub provider_binding: ProviderBinding,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack_capability_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applies_to_agents: Vec<String>,
}

/// Agent-local override for a shared capability binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentLocalBindingOverride {
    pub shared_binding_id: String,
    pub provider_binding: ProviderBinding,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack_capability_id: Option<String>,
}

/// Visibility policy for context shared during agent-to-agent delegation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgenticSharedContextPolicy {
    None,
    ParentTaskOnly,
    SharedMemoryAndArtifacts,
}

/// A worker that the coordinator can invoke through its tool/delegation surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CallableWorkerTool {
    pub tool_id: String,
    pub target_agent_id: String,
    pub description: String,
    pub input_schema_ref: String,
    pub output_schema_ref: String,
}

/// Explicit allowed route from one agent to another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentRoute {
    pub from_agent_id: String,
    pub to_agent_id: String,
    #[serde(default = "default_route_allowed")]
    pub allowed: bool,
}

fn default_route_allowed() -> bool {
    true
}

/// Routing and worker-tool configuration for inter-agent flows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InterAgentRoutingConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_routes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordinator_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finalizer_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<AgentRoute>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub callable_workers: Vec<CallableWorkerTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_context_policy: Option<AgenticSharedContextPolicy>,
}

/// Agent reference inside the target application/package model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DwApplicationAgentRef {
    pub agent_id: String,
    pub display_name: String,
    pub template_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_binding_overrides: Vec<AgentLocalBindingOverride>,
    #[serde(default)]
    pub behavior_config: BehaviorConfig,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asset_roots: Vec<String>,
}

/// Target runtime/package application model derived from composition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DwApplication {
    pub application_id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<DwApplicationAgentRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_bindings: Vec<SharedCapabilityBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_support_pack_refs: Vec<PackSourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing: Option<InterAgentRoutingConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_hint: Option<ApplicationPackLayoutHints>,
}
