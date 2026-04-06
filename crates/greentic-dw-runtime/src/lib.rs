//! Digital Worker runtime kernel.
//!
//! Runtime owns state transitions and side-effect mediation. Engines only
//! return structured decisions.

use greentic_cap_types::{CapabilityBinding, CapabilityId, CapabilityResolution};
use greentic_dw_core::{CoreRuntimeError, RuntimeEvent, RuntimeOperation, apply_operation};
use greentic_dw_engine::{DwEngine, EngineDecision, EngineError};
use greentic_dw_pack::{HookError, PackIntegration};
use greentic_dw_types::TaskEnvelope;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;

/// Logical operation used when dispatching memory capability calls.
pub const MEMORY_GET_OPERATION: &str = "get";
pub const MEMORY_PUT_OPERATION: &str = "put";

/// Logical operation used when dispatching task-state capability calls.
pub const STATE_LOAD_OPERATION: &str = "load";
pub const STATE_SAVE_OPERATION: &str = "save";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    Task,
    Worker,
    Tenant,
}

/// Portable memory record exchanged between runtime and memory provider packs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub scope: MemoryScope,
    pub subject: String,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub scope: MemoryScope,
    pub subject: String,
    pub key: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MemoryProviderError {
    #[error("memory backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MemoryPolicyError {
    #[error("memory access denied: {0}")]
    Denied(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MemoryError {
    #[error(transparent)]
    Provider(#[from] MemoryProviderError),
    #[error(transparent)]
    Policy(#[from] MemoryPolicyError),
    #[error("memory extension is not configured")]
    NotConfigured,
    #[error(transparent)]
    Dispatch(#[from] CapabilityDispatchError),
}

/// Provider-agnostic runtime capability bindings produced by bundle/setup resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCapabilityBindings {
    resolution: CapabilityResolution,
}

impl RuntimeCapabilityBindings {
    pub fn new(resolution: CapabilityResolution) -> Result<Self, CapabilityDispatchError> {
        resolution
            .validate()
            .map_err(|source| CapabilityDispatchError::Resolution(source.to_string()))?;
        Ok(Self { resolution })
    }

    pub fn resolution(&self) -> &CapabilityResolution {
        &self.resolution
    }

    pub fn binding_for_request(&self, request_id: &str) -> Option<&CapabilityBinding> {
        self.resolution
            .bindings
            .iter()
            .find(|binding| binding.request_id == request_id)
    }

    pub fn binding_for_capability(&self, capability: &CapabilityId) -> Option<&CapabilityBinding> {
        self.resolution
            .bindings
            .iter()
            .find(|binding| binding.capability == *capability)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CapabilityDispatchError {
    #[error("capability dispatch backend error: {0}")]
    Backend(String),
    #[error("capability binding not found for request {request_id}")]
    MissingBinding { request_id: String },
    #[error("capability binding not found for capability {capability}")]
    MissingCapabilityBinding { capability: String },
    #[error("capability resolution invalid: {0}")]
    Resolution(String),
    #[error("invalid capability payload: {0}")]
    InvalidPayload(String),
}

/// Dispatch interface used by capability-aware runtime integrations.
pub trait CapabilityDispatcher: Send + Sync {
    fn invoke(
        &self,
        binding: &CapabilityBinding,
        operation: &str,
        payload: Value,
    ) -> Result<Value, CapabilityDispatchError>;
}

/// Provider-agnostic task state store used for resume/load/save paths.
pub trait TaskStateStore: Send + Sync {
    fn save_state(&self, envelope: &TaskEnvelope) -> Result<(), CapabilityDispatchError>;
    fn load_state(&self, task_id: &str) -> Result<Option<TaskEnvelope>, CapabilityDispatchError>;
}

/// Capability-backed memory extension using resolved bindings and a dispatcher.
#[derive(Clone)]
pub struct CapabilityMemoryExtension {
    capability: CapabilityId,
    bindings: RuntimeCapabilityBindings,
    dispatcher: Arc<dyn CapabilityDispatcher>,
    policy: Arc<dyn MemoryPolicy>,
}

impl CapabilityMemoryExtension {
    pub fn new(
        capability: CapabilityId,
        bindings: RuntimeCapabilityBindings,
        dispatcher: Arc<dyn CapabilityDispatcher>,
        policy: Arc<dyn MemoryPolicy>,
    ) -> Self {
        Self {
            capability,
            bindings,
            dispatcher,
            policy,
        }
    }

    fn binding(&self) -> Result<&CapabilityBinding, CapabilityDispatchError> {
        self.bindings
            .binding_for_capability(&self.capability)
            .ok_or_else(|| CapabilityDispatchError::MissingCapabilityBinding {
                capability: self.capability.as_str().to_string(),
            })
    }

    pub fn remember(
        &self,
        envelope: &TaskEnvelope,
        record: MemoryRecord,
    ) -> Result<(), MemoryError> {
        self.policy.allow_write(envelope, &record)?;
        let payload = serde_json::to_value(&record)
            .map_err(|err| CapabilityDispatchError::InvalidPayload(err.to_string()))?;
        self.dispatcher
            .invoke(self.binding()?, MEMORY_PUT_OPERATION, payload)?;
        Ok(())
    }

    pub fn recall(
        &self,
        envelope: &TaskEnvelope,
        query: &MemoryQuery,
    ) -> Result<Option<MemoryRecord>, MemoryError> {
        self.policy.allow_read(envelope, query)?;
        let payload = serde_json::to_value(query)
            .map_err(|err| CapabilityDispatchError::InvalidPayload(err.to_string()))?;
        let value = self
            .dispatcher
            .invoke(self.binding()?, MEMORY_GET_OPERATION, payload)?;
        if value.is_null() {
            return Ok(None);
        }
        let record: MemoryRecord = serde_json::from_value(value)
            .map_err(|err| CapabilityDispatchError::InvalidPayload(err.to_string()))?;
        Ok(Some(record))
    }
}

/// Capability-backed task state store for provider-agnostic resume/load/save.
#[derive(Clone)]
pub struct CapabilityTaskStateStore {
    capability: CapabilityId,
    bindings: RuntimeCapabilityBindings,
    dispatcher: Arc<dyn CapabilityDispatcher>,
}

impl CapabilityTaskStateStore {
    pub fn new(
        capability: CapabilityId,
        bindings: RuntimeCapabilityBindings,
        dispatcher: Arc<dyn CapabilityDispatcher>,
    ) -> Self {
        Self {
            capability,
            bindings,
            dispatcher,
        }
    }

    fn binding(&self) -> Result<&CapabilityBinding, CapabilityDispatchError> {
        self.bindings
            .binding_for_capability(&self.capability)
            .ok_or_else(|| CapabilityDispatchError::MissingCapabilityBinding {
                capability: self.capability.as_str().to_string(),
            })
    }
}

impl TaskStateStore for CapabilityTaskStateStore {
    fn save_state(&self, envelope: &TaskEnvelope) -> Result<(), CapabilityDispatchError> {
        let payload = serde_json::to_value(envelope)
            .map_err(|err| CapabilityDispatchError::InvalidPayload(err.to_string()))?;
        self.dispatcher
            .invoke(self.binding()?, STATE_SAVE_OPERATION, payload)?;
        Ok(())
    }

    fn load_state(&self, task_id: &str) -> Result<Option<TaskEnvelope>, CapabilityDispatchError> {
        let payload = serde_json::json!({ "task_id": task_id });
        let value = self
            .dispatcher
            .invoke(self.binding()?, STATE_LOAD_OPERATION, payload)?;
        if value.is_null() {
            return Ok(None);
        }
        let envelope: TaskEnvelope = serde_json::from_value(value)
            .map_err(|err| CapabilityDispatchError::InvalidPayload(err.to_string()))?;
        Ok(Some(envelope))
    }
}

/// Backend interface for memory packs. Runtime does not assume any concrete storage.
pub trait MemoryProvider: Send + Sync {
    fn put(&self, record: MemoryRecord) -> Result<(), MemoryProviderError>;
    fn get(&self, query: &MemoryQuery) -> Result<Option<MemoryRecord>, MemoryProviderError>;
}

/// Policy interface for controlling memory read/write behavior.
pub trait MemoryPolicy: Send + Sync {
    fn allow_write(
        &self,
        envelope: &TaskEnvelope,
        record: &MemoryRecord,
    ) -> Result<(), MemoryPolicyError>;

    fn allow_read(
        &self,
        envelope: &TaskEnvelope,
        query: &MemoryQuery,
    ) -> Result<(), MemoryPolicyError>;
}

/// Default permissive memory policy; useful for integration and local tests.
#[derive(Default)]
pub struct AllowAllMemoryPolicy;

impl MemoryPolicy for AllowAllMemoryPolicy {
    fn allow_write(
        &self,
        _envelope: &TaskEnvelope,
        _record: &MemoryRecord,
    ) -> Result<(), MemoryPolicyError> {
        Ok(())
    }

    fn allow_read(
        &self,
        _envelope: &TaskEnvelope,
        _query: &MemoryQuery,
    ) -> Result<(), MemoryPolicyError> {
        Ok(())
    }
}

/// Runtime memory extension entrypoint.
#[derive(Clone)]
pub struct MemoryExtension {
    provider: Arc<dyn MemoryProvider>,
    policy: Arc<dyn MemoryPolicy>,
}

impl MemoryExtension {
    pub fn new(provider: Arc<dyn MemoryProvider>, policy: Arc<dyn MemoryPolicy>) -> Self {
        Self { provider, policy }
    }

    pub fn remember(
        &self,
        envelope: &TaskEnvelope,
        record: MemoryRecord,
    ) -> Result<(), MemoryError> {
        self.policy.allow_write(envelope, &record)?;
        self.provider.put(record)?;
        Ok(())
    }

    pub fn recall(
        &self,
        envelope: &TaskEnvelope,
        query: &MemoryQuery,
    ) -> Result<Option<MemoryRecord>, MemoryError> {
        self.policy.allow_read(envelope, query)?;
        self.provider.get(query).map_err(MemoryError::from)
    }
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Hook(#[from] HookError),
    #[error(transparent)]
    Core(#[from] CoreRuntimeError),
    #[error(transparent)]
    Memory(#[from] MemoryError),
    #[error(transparent)]
    Dispatch(#[from] CapabilityDispatchError),
}

/// Runtime kernel orchestrating engine decisions + lifecycle operations.
pub struct DwRuntime<E: DwEngine> {
    engine: E,
    packs: PackIntegration,
    capability_bindings: Option<RuntimeCapabilityBindings>,
    memory: Option<MemoryExtension>,
    capability_memory: Option<CapabilityMemoryExtension>,
    state_store: Option<Arc<dyn TaskStateStore>>,
}

impl<E: DwEngine> DwRuntime<E> {
    pub fn new(engine: E) -> Self {
        Self {
            engine,
            packs: PackIntegration::new(),
            capability_bindings: None,
            memory: None,
            capability_memory: None,
            state_store: None,
        }
    }

    pub fn with_packs(mut self, packs: PackIntegration) -> Self {
        self.packs = packs;
        self
    }

    pub fn with_capability_bindings(mut self, bindings: RuntimeCapabilityBindings) -> Self {
        self.capability_bindings = Some(bindings);
        self
    }

    pub fn with_memory(mut self, memory: MemoryExtension) -> Self {
        self.memory = Some(memory);
        self
    }

    pub fn with_capability_memory(mut self, memory: CapabilityMemoryExtension) -> Self {
        self.capability_memory = Some(memory);
        self
    }

    pub fn with_state_store(mut self, state_store: Arc<dyn TaskStateStore>) -> Self {
        self.state_store = Some(state_store);
        self
    }

    pub fn capability_bindings(&self) -> Option<&RuntimeCapabilityBindings> {
        self.capability_bindings.as_ref()
    }

    pub fn capability_binding_for_request(&self, request_id: &str) -> Option<&CapabilityBinding> {
        self.capability_bindings
            .as_ref()
            .and_then(|bindings| bindings.binding_for_request(request_id))
    }

    pub fn capability_binding_for_capability(
        &self,
        capability: &CapabilityId,
    ) -> Option<&CapabilityBinding> {
        self.capability_bindings
            .as_ref()
            .and_then(|bindings| bindings.binding_for_capability(capability))
    }

    /// Ask engine for a decision and apply it through runtime mediation.
    pub fn tick(&self, envelope: &mut TaskEnvelope) -> Result<Vec<RuntimeEvent>, RuntimeError> {
        let decision = self.engine.decide_with_envelope(envelope)?;
        self.apply_decision(envelope, decision)
    }

    pub fn start(&self, envelope: &mut TaskEnvelope) -> Result<RuntimeEvent, RuntimeError> {
        self.apply_operation(envelope, RuntimeOperation::Start)
    }

    pub fn step(&self, envelope: &mut TaskEnvelope) -> Result<RuntimeEvent, RuntimeError> {
        self.apply_operation(envelope, RuntimeOperation::Step)
    }

    pub fn delegate(
        &self,
        envelope: &mut TaskEnvelope,
        delegate_worker_id: impl Into<String>,
    ) -> Result<RuntimeEvent, RuntimeError> {
        self.apply_operation(
            envelope,
            RuntimeOperation::Delegate {
                delegate_worker_id: delegate_worker_id.into(),
            },
        )
    }

    pub fn complete(&self, envelope: &mut TaskEnvelope) -> Result<RuntimeEvent, RuntimeError> {
        self.apply_operation(envelope, RuntimeOperation::Complete)
    }

    pub fn fail(
        &self,
        envelope: &mut TaskEnvelope,
        reason: impl Into<String>,
    ) -> Result<RuntimeEvent, RuntimeError> {
        self.apply_operation(
            envelope,
            RuntimeOperation::Fail {
                reason: reason.into(),
            },
        )
    }

    pub fn remember(
        &self,
        envelope: &TaskEnvelope,
        record: MemoryRecord,
    ) -> Result<(), RuntimeError> {
        if let Some(memory) = self.capability_memory.as_ref() {
            memory.remember(envelope, record)?;
        } else {
            let memory = self.memory.as_ref().ok_or(MemoryError::NotConfigured)?;
            memory.remember(envelope, record)?;
        }
        Ok(())
    }

    pub fn recall(
        &self,
        envelope: &TaskEnvelope,
        query: &MemoryQuery,
    ) -> Result<Option<MemoryRecord>, RuntimeError> {
        if let Some(memory) = self.capability_memory.as_ref() {
            memory.recall(envelope, query).map_err(RuntimeError::from)
        } else {
            let memory = self.memory.as_ref().ok_or(MemoryError::NotConfigured)?;
            memory.recall(envelope, query).map_err(RuntimeError::from)
        }
    }

    pub fn save_state(&self, envelope: &TaskEnvelope) -> Result<(), RuntimeError> {
        let store = self.state_store.as_ref().ok_or_else(|| {
            CapabilityDispatchError::Backend("state store is not configured".to_string())
        })?;
        store.save_state(envelope)?;
        Ok(())
    }

    pub fn load_state(&self, task_id: &str) -> Result<Option<TaskEnvelope>, RuntimeError> {
        let store = self.state_store.as_ref().ok_or_else(|| {
            CapabilityDispatchError::Backend("state store is not configured".to_string())
        })?;
        store.load_state(task_id).map_err(RuntimeError::from)
    }

    fn apply_decision(
        &self,
        envelope: &mut TaskEnvelope,
        decision: EngineDecision,
    ) -> Result<Vec<RuntimeEvent>, RuntimeError> {
        match decision {
            EngineDecision::Noop => Ok(vec![]),
            EngineDecision::Operation(operation) => {
                let event = self.apply_operation(envelope, operation)?;
                Ok(vec![event])
            }
            EngineDecision::Batch(operations) => {
                let mut events = Vec::with_capacity(operations.len());
                for operation in operations {
                    events.push(self.apply_operation(envelope, operation)?);
                }
                Ok(events)
            }
        }
    }

    fn apply_operation(
        &self,
        envelope: &mut TaskEnvelope,
        operation: RuntimeOperation,
    ) -> Result<RuntimeEvent, RuntimeError> {
        self.packs.run_pre_hooks(envelope, &operation)?;
        let event = apply_operation(envelope, operation)?;
        self.packs.run_post_hooks(envelope, &event);
        self.packs.notify_observers(&event);
        if let Some(state_store) = &self.state_store {
            state_store.save_state(envelope)?;
        }
        Ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_cap_types::{
        CapabilityBinding, CapabilityBindingKind, CapabilityDeclaration, CapabilityId,
        CapabilityOffer, CapabilityProviderOperationMap, CapabilityProviderRef,
        CapabilityResolution,
    };
    use greentic_dw_engine::StaticEngine;
    use greentic_dw_pack::{ControlHook, HookDecision, ObserverSub};
    use greentic_dw_types::{
        LocaleContext, LocalePropagation, OutputLocaleGuidance, TaskLifecycleState, TenantScope,
        WorkerLocalePolicy,
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn sample_envelope() -> TaskEnvelope {
        TaskEnvelope {
            task_id: "task-1".to_string(),
            worker_id: "worker-1".to_string(),
            state: TaskLifecycleState::Created,
            scope: TenantScope {
                tenant: "tenant-a".to_string(),
                team: Some("team-a".to_string()),
            },
            locale: LocaleContext {
                worker_default_locale: "en-US".to_string(),
                requested_locale: Some("fr-FR".to_string()),
                human_locale: Some("nl-NL".to_string()),
                policy: WorkerLocalePolicy::PreferRequested,
                propagation: LocalePropagation::PropagateToDelegates,
                output: OutputLocaleGuidance::MatchRequested,
            },
        }
    }

    struct BlockCompleteHook;

    impl ControlHook for BlockCompleteHook {
        fn pre_operation(
            &self,
            _envelope: &TaskEnvelope,
            operation: &RuntimeOperation,
        ) -> HookDecision {
            if matches!(operation, RuntimeOperation::Complete) {
                HookDecision::Block {
                    reason: "completion blocked by control pack".to_string(),
                }
            } else {
                HookDecision::Continue
            }
        }

        fn post_operation(&self, _envelope: &TaskEnvelope, _event: &RuntimeEvent) {}
    }

    struct RecordingObserver {
        operations: Arc<Mutex<Vec<String>>>,
    }

    impl ObserverSub for RecordingObserver {
        fn on_operation(&self, event: &RuntimeEvent) {
            let mut ops = self.operations.lock().expect("lock operations");
            ops.push(event.operation.name().to_string());
        }
    }

    #[derive(Default)]
    struct InMemoryProvider {
        data: Mutex<HashMap<(MemoryScope, String, String), String>>,
    }

    impl MemoryProvider for InMemoryProvider {
        fn put(&self, record: MemoryRecord) -> Result<(), MemoryProviderError> {
            let mut data = self.data.lock().map_err(|_| {
                MemoryProviderError::Backend("memory provider lock poisoned".to_string())
            })?;
            data.insert((record.scope, record.subject, record.key), record.value);
            Ok(())
        }

        fn get(&self, query: &MemoryQuery) -> Result<Option<MemoryRecord>, MemoryProviderError> {
            let data = self.data.lock().map_err(|_| {
                MemoryProviderError::Backend("memory provider lock poisoned".to_string())
            })?;

            let key = (query.scope, query.subject.clone(), query.key.clone());
            Ok(data.get(&key).map(|value| MemoryRecord {
                scope: query.scope,
                subject: query.subject.clone(),
                key: query.key.clone(),
                value: value.clone(),
            }))
        }
    }

    struct DenyWritePolicy;

    impl MemoryPolicy for DenyWritePolicy {
        fn allow_write(
            &self,
            _envelope: &TaskEnvelope,
            _record: &MemoryRecord,
        ) -> Result<(), MemoryPolicyError> {
            Err(MemoryPolicyError::Denied(
                "writes disabled by policy".to_string(),
            ))
        }

        fn allow_read(
            &self,
            _envelope: &TaskEnvelope,
            _query: &MemoryQuery,
        ) -> Result<(), MemoryPolicyError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockCapabilityDispatcher {
        operations: Mutex<Vec<(String, String)>>,
        memory: Mutex<HashMap<(MemoryScope, String, String), MemoryRecord>>,
        states: Mutex<HashMap<String, TaskEnvelope>>,
    }

    impl CapabilityDispatcher for MockCapabilityDispatcher {
        fn invoke(
            &self,
            binding: &CapabilityBinding,
            operation: &str,
            payload: Value,
        ) -> Result<Value, CapabilityDispatchError> {
            let mut operations = self.operations.lock().map_err(|_| {
                CapabilityDispatchError::Backend("dispatch lock poisoned".to_string())
            })?;
            operations.push((
                binding.capability.as_str().to_string(),
                operation.to_string(),
            ));
            drop(operations);

            match (binding.capability.as_str(), operation) {
                ("cap://dw.memory.short-term", MEMORY_PUT_OPERATION) => {
                    let record: MemoryRecord = serde_json::from_value(payload)
                        .map_err(|err| CapabilityDispatchError::InvalidPayload(err.to_string()))?;
                    let mut memory = self.memory.lock().map_err(|_| {
                        CapabilityDispatchError::Backend("memory lock poisoned".to_string())
                    })?;
                    memory.insert(
                        (record.scope, record.subject.clone(), record.key.clone()),
                        record.clone(),
                    );
                    Ok(Value::Null)
                }
                ("cap://dw.memory.short-term", MEMORY_GET_OPERATION) => {
                    let query: MemoryQuery = serde_json::from_value(payload)
                        .map_err(|err| CapabilityDispatchError::InvalidPayload(err.to_string()))?;
                    let memory = self.memory.lock().map_err(|_| {
                        CapabilityDispatchError::Backend("memory lock poisoned".to_string())
                    })?;
                    let key = (query.scope, query.subject.clone(), query.key.clone());
                    match memory.get(&key) {
                        Some(record) => serde_json::to_value(record).map_err(|err| {
                            CapabilityDispatchError::InvalidPayload(err.to_string())
                        }),
                        None => Ok(Value::Null),
                    }
                }
                ("cap://dw.state.task-store", STATE_SAVE_OPERATION) => {
                    let envelope: TaskEnvelope = serde_json::from_value(payload)
                        .map_err(|err| CapabilityDispatchError::InvalidPayload(err.to_string()))?;
                    let mut states = self.states.lock().map_err(|_| {
                        CapabilityDispatchError::Backend("state lock poisoned".to_string())
                    })?;
                    states.insert(envelope.task_id.clone(), envelope);
                    Ok(Value::Null)
                }
                ("cap://dw.state.task-store", STATE_LOAD_OPERATION) => {
                    let task_id =
                        payload
                            .get("task_id")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                CapabilityDispatchError::InvalidPayload(
                                    "task_id must be a string".to_string(),
                                )
                            })?;
                    let states = self.states.lock().map_err(|_| {
                        CapabilityDispatchError::Backend("state lock poisoned".to_string())
                    })?;
                    match states.get(task_id) {
                        Some(envelope) => serde_json::to_value(envelope).map_err(|err| {
                            CapabilityDispatchError::InvalidPayload(err.to_string())
                        }),
                        None => Ok(Value::Null),
                    }
                }
                _ => Err(CapabilityDispatchError::Backend(format!(
                    "unsupported capability operation {}::{operation}",
                    binding.capability
                ))),
            }
        }
    }

    fn capability_resolution() -> RuntimeCapabilityBindings {
        let mut declaration = CapabilityDeclaration::new();

        let mut memory_offer = CapabilityOffer::new(
            "offer.memory",
            CapabilityId::new("cap://dw.memory.short-term").expect("cap"),
        );
        memory_offer.provider = Some(CapabilityProviderRef {
            component_ref: "component:memory.redis".to_string(),
            operation: "memory.put".to_string(),
            operation_map: vec![
                CapabilityProviderOperationMap {
                    contract_operation: "get".to_string(),
                    component_operation: "memory.get".to_string(),
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: serde_json::json!({"type": "object"}),
                },
                CapabilityProviderOperationMap {
                    contract_operation: "put".to_string(),
                    component_operation: "memory.put".to_string(),
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: serde_json::json!({"type": "null"}),
                },
            ],
        });
        declaration.offers.push(memory_offer);

        let mut state_offer = CapabilityOffer::new(
            "offer.state",
            CapabilityId::new("cap://dw.state.task-store").expect("cap"),
        );
        state_offer.provider = Some(CapabilityProviderRef {
            component_ref: "component:state.store".to_string(),
            operation: "state.save".to_string(),
            operation_map: vec![
                CapabilityProviderOperationMap {
                    contract_operation: "load".to_string(),
                    component_operation: "state.load".to_string(),
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: serde_json::json!({"type": "object"}),
                },
                CapabilityProviderOperationMap {
                    contract_operation: "save".to_string(),
                    component_operation: "state.save".to_string(),
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: serde_json::json!({"type": "null"}),
                },
            ],
        });
        declaration.offers.push(state_offer);

        let resolution = CapabilityResolution::new(declaration.clone());
        let mut resolution = resolution;
        resolution.bindings.push(CapabilityBinding {
            kind: CapabilityBindingKind::Requirement,
            request_id: "require.memory".to_string(),
            offer_id: "offer.memory".to_string(),
            capability: CapabilityId::new("cap://dw.memory.short-term").expect("cap"),
            provider: declaration.offers[0].provider.clone(),
            profile: None,
        });
        resolution.bindings.push(CapabilityBinding {
            kind: CapabilityBindingKind::Requirement,
            request_id: "require.state".to_string(),
            offer_id: "offer.state".to_string(),
            capability: CapabilityId::new("cap://dw.state.task-store").expect("cap"),
            provider: declaration.offers[1].provider.clone(),
            profile: None,
        });

        RuntimeCapabilityBindings::new(resolution).expect("valid capability bindings")
    }

    #[test]
    fn runtime_applies_start_and_complete() {
        let engine = StaticEngine::new(EngineDecision::Noop);
        let runtime = DwRuntime::new(engine);
        let mut envelope = sample_envelope();

        runtime.start(&mut envelope).expect("start should succeed");
        runtime
            .complete(&mut envelope)
            .expect("complete should succeed");

        assert_eq!(envelope.state, TaskLifecycleState::Completed);
    }

    #[test]
    fn runtime_rejects_illegal_transition() {
        let engine = StaticEngine::new(EngineDecision::Noop);
        let runtime = DwRuntime::new(engine);
        let mut envelope = sample_envelope();

        let err = runtime
            .complete(&mut envelope)
            .expect_err("complete from created should fail");

        assert!(matches!(
            err,
            RuntimeError::Core(CoreRuntimeError::IllegalTransition { .. })
        ));
    }

    #[test]
    fn control_hook_blocks_operation() {
        let engine = StaticEngine::new(EngineDecision::Noop);
        let packs = PackIntegration::new().with_control_hook(BlockCompleteHook);
        let runtime = DwRuntime::new(engine).with_packs(packs);
        let mut envelope = sample_envelope();

        runtime.start(&mut envelope).expect("start should succeed");
        let err = runtime
            .complete(&mut envelope)
            .expect_err("complete should be blocked");

        assert!(matches!(err, RuntimeError::Hook(HookError::Blocked { .. })));
    }

    #[test]
    fn observer_is_notified_for_operations() {
        let engine = StaticEngine::new(EngineDecision::Noop);
        let operations = Arc::new(Mutex::new(Vec::new()));
        let packs = PackIntegration::new().with_observer_sub(RecordingObserver {
            operations: Arc::clone(&operations),
        });
        let runtime = DwRuntime::new(engine).with_packs(packs);

        let mut envelope = sample_envelope();
        runtime.start(&mut envelope).expect("start should succeed");
        runtime.step(&mut envelope).expect("step should succeed");

        let ops = operations.lock().expect("lock operations");
        assert_eq!(ops.as_slice(), ["start", "step"]);
    }

    #[test]
    fn tick_applies_engine_batch() {
        let engine = StaticEngine::new(EngineDecision::Batch(vec![
            RuntimeOperation::Start,
            RuntimeOperation::Step,
        ]));
        let runtime = DwRuntime::new(engine);

        let mut envelope = sample_envelope();
        let events = runtime.tick(&mut envelope).expect("tick should succeed");

        assert_eq!(events.len(), 2);
        assert_eq!(envelope.state, TaskLifecycleState::Running);
    }

    #[test]
    fn memory_roundtrip_with_extension() {
        let engine = StaticEngine::new(EngineDecision::Noop);
        let memory = MemoryExtension::new(
            Arc::new(InMemoryProvider::default()),
            Arc::new(AllowAllMemoryPolicy),
        );
        let runtime = DwRuntime::new(engine).with_memory(memory);
        let envelope = sample_envelope();

        runtime
            .remember(
                &envelope,
                MemoryRecord {
                    scope: MemoryScope::Task,
                    subject: envelope.task_id.clone(),
                    key: "summary".to_string(),
                    value: "customer issue triaged".to_string(),
                },
            )
            .expect("memory write should succeed");

        let recalled = runtime
            .recall(
                &envelope,
                &MemoryQuery {
                    scope: MemoryScope::Task,
                    subject: envelope.task_id.clone(),
                    key: "summary".to_string(),
                },
            )
            .expect("memory read should succeed")
            .expect("memory value should exist");

        assert_eq!(recalled.value, "customer issue triaged");
    }

    #[test]
    fn memory_write_is_blocked_by_policy() {
        let engine = StaticEngine::new(EngineDecision::Noop);
        let memory = MemoryExtension::new(
            Arc::new(InMemoryProvider::default()),
            Arc::new(DenyWritePolicy),
        );
        let runtime = DwRuntime::new(engine).with_memory(memory);
        let envelope = sample_envelope();

        let err = runtime
            .remember(
                &envelope,
                MemoryRecord {
                    scope: MemoryScope::Task,
                    subject: envelope.task_id.clone(),
                    key: "summary".to_string(),
                    value: "should fail".to_string(),
                },
            )
            .expect_err("policy should block writes");

        assert!(matches!(
            err,
            RuntimeError::Memory(MemoryError::Policy(MemoryPolicyError::Denied(_)))
        ));
    }

    #[test]
    fn capability_memory_and_state_use_resolved_bindings() {
        let engine = StaticEngine::new(EngineDecision::Noop);
        let bindings = capability_resolution();
        let dispatcher: Arc<dyn CapabilityDispatcher> =
            Arc::new(MockCapabilityDispatcher::default());
        let memory = CapabilityMemoryExtension::new(
            CapabilityId::new("cap://dw.memory.short-term").expect("cap"),
            bindings.clone(),
            Arc::clone(&dispatcher),
            Arc::new(AllowAllMemoryPolicy),
        );
        let state_store = CapabilityTaskStateStore::new(
            CapabilityId::new("cap://dw.state.task-store").expect("cap"),
            bindings.clone(),
            Arc::clone(&dispatcher),
        );
        let runtime = DwRuntime::new(engine)
            .with_capability_bindings(bindings)
            .with_capability_memory(memory)
            .with_state_store(Arc::new(state_store));
        let envelope = sample_envelope();

        runtime
            .remember(
                &envelope,
                MemoryRecord {
                    scope: MemoryScope::Task,
                    subject: envelope.task_id.clone(),
                    key: "summary".to_string(),
                    value: "resolved through capability bindings".to_string(),
                },
            )
            .expect("capability memory write should succeed");

        let recalled = runtime
            .recall(
                &envelope,
                &MemoryQuery {
                    scope: MemoryScope::Task,
                    subject: envelope.task_id.clone(),
                    key: "summary".to_string(),
                },
            )
            .expect("capability memory read should succeed")
            .expect("memory record should exist");
        assert_eq!(recalled.value, "resolved through capability bindings");

        let mut envelope = envelope;
        runtime.start(&mut envelope).expect("start should succeed");
        runtime
            .complete(&mut envelope)
            .expect("complete should succeed");

        let persisted = runtime
            .load_state(&envelope.task_id)
            .expect("state load should succeed")
            .expect("state should be persisted");
        assert_eq!(persisted.state, TaskLifecycleState::Completed);

        assert!(
            runtime
                .capability_binding_for_capability(
                    &CapabilityId::new("cap://dw.memory.short-term").expect("cap")
                )
                .is_some()
        );
    }
}
