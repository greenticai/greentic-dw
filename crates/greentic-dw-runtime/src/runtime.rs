use greentic_cap_types::{CapabilityBinding, CapabilityId};
use greentic_dw_core::{CoreRuntimeError, RuntimeEvent, RuntimeOperation, apply_operation};
use greentic_dw_engine::{DwEngine, EngineDecision, EngineError};
use greentic_dw_pack::{HookError, PackIntegration};
use greentic_dw_types::TaskEnvelope;
use std::sync::Arc;
use thiserror::Error;

use crate::{
    CapabilityDispatchError, CapabilityMemoryExtension, MemoryError, MemoryExtension, MemoryQuery,
    MemoryRecord, RuntimeCapabilityBindings, TaskStateStore,
};

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
