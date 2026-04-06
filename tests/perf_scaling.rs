use greentic_cap_types::CapabilityDeclaration;
use greentic_dw_core::RuntimeOperation;
use greentic_dw_engine::{EngineDecision, StaticEngine};
use greentic_dw_manifest::{
    DigitalWorkerManifest, LocaleContract, MANIFEST_SCHEMA_VERSION, RequestScope, TeamPolicy,
    TenancyContract,
};
use greentic_dw_runtime::DwRuntime;
use greentic_dw_types::{LocalePropagation, OutputLocaleGuidance, WorkerLocalePolicy};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn sample_manifest() -> DigitalWorkerManifest {
    DigitalWorkerManifest {
        id: "dw.perf.scaling".to_string(),
        display_name: "Perf Scaling Worker".to_string(),
        version: MANIFEST_SCHEMA_VERSION.to_string(),
        worker_version: Some("0.5.0".to_string()),
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
    }
}

fn run_workload(threads: usize, total_iterations: usize) -> Duration {
    let start = Instant::now();
    let per_thread = total_iterations / threads;

    let manifest = Arc::new(sample_manifest());
    let request = Arc::new(RequestScope {
        tenant: "tenant-perf".to_string(),
        team: Some("team-request".to_string()),
    });

    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let manifest = Arc::clone(&manifest);
            let request = Arc::clone(&request);

            std::thread::spawn(move || {
                let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Batch(vec![
                    RuntimeOperation::Start,
                    RuntimeOperation::Step,
                    RuntimeOperation::Complete,
                ])));

                for i in 0..per_thread {
                    let mut envelope = manifest
                        .to_task_envelope(
                            format!("task-{t}-{i}"),
                            "worker-perf",
                            &request,
                            Some("en-US".to_string()),
                            Some("en-US".to_string()),
                        )
                        .expect("envelope should build");
                    runtime.tick(&mut envelope).expect("tick should succeed");
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("worker thread should join");
    }

    start.elapsed()
}

#[test]
fn scaling_should_not_degrade_badly() {
    let t1 = run_workload(1, 3_000);
    let t4 = run_workload(4, 3_000);
    let t8 = run_workload(8, 3_000);
    eprintln!("scaling timings: t1={t1:?}, t4={t4:?}, t8={t8:?}");

    assert!(
        t4 <= t1.mul_f64(2.2),
        "4 threads slower than expected: t1={:?}, t4={:?}",
        t1,
        t4
    );

    assert!(
        t8 <= t4.mul_f64(2.2),
        "8 threads slower than expected: t4={:?}, t8={:?}",
        t4,
        t8
    );
}
