use greentic_cap_types::CapabilityDeclaration;
use greentic_dw_core::RuntimeOperation;
use greentic_dw_engine::{EngineDecision, StaticEngine};
use greentic_dw_manifest::{
    DigitalWorkerManifest, LocaleContract, MANIFEST_SCHEMA_VERSION, RequestScope, TeamPolicy,
    TenancyContract,
};
use greentic_dw_runtime::DwRuntime;
use greentic_dw_types::{LocalePropagation, OutputLocaleGuidance, WorkerLocalePolicy};
use std::time::{Duration, Instant};

fn sample_manifest() -> DigitalWorkerManifest {
    DigitalWorkerManifest {
        id: "dw.perf.timeout".to_string(),
        display_name: "Perf Timeout Worker".to_string(),
        version: MANIFEST_SCHEMA_VERSION.to_string(),
        worker_version: Some("0.5".to_string()),
        capabilities: CapabilityDeclaration::new(),
        tenancy: TenancyContract {
            tenant: "tenant-perf".to_string(),
            team_policy: TeamPolicy::Optional {
                default_team: Some("team-perf".to_string()),
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
    }
}

#[test]
fn workload_should_finish_quickly() {
    let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Batch(vec![
        RuntimeOperation::Start,
        RuntimeOperation::Step,
        RuntimeOperation::Complete,
    ])));
    let manifest = sample_manifest();
    let request = RequestScope {
        tenant: "tenant-perf".to_string(),
        team: Some("team-request".to_string()),
    };

    let start = Instant::now();

    for i in 0..4_000 {
        let mut envelope = manifest
            .to_task_envelope(
                format!("task-timeout-{i}"),
                "worker-perf",
                &request,
                Some("en-US".to_string()),
                Some("en-US".to_string()),
            )
            .expect("envelope should build");
        runtime.tick(&mut envelope).expect("tick should succeed");
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(2),
        "workload too slow: {:?}",
        elapsed
    );
}
