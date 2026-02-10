use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use tokio::runtime::Builder;
use up_streamer::benchmark_support::{
    run_single_route_dispatch_once, IngressRegistryFixture, PublishResolutionFixture,
    RoutingLookupFixture,
};

const ROUTING_LOOKUP_ROWS: usize = 256;
const PUBLISH_RESOLUTION_ROWS: usize = 512;
const INGRESS_REGISTRY_ROWS: usize = 128;
const INGRESS_BATCH_OPS: usize = 8;

fn streamer_criterion(c: &mut Criterion) {
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("benchmark runtime should build");

    let exact_lookup_fixture = runtime
        .block_on(RoutingLookupFixture::exact_authority(ROUTING_LOOKUP_ROWS))
        .expect("exact-authority lookup fixture should build");
    let wildcard_lookup_fixture = runtime
        .block_on(RoutingLookupFixture::wildcard_authority(
            ROUTING_LOOKUP_ROWS,
        ))
        .expect("wildcard-authority lookup fixture should build");

    let mut routing_lookup_group = c.benchmark_group("routing_lookup");
    routing_lookup_group.bench_function("exact_authority", |b| {
        b.iter(|| {
            let count = runtime.block_on(exact_lookup_fixture.lookup_count());
            black_box(count);
        });
    });
    routing_lookup_group.bench_function("wildcard_authority", |b| {
        b.iter(|| {
            let count = runtime.block_on(wildcard_lookup_fixture.lookup_count());
            black_box(count);
        });
    });
    routing_lookup_group.finish();

    let publish_resolution_fixture = runtime
        .block_on(PublishResolutionFixture::new(PUBLISH_RESOLUTION_ROWS))
        .expect("publish-resolution fixture should build");

    let mut publish_resolution_group = c.benchmark_group("publish_resolution");
    publish_resolution_group.bench_function("source_filter_derivation", |b| {
        b.iter(|| {
            let count = publish_resolution_fixture.derive_source_filter_count();
            black_box(count);
        });
    });
    publish_resolution_group.finish();

    let mut ingress_registry_group = c.benchmark_group("ingress_registry");
    ingress_registry_group.bench_function("register_route", |b| {
        b.iter_batched(
            || {
                runtime
                    .block_on(IngressRegistryFixture::new(INGRESS_REGISTRY_ROWS))
                    .expect("ingress-registry fixture should build")
            },
            |fixture| {
                for _ in 0..INGRESS_BATCH_OPS {
                    let registered = runtime.block_on(fixture.register_route());
                    assert!(
                        registered,
                        "register benchmark iteration should register route"
                    );
                    runtime.block_on(fixture.unregister_route());
                    black_box(registered);
                }
            },
            BatchSize::SmallInput,
        );
    });
    ingress_registry_group.bench_function("unregister_route", |b| {
        b.iter_batched(
            || {
                let fixture = runtime
                    .block_on(IngressRegistryFixture::new(INGRESS_REGISTRY_ROWS))
                    .expect("ingress-registry fixture should build");
                let primed = runtime.block_on(fixture.register_route());
                assert!(primed, "unregister benchmark setup should prime route");
                fixture
            },
            |fixture| {
                for _ in 0..INGRESS_BATCH_OPS {
                    runtime.block_on(fixture.unregister_route());
                    let re_registered = runtime.block_on(fixture.register_route());
                    assert!(
                        re_registered,
                        "unregister benchmark iteration should restore route"
                    );
                }
                black_box(());
            },
            BatchSize::SmallInput,
        );
    });
    ingress_registry_group.finish();

    let mut egress_forwarding_group = c.benchmark_group("egress_forwarding");
    egress_forwarding_group.bench_function("single_route_dispatch", |b| {
        b.iter(|| {
            let send_count = runtime.block_on(run_single_route_dispatch_once());
            black_box(send_count);
        });
    });
    egress_forwarding_group.finish();
}

criterion_group!(benches, streamer_criterion);
criterion_main!(benches);
