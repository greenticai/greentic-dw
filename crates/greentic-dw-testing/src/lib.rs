//! Conformance fixtures and test helpers for the DW workspace.

use greentic_dw_manifest::{
    DigitalWorkerManifest, LocaleContract, RequestScope, TeamPolicy, TenancyContract,
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
            version: "0.1.0".to_string(),
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
    use greentic_dw_core::RuntimeOperation;
    use greentic_dw_engine::{EngineDecision, StaticEngine};
    use greentic_dw_runtime::{
        AllowAllMemoryPolicy, DwRuntime, MemoryExtension, MemoryPolicyError, MemoryProvider,
        MemoryProviderError, MemoryQuery, MemoryRecord, MemoryScope,
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

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
}
