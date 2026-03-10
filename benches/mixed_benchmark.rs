//! Mixed Workload Benchmarks
//!
//! Simulates real-world workloads with mixed read/write ratios:
//! - Read-heavy (90% search, 10% insert)
//! - Write-heavy (10% search, 90% insert)
//! - Balanced (50% search, 50% insert)
//! - Sequential scan vs random access

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rtdb::index::hnsw::HNSWIndex;
use rtdb::index::VectorIndex;
use rtdb::{Distance, HnswConfig, SearchRequest, Vector, VectorId};
use rand::prelude::*;
use rand::SeedableRng;
use std::time::Duration;

/// Generate random vectors
fn generate_vectors(count: usize, dim: usize, seed: u64) -> Vec<(VectorId, Vector)> {
    let mut rng = StdRng::seed_from_u64(seed);
    (0..count)
        .map(|i| {
            let data: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
            (i as VectorId, Vector::new(data))
        })
        .collect()
}

/// Generate a random query vector
fn generate_query(dim: usize, seed: u64) -> Vector {
    let mut rng = StdRng::seed_from_u64(seed);
    let data: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
    Vector::new(data)
}

/// Read-heavy workload (90% reads, 10% writes)
fn bench_read_heavy(c: &mut Criterion) {
    let mut group = c.benchmark_group("workload_read_heavy");
    group.measurement_time(Duration::from_secs(15));

    let dim = 128;
    let initial_size = 10000;
    let ops_count = 1000;

    group.bench_function("90r_10w", |b| {
        b.iter_with_setup(
            || {
                let vectors = generate_vectors(initial_size, dim, 42);
                let config = HnswConfig {
                    m: 16,
                    ef_construct: 100,
                    ef: 64,
                    num_layers: None,
                };
                let mut index = HNSWIndex::new(config, Distance::Cosine);
                for (id, vec) in &vectors {
                    index.add(*id, vec).unwrap();
                }

                let new_vectors = generate_vectors(ops_count, dim, 100);
                let queries: Vec<Vector> = (0..ops_count)
                    .map(|i| generate_query(dim, 200 + i as u64))
                    .collect();

                (index, new_vectors, queries)
            },
            |(mut index, new_vectors, queries)| {
                for i in 0..ops_count {
                    if i % 10 == 0 {
                        // 10% writes
                        let (id, vec) = &new_vectors[i % new_vectors.len()];
                        black_box(index.add(*id, vec).unwrap());
                    } else {
                        // 90% reads
                        let request = SearchRequest {
                            vector: queries[i % queries.len()].data.clone(),
                            limit: 10,
                            offset: 0,
                            score_threshold: None,
                            with_payload: None,
                            with_vector: false,
                            filter: None,
                            params: None,
                        };
                        black_box(index.search(&request).unwrap());
                    }
                }
            },
        );
    });

    group.finish();
}

/// Write-heavy workload (10% reads, 90% writes)
fn bench_write_heavy(c: &mut Criterion) {
    let mut group = c.benchmark_group("workload_write_heavy");
    group.measurement_time(Duration::from_secs(15));

    let dim = 128;
    let initial_size = 1000;
    let ops_count = 1000;

    group.bench_function("10r_90w", |b| {
        b.iter_with_setup(
            || {
                let vectors = generate_vectors(initial_size, dim, 42);
                let config = HnswConfig {
                    m: 16,
                    ef_construct: 100,
                    ef: 64,
                    num_layers: None,
                };
                let mut index = HNSWIndex::new(config, Distance::Cosine);
                for (id, vec) in &vectors {
                    index.add(*id, vec).unwrap();
                }

                let new_vectors = generate_vectors(ops_count, dim, 100);
                let queries: Vec<Vector> = (0..10).map(|i| generate_query(dim, 200 + i as u64)).collect();

                (index, new_vectors, queries)
            },
            |(mut index, new_vectors, queries)| {
                for i in 0..ops_count {
                    if i % 10 == 0 {
                        // 10% reads
                        let request = SearchRequest {
                            vector: queries[i % queries.len()].data.clone(),
                            limit: 10,
                            offset: 0,
                            score_threshold: None,
                            with_payload: None,
                            with_vector: false,
                            filter: None,
                            params: None,
                        };
                        black_box(index.search(&request).unwrap());
                    } else {
                        // 90% writes
                        let (id, vec) = &new_vectors[i % new_vectors.len()];
                        black_box(index.add(*id, vec).unwrap());
                    }
                }
            },
        );
    });

    group.finish();
}

