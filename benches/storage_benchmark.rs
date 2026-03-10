//! Storage benchmarks

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rtdb::storage::{StorageConfig, StorageEngine};
use rtdb::{Vector, VectorId};
use tempfile::TempDir;

fn bench_put(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let config = StorageConfig {
        path: temp_dir.path().to_str().unwrap().to_string(),
        wal_segment_size: 1024 * 1024,
        memtable_size_threshold: 1024 * 1024,
        block_size: 4 * 1024,
        compression: rtdb::storage::CompressionType::None,
    };

    let engine = StorageEngine::open(config).unwrap();

    c.bench_function("storage_put", |b| {
        let mut i = 0u64;
        b.iter(|| {
            i += 1;
            let v = Vector::new(vec![i as f32; 128]);
            engine.put(i, v).unwrap();
        });
    });
}

fn bench_get(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let config = StorageConfig {
        path: temp_dir.path().to_str().unwrap().to_string(),
        wal_segment_size: 1024 * 1024,
        memtable_size_threshold: 1024 * 1024,
        block_size: 4 * 1024,
        compression: rtdb::storage::CompressionType::None,
    };

    let engine = StorageEngine::open(config).unwrap();

    // Insert test data
    for i in 1..=1000 {
        let v = Vector::new(vec![i as f32; 128]);
        engine.put(i, v).unwrap();
    }

    c.bench_function("storage_get", |b| {
        let mut i = 0u64;
        b.iter(|| {
            i = (i % 1000) + 1;
            black_box(engine.get(i).unwrap());
        });
    });
}

criterion_group!(benches, bench_put, bench_get);
criterion_main!(benches);
