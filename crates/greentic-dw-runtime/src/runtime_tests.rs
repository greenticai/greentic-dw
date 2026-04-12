#[cfg(test)]
mod tests {
    use crate::{
        AllowAllMemoryPolicy, CapabilityDispatchError, CapabilityDispatcher,
        CapabilityMemoryExtension, CapabilityTaskStateStore, DwRuntime, MEMORY_GET_OPERATION,
        MEMORY_PUT_OPERATION, MemoryError, MemoryExtension, MemoryPolicy, MemoryPolicyError,
        MemoryProvider, MemoryProviderError, MemoryQuery, MemoryRecord, MemoryScope,
        RuntimeCapabilityBindings, RuntimeError, STATE_LOAD_OPERATION, STATE_SAVE_OPERATION,
    };
    use greentic_cap_types::{
        CapabilityBinding, CapabilityBindingKind, CapabilityDeclaration, CapabilityId,
        CapabilityOffer, CapabilityProviderOperationMap, CapabilityProviderRef,
        CapabilityResolution,
    };
    use greentic_dw_core::{CoreRuntimeError, RuntimeEvent, RuntimeOperation};
    use greentic_dw_engine::{EngineDecision, StaticEngine};
    use greentic_dw_pack::{ControlHook, HookDecision, HookError, ObserverSub, PackIntegration};
    use greentic_dw_types::{
        LocaleContext, LocalePropagation, OutputLocaleGuidance, TaskEnvelope, TaskLifecycleState,
        TenantScope, WorkerLocalePolicy,
    };
    use serde_json::Value;
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
        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Noop));
        let mut envelope = sample_envelope();

        runtime.start(&mut envelope).expect("start should succeed");
        runtime
            .complete(&mut envelope)
            .expect("complete should succeed");

        assert_eq!(envelope.state, TaskLifecycleState::Completed);
    }

    #[test]
    fn runtime_rejects_illegal_transition() {
        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Noop));
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
        let packs = PackIntegration::new().with_control_hook(BlockCompleteHook);
        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Noop)).with_packs(packs);
        let mut envelope = sample_envelope();

        runtime.start(&mut envelope).expect("start should succeed");
        let err = runtime
            .complete(&mut envelope)
            .expect_err("complete should be blocked");

        assert!(matches!(err, RuntimeError::Hook(HookError::Blocked { .. })));
    }

    #[test]
    fn observer_is_notified_for_operations() {
        let operations = Arc::new(Mutex::new(Vec::new()));
        let packs = PackIntegration::new().with_observer_sub(RecordingObserver {
            operations: Arc::clone(&operations),
        });
        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Noop)).with_packs(packs);

        let mut envelope = sample_envelope();
        runtime.start(&mut envelope).expect("start should succeed");
        runtime.step(&mut envelope).expect("step should succeed");

        let ops = operations.lock().expect("lock operations");
        assert_eq!(ops.as_slice(), ["start", "step"]);
    }

    #[test]
    fn tick_applies_engine_batch() {
        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Batch(vec![
            RuntimeOperation::Start,
            RuntimeOperation::Step,
        ])));

        let mut envelope = sample_envelope();
        let events = runtime.tick(&mut envelope).expect("tick should succeed");

        assert_eq!(events.len(), 2);
        assert_eq!(envelope.state, TaskLifecycleState::Running);
    }

    #[test]
    fn memory_roundtrip_with_extension() {
        let memory = MemoryExtension::new(
            Arc::new(InMemoryProvider::default()),
            Arc::new(AllowAllMemoryPolicy),
        );
        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Noop)).with_memory(memory);
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
        let memory = MemoryExtension::new(
            Arc::new(InMemoryProvider::default()),
            Arc::new(DenyWritePolicy),
        );
        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Noop)).with_memory(memory);
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
        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Noop))
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
