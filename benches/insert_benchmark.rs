//! Vector Insertion Benchmarks
//!
//! Benchmarks vector insertion performance with:
//! - Batch insertions (100, 1K, 10K, 100K)
//! - Different dimensions (128, 384, 768, 1536)
//! - HNSW index building time
//! - LSM-tree write performance

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rtdb::index::hnsw::HNSWIndex;
use rtdb::index::VectorIndex;
use rtdb::storage::engine::StorageEngine;
use rtdb::storage::Storage;
use rtdb::storage::StorageConfig;
use rtdb::{Distance, HnswConfig, Vector, VectorId};
use rand::prelude::*;
use rand::SeedableRng;
use std::time::Duration;
use tempfile::tempdir;

/// Generate random vectors for benchmarking
fn generate_vectors(count: usize, dim: usize, seed: u64) -> Vec<(VectorId, Vector)> {
    let mut rng = StdRng::seed_from_u64(seed);
    (0..count)
        .map(|i| {
            let data: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
            (i as VectorId, Vector::new(data))
        })
        .collect()
}

fn bench_hnsw_insertion(c: &mut Criterion) {
    let mut group = c.benchmark_group("hnsw_insertion");
    group.measurement_time(Duration::from_secs(10));

    let dim = 128;

    for &batch_size in &[100, 1000, 10000] {
        let vectors = generate_vectors(batch_size, dim, 42);

        group.throughput(Throughput::Elements(batch_size as u64));

        group.bench_with_input(BenchmarkId::new("sequential", batch_size), &batch_size, |b, _| {
            b.iter(|| {
                let config = HnswConfig {
                    m: 16,
                    ef_construct: 100,
                    ef: 64,
                    num_layers: None,
                };
                let mut index = HNSWIndex::new(config, Distance::Cosine);
                for (id, vec) in &vectors {
                    black_box(index.add(*id, vec).unwrap());
                }
            });
        });
    }

    group.finish();
}

fn bench_hnsw_build_time(c: &mut Criterion) {
    let mut group = c.benchmark_group("hnsw_build");
    group.measurement_time(Duration::from_secs(30));
    group.sample_size(10);

    let dim = 128;

    for &dataset_size in &[1000, 10000, 50000] {
        let vectors = generate_vectors(dataset_size, dim, 42);

        group.throughput(Throughput::Elements(dataset_size as u64));

        group.bench_with_input(BenchmarkId::new("index_build", dataset_size), &dataset_size, |b, _| {
            b.iter_with_setup(
                || {
                    let config = HnswConfig {
                        m: 16,
                        ef_construct: 100,
                        ef: 64,
                        num_layers: None,
                    };
                    (config, vectors.clone())
                },
                |(config, vecs)| {
                    let mut index = HNSWIndex::new(config, Distance::Cosine);
                    black_box(index.build(&vecs).unwrap());
                },
            );
        });
    }

    group.finish();
}

fn bench_storage_insertion(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage_insertion");
    group.measurement_time(Duration::from_secs(10));

    let dim = 128;

    for &batch_size in &[100, 1000, 10000] {
        let vectors = generate_vectors(batch_size, dim, 42);

        group.throughput(Throughput::Elements(batch_size as u64));

        group.bench_with_input(
            BenchmarkId::new("storage_engine", batch_size),
            &batch_size,
            |b, _| {
                b.iter_with_setup(
                    || {
                        let temp_dir = tempdir().unwrap();
                        let config = StorageConfig {
                            path: temp_dir.path().to_string_lossy().to_string(),
                            wal_segment_size: 64 * 1024 * 1024,
                            memtable_size_threshold: 64 * 1024 * 1024,
                            block_size: 4096,
                            compression: rtdb::storage::CompressionType::None,
                        };
                        let storage = StorageEngine::open(config).unwrap();
                        (storage, temp_dir) // temp_dir must be kept alive
                    },
                    |(storage, _temp_dir)| {
                        for (id, vec) in &vectors {
                            black_box(storage.put(*id, vec.clone()).unwrap());
                        }
                    },
                );
            },
        );
    }

    group.finish();
}

fn bench_dimension_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("dimension_scaling");
    group.measurement_time(Duration::from_secs(10));

    let batch_size = 1000;

    for &dim in &[128, 384, 768, 1536] {
        let vectors = generate_vectors(batch_size, dim, 42);

        group.throughput(Throughput::Bytes((batch_size * dim * 4) as u64));

        group.bench_with_input(BenchmarkId::new("hnsw_insert", dim), &dim, |b, _| {
            b.iter(|| {
                let config = HnswConfig {
                    m: 16,
                    ef_construct: 100,
                    ef: 64,
                    num_layers: None,
                };
                let mut index = HNSWIndex::new(config, Distance::Cosine);
                for (id, vec) in &vectors {
                    black_box(index.add(*id, vec).unwrap());
                }
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_hnsw_insertion,
    bench_hnsw_build_time,
    bench_storage_insertion,
    bench_dimension_scaling
);
criterion_main!(benches);
