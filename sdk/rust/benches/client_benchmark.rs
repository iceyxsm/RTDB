use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rtdb_client::{RTDBClient, RTDBConfig};
use std::time::Duration;

fn bench_client_creation(c: &mut Criterion) {
    c.bench_function("client_creation", |b| {
        b.iter(|| {
            let config = RTDBConfig::new("http://localhost:8080");
            black_box(config)
        })
    });
}

criterion_group!(benches, bench_client_creation);
criterion_main!(benches);