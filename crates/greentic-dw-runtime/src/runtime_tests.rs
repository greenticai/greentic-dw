#[cfg(test)]
mod tests {
    use crate::{
        AllowAllMemoryPolicy, CallableWorkerRegistry, CapabilityDispatchError,
        CapabilityDispatcher, CapabilityMemoryExtension, CapabilityTaskStateStore, DwRuntime,
        LONG_TERM_MEMORY_CAPABILITY, MEMORY_GET_OPERATION, MEMORY_PUT_OPERATION, MemoryError,
        MemoryExtension, MemoryPolicy, MemoryPolicyError, MemoryProvider, MemoryProviderError,
        MemoryQuery, MemoryRecord, MemoryScope, RuntimeCapabilityBindings, RuntimeError,
        SHORT_TERM_MEMORY_CAPABILITY, STATE_LOAD_OPERATION, STATE_SAVE_OPERATION,
    };
    use greentic_cap_types::{
        CapabilityBinding, CapabilityBindingKind, CapabilityDeclaration, CapabilityId,
        CapabilityOffer, CapabilityProviderOperationMap, CapabilityProviderRef,
        CapabilityResolution,
    };
    use greentic_dw_core::{CoreRuntimeError, RuntimeEvent, RuntimeOperation};
    use greentic_dw_delegation::{HandoffContextScope, HandoffReturnPolicy, SubtaskEnvelope};
    use greentic_dw_engine::{EngineDecision, StaticEngine};
    use greentic_dw_pack::{ControlHook, HookDecision, HookError, ObserverSub, PackIntegration};
    use greentic_dw_types::{
        AgentRoute, ApplicationPackLayoutHints, CallableWorkerTool, DwApplicationPackLayout,
        DwApplicationPackMetadata, DwApplicationPackSpec, InterAgentRoutingConfig, LocaleContext,
        LocalePropagation, OutputLocaleGuidance, TaskEnvelope, TaskLifecycleState, TenantScope,
        WorkerLocalePolicy,
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
        long_term: Mutex<HashMap<(MemoryScope, String, String), MemoryRecord>>,
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
                ("cap://dw.memory.long-term", MEMORY_PUT_OPERATION) => {
                    let record: MemoryRecord = serde_json::from_value(payload)
                        .map_err(|err| CapabilityDispatchError::InvalidPayload(err.to_string()))?;
                    let mut memory = self.long_term.lock().map_err(|_| {
                        CapabilityDispatchError::Backend("long_term lock poisoned".to_string())
                    })?;
                    memory.insert(
                        (record.scope, record.subject.clone(), record.key.clone()),
                        record.clone(),
                    );
                    Ok(Value::Null)
                }
                ("cap://dw.memory.long-term", MEMORY_GET_OPERATION) => {
                    let query: MemoryQuery = serde_json::from_value(payload)
                        .map_err(|err| CapabilityDispatchError::InvalidPayload(err.to_string()))?;
                    let memory = self.long_term.lock().map_err(|_| {
                        CapabilityDispatchError::Backend("long_term lock poisoned".to_string())
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
            CapabilityId::new(SHORT_TERM_MEMORY_CAPABILITY).expect("cap"),
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

        let mut long_term_offer = CapabilityOffer::new(
            "offer.memory.long",
            CapabilityId::new(LONG_TERM_MEMORY_CAPABILITY).expect("cap"),
        );
        long_term_offer.provider = Some(CapabilityProviderRef {
            component_ref: "component:memory.chronicle".to_string(),
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
        declaration.offers.push(long_term_offer);

        let resolution = CapabilityResolution::new(declaration.clone());
        let mut resolution = resolution;
        resolution.bindings.push(CapabilityBinding {
            kind: CapabilityBindingKind::Requirement,
            request_id: "require.memory".to_string(),
            offer_id: "offer.memory".to_string(),
            capability: CapabilityId::new(SHORT_TERM_MEMORY_CAPABILITY).expect("cap"),
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
        resolution.bindings.push(CapabilityBinding {
            kind: CapabilityBindingKind::Requirement,
            request_id: "require.memory.long".to_string(),
            offer_id: "offer.memory.long".to_string(),
            capability: CapabilityId::new(LONG_TERM_MEMORY_CAPABILITY).expect("cap"),
            provider: declaration.offers[2].provider.clone(),
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
    #[test]
    fn runtime_validates_worker_tool_handoff_against_registry() {
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
        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Noop))
            .with_callable_worker_registry(registry);

        let envelope = SubtaskEnvelope {
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
        };

        runtime
            .validate_worker_tool_handoff(&envelope)
            .expect("handoff should be valid");
    }
    #[test]
    fn runtime_can_configure_callable_worker_registry_from_application_pack_spec() {
        let spec = DwApplicationPackSpec {
            metadata: DwApplicationPackMetadata {
                pack_id: "pack.aw.test".to_string(),
                application_id: "aw.test".to_string(),
                display_name: "AW Test".to_string(),
                version: None,
                multi_agent: true,
            },
            agents: Vec::new(),
            assets: Vec::new(),
            generated_configs: Vec::new(),
            generated_flows: Vec::new(),
            generated_prompts: Vec::new(),
            requirements: Vec::new(),
            dependency_pack_refs: Vec::new(),
            setup_requirements: Vec::new(),
            routing: Some(InterAgentRoutingConfig {
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
            }),
            layout: DwApplicationPackLayout {
                app_root: "aw.test.pack".to_string(),
                shared_asset_roots: vec!["shared".to_string()],
                layout_hint: Some(ApplicationPackLayoutHints::MultiAgentSharedProviders),
            },
        };

        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Noop))
            .with_application_pack_spec(&spec)
            .expect("pack spec configures runtime");

        assert!(
            runtime
                .callable_worker_registry()
                .and_then(|registry| registry.callable_worker("traffic_analysis"))
                .is_some()
        );
    }

    #[test]
    fn long_term_memory_roundtrip_via_capability() {
        let bindings = capability_resolution();
        let dispatcher = Arc::new(MockCapabilityDispatcher::default());
        let long_term = CapabilityMemoryExtension::new(
            CapabilityId::new(LONG_TERM_MEMORY_CAPABILITY).expect("cap"),
            bindings,
            dispatcher,
            Arc::new(AllowAllMemoryPolicy),
        );
        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Noop))
            .with_long_term_memory(long_term);
        let envelope = sample_envelope();

        let record = MemoryRecord {
            scope: MemoryScope::Worker,
            subject: "worker-1".to_string(),
            key: "fact".to_string(),
            value: "graph-value".to_string(),
        };
        runtime
            .remember_long_term(&envelope, record.clone())
            .expect("remember_long_term should succeed");

        let got = runtime
            .recall_long_term(
                &envelope,
                &MemoryQuery {
                    scope: MemoryScope::Worker,
                    subject: "worker-1".to_string(),
                    key: "fact".to_string(),
                },
            )
            .expect("recall_long_term should succeed");
        assert_eq!(got, Some(record));
    }

    #[test]
    fn long_term_memory_not_configured_errors() {
        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Noop));
        let envelope = sample_envelope();
        let err = runtime
            .recall_long_term(
                &envelope,
                &MemoryQuery {
                    scope: MemoryScope::Task,
                    subject: "t".to_string(),
                    key: "k".to_string(),
                },
            )
            .expect_err("recall_long_term without a long-term slot should error");
        assert!(matches!(
            err,
            RuntimeError::Memory(MemoryError::NotConfigured)
        ));
    }
}
