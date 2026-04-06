use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use greentic_cap_types::CapabilityDeclaration;
use greentic_dw_core::RuntimeOperation;
use greentic_dw_engine::{EngineDecision, StaticEngine};
use greentic_dw_manifest::{
    DigitalWorkerManifest, LocaleContract, RequestScope, TeamPolicy, TenancyContract,
};
use greentic_dw_runtime::DwRuntime;
use greentic_dw_types::{LocalePropagation, OutputLocaleGuidance, WorkerLocalePolicy};
use std::hint::black_box;

fn sample_manifest() -> DigitalWorkerManifest {
    DigitalWorkerManifest {
        id: "dw.bench".to_string(),
        display_name: "Benchmark Worker".to_string(),
        version: "0.2".to_string(),
        worker_version: Some("0.5".to_string()),
        capabilities: CapabilityDeclaration::new(),
        tenancy: TenancyContract {
            tenant: "tenant-bench".to_string(),
            team_policy: TeamPolicy::Optional {
                default_team: Some("team-bench".to_string()),
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

fn bench_manifest_hot_paths(c: &mut Criterion) {
    let manifest = sample_manifest();
    let request = RequestScope {
        tenant: "tenant-bench".to_string(),
        team: Some("request-team".to_string()),
    };

    c.bench_function("manifest_validate", |b| {
        b.iter(|| {
            manifest.validate().expect("manifest should validate");
            black_box(());
        })
    });

    c.bench_function("manifest_to_task_envelope", |b| {
        b.iter(|| {
            let envelope = manifest
                .to_task_envelope(
                    "task-bench",
                    "worker-bench",
                    &request,
                    Some("en-US".to_string()),
                    Some("en-US".to_string()),
                )
                .expect("envelope should build");
            black_box(envelope);
        })
    });
}

fn bench_runtime_tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_tick");

    for op_count in [1_usize, 3, 8] {
        let operations = std::iter::once(RuntimeOperation::Start)
            .chain(std::iter::repeat_n(
                RuntimeOperation::Step,
                op_count.saturating_sub(2),
            ))
            .chain(std::iter::once(RuntimeOperation::Complete))
            .collect::<Vec<_>>();

        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Batch(operations)));

        let manifest = sample_manifest();
        let request = RequestScope {
            tenant: "tenant-bench".to_string(),
            team: Some("request-team".to_string()),
        };

        group.throughput(Throughput::Elements(op_count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(op_count), &op_count, |b, _| {
            b.iter(|| {
                let mut envelope = manifest
                    .to_task_envelope(
                        "task-bench",
                        "worker-bench",
                        &request,
                        Some("en-US".to_string()),
                        Some("en-US".to_string()),
                    )
                    .expect("envelope should build");

                let events = runtime.tick(&mut envelope).expect("tick should succeed");
                black_box(events);
            })
        });
    }

    group.finish();
}

criterion_group!(benches, bench_manifest_hot_paths, bench_runtime_tick);
criterion_main!(benches);
