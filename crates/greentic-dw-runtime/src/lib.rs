//! Digital Worker runtime kernel.
//!
//! Runtime owns state transitions and side-effect mediation. Engines only
//! return structured decisions.

use greentic_dw_core::{CoreRuntimeError, RuntimeEvent, RuntimeOperation, apply_operation};
use greentic_dw_engine::{DwEngine, EngineDecision, EngineError};
use greentic_dw_pack::{HookError, PackIntegration};
use greentic_dw_types::TaskEnvelope;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryScope {
    Task,
    Worker,
    Tenant,
}

/// Portable memory record exchanged between runtime and memory provider packs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRecord {
    pub scope: MemoryScope,
    pub subject: String,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
}

/// Runtime kernel orchestrating engine decisions + lifecycle operations.
pub struct DwRuntime<E: DwEngine> {
    engine: E,
    packs: PackIntegration,
    memory: Option<MemoryExtension>,
}

impl<E: DwEngine> DwRuntime<E> {
    pub fn new(engine: E) -> Self {
        Self {
            engine,
            packs: PackIntegration::new(),
            memory: None,
        }
    }

    pub fn with_packs(mut self, packs: PackIntegration) -> Self {
        self.packs = packs;
        self
    }

    pub fn with_memory(mut self, memory: MemoryExtension) -> Self {
        self.memory = Some(memory);
        self
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
        let memory = self.memory.as_ref().ok_or(MemoryError::NotConfigured)?;
        memory.remember(envelope, record)?;
        Ok(())
    }

    pub fn recall(
        &self,
        envelope: &TaskEnvelope,
        query: &MemoryQuery,
    ) -> Result<Option<MemoryRecord>, RuntimeError> {
        let memory = self.memory.as_ref().ok_or(MemoryError::NotConfigured)?;
        memory.recall(envelope, query).map_err(RuntimeError::from)
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
        Ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
