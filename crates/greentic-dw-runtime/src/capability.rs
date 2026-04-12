use greentic_cap_types::{CapabilityBinding, CapabilityId, CapabilityResolution};
use greentic_dw_types::TaskEnvelope;
use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;

use crate::{
    MEMORY_GET_OPERATION, MEMORY_PUT_OPERATION, MemoryError, MemoryPolicy, MemoryQuery,
    MemoryRecord, STATE_LOAD_OPERATION, STATE_SAVE_OPERATION,
};

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
