//! Conformance fixtures and test helpers for the DW workspace.

#[cfg(test)]
mod deep_examples_tests;
#[cfg(test)]
mod deep_fixtures_tests;
#[cfg(test)]
mod deep_loop_harness_tests;
#[cfg(test)]
mod deep_matrix_tests;
#[cfg(test)]
mod deep_snapshot_tests;
#[cfg(test)]
mod starter_e2e_tests;

use greentic_cap_types::CapabilityDeclaration;
use greentic_dw_manifest::{
    DigitalWorkerManifest, LocaleContract, MANIFEST_SCHEMA_VERSION, RequestScope, TeamPolicy,
    TenancyContract,
};
use greentic_dw_types::{
    LocalePropagation, OutputLocaleGuidance, TaskEnvelope, WorkerLocalePolicy,
};

/// Shared fixture with a valid manifest and request scope.
#[derive(Debug, Clone)]
pub struct ConformanceFixture {
    pub manifest: DigitalWorkerManifest,
    pub request_scope: RequestScope,
}

impl ConformanceFixture {
    pub fn task_envelope(&self) -> TaskEnvelope {
        self.manifest
            .to_task_envelope(
                "fixture-task-1",
                self.manifest.id.clone(),
                &self.request_scope,
                Some("en-US".to_string()),
                Some("en-GB".to_string()),
            )
            .expect("fixture manifest should always produce a valid task envelope")
    }
}

