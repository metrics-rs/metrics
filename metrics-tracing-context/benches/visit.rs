use std::sync::Arc;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use indexmap::IndexMap;
use lockfree_object_pool::LinearObjectPool;
use metrics::SharedString;
use metrics_tracing_context::Labels;
use once_cell::sync::OnceCell;
use tracing::Metadata;
use tracing_core::{
    field::Visit,
    metadata,
    metadata::{Kind, Level},
    Callsite, Interest,
};

type Map = IndexMap<SharedString, SharedString>;

fn get_pool() -> &'static Arc<LinearObjectPool<Map>> {
    static POOL: OnceCell<Arc<LinearObjectPool<Map>>> = OnceCell::new();
    POOL.get_or_init(|| Arc::new(LinearObjectPool::new(Map::new, Map::clear)))
}

const BATCH_SIZE: usize = 1000;

static CALLSITE: TestCallsite = TestCallsite;
static CALLSITE_METADATA: Metadata = metadata! {
    name: "test",
    target: module_path!(),
    level: Level::DEBUG,
    fields: &["test"],
    callsite: &CALLSITE,
    kind: Kind::SPAN,
};

fn visit_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("visit");
    group.bench_function("record_str", |b| {
        let field = CALLSITE.metadata().fields().field("test").expect("test field missing");
        b.iter_batched_ref(
            || Labels(get_pool().pull_owned()),
            |labels| {
                labels.record_str(&field, "test test");
            },
            BatchSize::NumIterations(BATCH_SIZE as u64),
        )
    });
    group.bench_function("record_bool[true]", |b| {
        let field = CALLSITE.metadata().fields().field("test").expect("test field missing");
        b.iter_batched_ref(
            || Labels(get_pool().pull_owned()),
            |labels| {
                labels.record_bool(&field, true);
            },
            BatchSize::NumIterations(BATCH_SIZE as u64),
        )
    });
    group.bench_function("record_bool[false]", |b| {
        let field = CALLSITE.metadata().fields().field("test").expect("test field missing");
        b.iter_batched_ref(
            || Labels(get_pool().pull_owned()),
            |labels| {
                labels.record_bool(&field, false);
            },
            BatchSize::NumIterations(BATCH_SIZE as u64),
        )
    });
    group.bench_function("record_i64", |b| {
        let field = CALLSITE.metadata().fields().field("test").expect("test field missing");
        b.iter_batched_ref(
            || Labels(get_pool().pull_owned()),
            |labels| {
                labels.record_i64(&field, -3423432);
            },
            BatchSize::NumIterations(BATCH_SIZE as u64),
        )
    });
    group.bench_function("record_u64", |b| {
        let field = CALLSITE.metadata().fields().field("test").expect("test field missing");
        b.iter_batched_ref(
            || Labels(get_pool().pull_owned()),
            |labels| {
                labels.record_u64(&field, 3423432);
            },
            BatchSize::NumIterations(BATCH_SIZE as u64),
        )
    });
    group.bench_function("record_debug", |b| {
        let debug_struct = DebugStruct::new();
        let field = CALLSITE.metadata().fields().field("test").expect("test field missing");
        b.iter_batched_ref(
            || Labels(get_pool().pull_owned()),
            |labels| {
                labels.record_debug(&field, &debug_struct);
            },
            BatchSize::NumIterations(BATCH_SIZE as u64),
        )
    });
    group.bench_function("record_debug[bool]", |b| {
        let field = CALLSITE.metadata().fields().field("test").expect("test field missing");
        b.iter_batched_ref(
            || Labels(get_pool().pull_owned()),
            |labels| {
                labels.record_debug(&field, &true);
            },
            BatchSize::NumIterations(BATCH_SIZE as u64),
        )
    });
    group.bench_function("record_debug[i64]", |b| {
        let value: i64 = -3423432;
        let field = CALLSITE.metadata().fields().field("test").expect("test field missing");
        b.iter_batched_ref(
            || Labels(get_pool().pull_owned()),
            |labels| {
                labels.record_debug(&field, &value);
            },
            BatchSize::NumIterations(BATCH_SIZE as u64),
        )
    });
    group.bench_function("record_debug[u64]", |b| {
        let value: u64 = 3423432;
        let field = CALLSITE.metadata().fields().field("test").expect("test field missing");
        b.iter_batched_ref(
            || Labels(get_pool().pull_owned()),
            |labels| {
                labels.record_debug(&field, &value);
            },
            BatchSize::NumIterations(BATCH_SIZE as u64),
        )
    });
    group.finish();
}

#[allow(dead_code)]
#[derive(Debug)]
struct DebugStruct {
    field1: String,
    field2: u64,
}

impl DebugStruct {
    pub fn new() -> DebugStruct {
        DebugStruct { field1: "yeehaw!".to_string(), field2: 324242343243 }
    }
}

struct TestCallsite;

impl Callsite for TestCallsite {
    fn set_interest(&self, _interest: Interest) {}
    fn metadata(&self) -> &Metadata<'_> {
        &CALLSITE_METADATA
    }
}

criterion_group!(benches, visit_benchmark);
criterion_main!(benches);