/// Balanced workload (50% reads, 50% writes)
fn bench_balanced(c: &mut Criterion) {
    let mut group = c.benchmark_group("workload_balanced");
    group.measurement_time(Duration::from_secs(15));

    let dim = 128;
    let initial_size = 5000;
    let ops_count = 1000;

    group.bench_function("50r_50w", |b| {
        b.iter_with_setup(
            || {
                let vectors = generate_vectors(initial_size, dim, 42);
                let config = HnswConfig {
                    m: 16,
                    ef_construct: 100,
                    ef: 64,
                    num_layers: None,
                };
                let mut index = HNSWIndex::new(config, Distance::Cosine);
                for (id, vec) in &vectors {
                    index.add(*id, vec).unwrap();
                }

                let new_vectors = generate_vectors(ops_count, dim, 100);
                let queries: Vec<Vector> = (0..500).map(|i| generate_query(dim, 200 + i as u64)).collect();

                (index, new_vectors, queries)
            },
            |(mut index, new_vectors, queries)| {
                for i in 0..ops_count {
                    if i % 2 == 0 {
                        // 50% reads
                        let request = SearchRequest {
                            vector: queries[i % queries.len()].data.clone(),
                            limit: 10,
                            offset: 0,
                            score_threshold: None,
                            with_payload: None,
                            with_vector: false,
                            filter: None,
                            params: None,
                        };
                        black_box(index.search(&request).unwrap());
                    } else {
                        // 50% writes
                        let (id, vec) = &new_vectors[i % new_vectors.len()];
                        black_box(index.add(*id, vec).unwrap());
                    }
                }
            },
        );
    });

    group.finish();
}

/// Batch insert vs individual insert
fn bench_batch_vs_individual(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_vs_individual");
    group.measurement_time(Duration::from_secs(10));

    let dim = 128;
    let batch_sizes = [100, 500, 1000];

    for &batch_size in &batch_sizes {
        let vectors = generate_vectors(batch_size, dim, 42);

        group.throughput(Throughput::Elements(batch_size as u64));

        // Individual inserts
        group.bench_with_input(
            BenchmarkId::new("individual", batch_size),
            &batch_size,
            |b, _| {
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
            },
        );

        // Batch insert (using build)
        group.bench_with_input(
            BenchmarkId::new("batch", batch_size),
            &batch_size,
            |b, _| {
                b.iter(|| {
                    let config = HnswConfig {
                        m: 16,
                        ef_construct: 100,
                        ef: 64,
                        num_layers: None,
                    };
                    let mut index = HNSWIndex::new(config, Distance::Cosine);
                    black_box(index.build(&vectors).unwrap());
                });
            },
        );
    }

    group.finish();
}

/// Memory vs performance tradeoff
fn bench_memory_tradeoff(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_tradeoff");
    group.measurement_time(Duration::from_secs(10));

    let dim = 128;
    let dataset_size = 10000;
    let vectors = generate_vectors(dataset_size, dim, 42);

    // Test different HNSW M values (memory vs speed tradeoff)
    for &m in &[8, 16, 32, 64] {
        let config = HnswConfig {
            m,
            ef_construct: 100,
            ef: 64,
            num_layers: None,
        };
        let mut index = HNSWIndex::new(config, Distance::Cosine);
        for (id, vec) in &vectors {
            index.add(*id, vec).unwrap();
        }

        let query = generate_query(dim, 999);
        let request = SearchRequest {
            vector: query.data,
            limit: 10,
            offset: 0,
            score_threshold: None,
            with_payload: None,
            with_vector: false,
            filter: None,
            params: None,
        };

        group.bench_with_input(BenchmarkId::new("hnsw_m", m), &m, |b, _| {
            b.iter(|| {
                black_box(index.search(&request).unwrap());
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_read_heavy,
    bench_write_heavy,
    bench_balanced,
    bench_batch_vs_individual,
    bench_memory_tradeoff
);
criterion_main!(benches);