/// Canonical fixture used by runtime/CLI conformance tests.
pub fn default_fixture() -> ConformanceFixture {
    ConformanceFixture {
        manifest: DigitalWorkerManifest {
            id: "dw.fixture".to_string(),
            display_name: "DW Fixture".to_string(),
            version: MANIFEST_SCHEMA_VERSION.to_string(),
            worker_version: Some("0.5".to_string()),
            capabilities: CapabilityDeclaration::new(),
            tenancy: TenancyContract {
                tenant: "tenant-a".to_string(),
                team_policy: TeamPolicy::Optional {
                    default_team: Some("team-a".to_string()),
                    allow_request_override: true,
                },
            },
            locale: LocaleContract {
                worker_default_locale: "en-US".to_string(),
                policy: WorkerLocalePolicy::PreferRequested,
                propagation: LocalePropagation::PropagateToDelegates,
                output: OutputLocaleGuidance::MatchRequested,
            },
            deep_agent: None,
        },
        request_scope: RequestScope {
            tenant: "tenant-a".to_string(),
            team: Some("team-a".to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_cap_types::{
        CapabilityBinding, CapabilityBindingKind, CapabilityComponentDescriptor,
        CapabilityDeclaration, CapabilityId, CapabilityOffer, CapabilityProviderOperationMap,
        CapabilityProviderRef, CapabilityResolution,
    };
    use greentic_dw_core::RuntimeOperation;
    use greentic_dw_engine::{EngineDecision, StaticEngine};
    use greentic_dw_pack::{pack_capabilities, validate_pack_capabilities};
    use greentic_dw_runtime::{
        AllowAllMemoryPolicy, CapabilityDispatchError, CapabilityDispatcher,
        CapabilityTaskStateStore, DwRuntime, MemoryExtension, MemoryPolicyError, MemoryProvider,
        MemoryProviderError, MemoryQuery, MemoryRecord, MemoryScope, RuntimeCapabilityBindings,
        STATE_LOAD_OPERATION, STATE_SAVE_OPERATION,
    };
    use serde_json::Value;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    fn workspace_examples_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .canonicalize()
            .expect("workspace examples dir")
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

    struct TenantGuardPolicy;

    impl greentic_dw_runtime::MemoryPolicy for TenantGuardPolicy {
        fn allow_write(
            &self,
            envelope: &TaskEnvelope,
            record: &MemoryRecord,
        ) -> Result<(), MemoryPolicyError> {
            if record.scope == MemoryScope::Tenant && record.subject != envelope.scope.tenant {
                return Err(MemoryPolicyError::Denied(
                    "tenant mismatch for write".to_string(),
                ));
            }
            Ok(())
        }

        fn allow_read(
            &self,
            envelope: &TaskEnvelope,
            query: &MemoryQuery,
        ) -> Result<(), MemoryPolicyError> {
            if query.scope == MemoryScope::Tenant && query.subject != envelope.scope.tenant {
                return Err(MemoryPolicyError::Denied(
                    "tenant mismatch for read".to_string(),
                ));
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockCapabilityDispatcher {
        calls: Mutex<Vec<(String, String)>>,
        states: Mutex<HashMap<String, TaskEnvelope>>,
    }

    impl CapabilityDispatcher for MockCapabilityDispatcher {
        fn invoke(
            &self,
            binding: &CapabilityBinding,
            operation: &str,
            payload: Value,
        ) -> Result<Value, CapabilityDispatchError> {
            let mut calls = self
                .calls
                .lock()
                .map_err(|_| CapabilityDispatchError::Backend("call log poisoned".to_string()))?;
            calls.push((
                binding.capability.as_str().to_string(),
                operation.to_string(),
            ));
            drop(calls);

            match (binding.capability.as_str(), operation) {
                ("cap://dw.state.task-store", STATE_SAVE_OPERATION) => {
                    let envelope: TaskEnvelope = serde_json::from_value(payload)
                        .map_err(|err| CapabilityDispatchError::InvalidPayload(err.to_string()))?;
                    let mut states = self.states.lock().map_err(|_| {
                        CapabilityDispatchError::Backend("state store poisoned".to_string())
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
                        CapabilityDispatchError::Backend("state store poisoned".to_string())
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

    fn shared_pack_section() -> greentic_dw_pack::DwPackCapabilitySection {
        let mut declaration = CapabilityDeclaration::new();

        let mut offer = CapabilityOffer::new(
            "offer.short-term-memory",
            CapabilityId::new("cap://dw.memory.short-term").expect("cap"),
        );
        offer.provider = Some(CapabilityProviderRef {
            component_ref: "component:memory.redis".to_string(),
            operation: "memory.put".to_string(),
            operation_map: vec![
                CapabilityProviderOperationMap {
                    contract_operation: "get".to_string(),
                    component_operation: "memory.get".to_string(),
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: serde_json::json!({"type": "string"}),
                },
                CapabilityProviderOperationMap {
                    contract_operation: "put".to_string(),
                    component_operation: "memory.put".to_string(),
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: serde_json::json!({"type": "null"}),
                },
            ],
        });
        declaration.offers.push(offer);

        let mut require = greentic_cap_types::CapabilityRequirement::new(
            "require.task-store",
            CapabilityId::new("cap://dw.state.task-store").expect("cap"),
        );
        require.description = Some("Task state storage".to_string());
        declaration.requires.push(require);

        let mut consume = greentic_cap_types::CapabilityConsume::new(
            "consume.audit",
            CapabilityId::new("cap://dw.observer.audit").expect("cap"),
        );
        consume.description = Some("Audit observer".to_string());
        declaration.consumes.push(consume);

        declaration
            .profiles
            .push(greentic_cap_types::CapabilityProfile {
                id: "dw.production".to_string(),
                description: Some("Production DW capability profile".to_string()),
                requires: vec![],
                consumes: vec![],
            });

        pack_capabilities(declaration)
    }

    fn shared_component_descriptor() -> CapabilityComponentDescriptor {
        CapabilityComponentDescriptor {
            component_ref: "component:memory.redis".to_string(),
            version: "1.2.3".to_string(),
            operations: vec![
                greentic_cap_types::CapabilityComponentOperation {
                    name: "memory.get".to_string(),
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: serde_json::json!({"type": "string"}),
                },
                greentic_cap_types::CapabilityComponentOperation {
                    name: "memory.put".to_string(),
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: serde_json::json!({"type": "null"}),
                },
            ],
            capabilities: vec![CapabilityId::new("cap://dw.memory.short-term").expect("cap")],
            metadata: Default::default(),
        }
    }

    fn state_resolution() -> RuntimeCapabilityBindings {
        let mut declaration = CapabilityDeclaration::new();
        let mut offer = CapabilityOffer::new(
            "offer.task-store",
            CapabilityId::new("cap://dw.state.task-store").expect("cap"),
        );
        offer.provider = Some(CapabilityProviderRef {
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
        declaration.offers.push(offer);

        let mut resolution = CapabilityResolution::new(declaration);
        resolution.bindings.push(CapabilityBinding {
            kind: CapabilityBindingKind::Requirement,
            request_id: "require.dw.state".to_string(),
            offer_id: "offer.task-store".to_string(),
            capability: CapabilityId::new("cap://dw.state.task-store").expect("cap"),
            provider: Some(CapabilityProviderRef {
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
            }),
            profile: None,
        });

        RuntimeCapabilityBindings::new(resolution).expect("state bindings should be valid")
    }

    #[test]
    fn conformance_runtime_batch_reaches_completed() {
        let fixture = default_fixture();
        let mut envelope = fixture.task_envelope();

        let engine = StaticEngine::new(EngineDecision::Batch(vec![
            RuntimeOperation::Start,
            RuntimeOperation::Step,
            RuntimeOperation::Complete,
        ]));
        let runtime = DwRuntime::new(engine);

        let events = runtime.tick(&mut envelope).expect("tick should succeed");
        assert_eq!(events.len(), 3);
        assert_eq!(format!("{:?}", envelope.state), "Completed");
    }

    #[test]
    fn conformance_memory_roundtrip_task_scope() {
        let fixture = default_fixture();
        let envelope = fixture.task_envelope();

        let engine = StaticEngine::new(EngineDecision::Noop);
        let runtime = DwRuntime::new(engine).with_memory(MemoryExtension::new(
            Arc::new(InMemoryProvider::default()),
            Arc::new(AllowAllMemoryPolicy),
        ));

        runtime
            .remember(
                &envelope,
                MemoryRecord {
                    scope: MemoryScope::Task,
                    subject: envelope.task_id.clone(),
                    key: "result".to_string(),
                    value: "ok".to_string(),
                },
            )
            .expect("memory write should succeed");

        let recalled = runtime
            .recall(
                &envelope,
                &MemoryQuery {
                    scope: MemoryScope::Task,
                    subject: envelope.task_id.clone(),
                    key: "result".to_string(),
                },
            )
            .expect("memory read should succeed")
            .expect("record should exist");

        assert_eq!(recalled.value, "ok");
    }

    #[test]
    fn conformance_memory_policy_enforces_tenant_boundary() {
        let fixture = default_fixture();
        let envelope = fixture.task_envelope();

        let engine = StaticEngine::new(EngineDecision::Noop);
        let runtime = DwRuntime::new(engine).with_memory(MemoryExtension::new(
            Arc::new(InMemoryProvider::default()),
            Arc::new(TenantGuardPolicy),
        ));

        let err = runtime
            .remember(
                &envelope,
                MemoryRecord {
                    scope: MemoryScope::Tenant,
                    subject: "tenant-other".to_string(),
                    key: "summary".to_string(),
                    value: "denied".to_string(),
                },
            )
            .expect_err("tenant mismatch should fail");

        let err_text = format!("{err}");
        assert!(err_text.contains("memory access denied"));
    }

    #[test]
    fn conformance_wizard_dry_run_contract_executes() {
        let args = vec![
            "greentic-dw",
            "wizard",
            "--non-interactive",
            "--manifest-id",
            "dw.fixture",
            "--display-name",
            "DW Fixture",
            "--tenant",
            "tenant-a",
            "--dry-run",
            "--emit-answers",
        ];

        greentic_dw_cli::run(args).expect("wizard dry-run should succeed");
    }

    #[test]
    fn conformance_wizard_dry_run_replays_structured_multi_agent_answers() {
        let examples_dir = workspace_examples_dir();
        let answers_path = examples_dir.join("answers/support-squad-create-answers.json");
        let template_catalog_path = examples_dir.join("templates/catalog.json");
        let provider_catalog_path = examples_dir.join("providers/catalog.json");
        let args = vec![
            "greentic-dw",
            "wizard",
            "--non-interactive",
            "--dry-run",
            "--emit-answers",
            "--answers",
            answers_path.to_str().expect("answers path"),
            "--template-catalog",
            template_catalog_path
                .to_str()
                .expect("template catalog path"),
            "--template",
            "dw.support-assistant",
            "--provider-catalog",
            provider_catalog_path
                .to_str()
                .expect("provider catalog path"),
        ];

        greentic_dw_cli::run(args).expect("structured multi-agent wizard dry-run should succeed");
    }

    #[test]
    fn conformance_pack_compatibility_uses_shared_capability_artifacts() {
        let section = shared_pack_section();
        let component = shared_component_descriptor();
        let reports = validate_pack_capabilities(&section, &component).expect("compatibility");

        assert_eq!(reports.len(), 1);
        assert!(reports[0].compatible);
    }

    #[test]
    fn conformance_state_store_roundtrip_uses_resolved_bindings() {
        let dispatcher: Arc<dyn CapabilityDispatcher> =
            Arc::new(MockCapabilityDispatcher::default());
        let state_store = CapabilityTaskStateStore::new(
            CapabilityId::new("cap://dw.state.task-store").expect("cap"),
            state_resolution(),
            Arc::clone(&dispatcher),
        );

        let engine = StaticEngine::new(EngineDecision::Noop);
        let runtime = DwRuntime::new(engine).with_state_store(Arc::new(state_store));
        let fixture = default_fixture();
        let mut envelope = fixture.task_envelope();

        runtime.start(&mut envelope).expect("start should succeed");
        runtime.save_state(&envelope).expect("save should succeed");

        let loaded = runtime
            .load_state(&envelope.task_id)
            .expect("load should succeed")
            .expect("state should be present");

        assert_eq!(loaded.state, envelope.state);
        assert_eq!(loaded.task_id, envelope.task_id);
    }
}
