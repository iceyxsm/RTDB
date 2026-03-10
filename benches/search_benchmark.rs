//! Vector Search Benchmarks
//!
//! Benchmarks vector search performance with different:
//! - Dataset sizes (1K, 10K, 100K, 1M vectors)
//! - Dimensions (128, 384, 768, 1536)
//! - Top-k values (1, 10, 100)
//! - Distance metrics (Cosine, Euclidean, Dot)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rtdb::index::distance::{cosine_similarity, dot_product, l2_distance};
use rtdb::index::hnsw::HNSWIndex;
use rtdb::index::VectorIndex;
use rtdb::{Distance, HnswConfig, SearchRequest, Vector, VectorId};
use rand::prelude::*;
use rand::SeedableRng;
use std::time::Duration;

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

/// Generate a random query vector
fn generate_query(dim: usize, seed: u64) -> Vector {
    let mut rng = StdRng::seed_from_u64(seed);
    let data: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
    Vector::new(data)
}

fn bench_distance_metrics(c: &mut Criterion) {
    let mut group = c.benchmark_group("distance_metrics");
    group.measurement_time(Duration::from_secs(5));

    for dim in [128, 384, 768, 1536] {
        let v1: Vec<f32> = generate_vectors(1, dim, 42)[0].1.data.clone();
        let v2: Vec<f32> = generate_vectors(1, dim, 43)[0].1.data.clone();

        group.throughput(Throughput::Elements(dim as u64));

        group.bench_with_input(BenchmarkId::new("cosine", dim), &dim, |b, _| {
            b.iter(|| cosine_similarity(black_box(&v1), black_box(&v2)));
        });

        group.bench_with_input(BenchmarkId::new("euclidean", dim), &dim, |b, _| {
            b.iter(|| l2_distance(black_box(&v1), black_box(&v2)));
        });

        group.bench_with_input(BenchmarkId::new("dot_product", dim), &dim, |b, _| {
            b.iter(|| dot_product(black_box(&v1), black_box(&v2)));
        });
    }

    group.finish();
}

fn bench_hnsw_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("hnsw_search");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(50);

    let dim = 128;
    let ef_search_values = [16, 32, 64, 128];

    for &dataset_size in &[1000, 10000] {
        let vectors = generate_vectors(dataset_size, dim, 42);
        let query = generate_query(dim, 999);

        // Build index
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

        group.throughput(Throughput::Elements(dataset_size as u64));

        for &ef in &ef_search_values {
            let request = SearchRequest {
                vector: query.data.clone(),
                limit: 10,
                offset: 0,
                score_threshold: None,
                with_payload: None,
                with_vector: false,
                filter: None,
                params: None,
            };

            group.bench_with_input(
                BenchmarkId::new(format!("dataset_{}", dataset_size), ef),
                &ef,
                |b, _| {
                    b.iter(|| {
                        let _results = index.search(black_box(&request));
                    });
                },
            );
        }
    }

    group.finish();
}

fn bench_topk_variations(c: &mut Criterion) {
    let mut group = c.benchmark_group("topk_search");
    group.measurement_time(Duration::from_secs(10));

    let dim = 128;
    let dataset_size = 10000;
    let vectors = generate_vectors(dataset_size, dim, 42);
    let query = generate_query(dim, 999);

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

    for k in [1, 5, 10, 50, 100] {
        let request = SearchRequest {
            vector: query.data.clone(),
            limit: k,
            offset: 0,
            score_threshold: None,
            with_payload: None,
            with_vector: false,
            filter: None,
            params: None,
        };

        group.bench_with_input(BenchmarkId::new("hnsw", k), &k, |b, _| {
            b.iter(|| {
                let _results = index.search(black_box(&request));
            });
        });
    }

    group.finish();
}

fn bench_brute_force_vs_hnsw(c: &mut Criterion) {
    let mut group = c.benchmark_group("brute_force_vs_hnsw");
    group.measurement_time(Duration::from_secs(10));

    let dim = 128;
    let query = generate_query(dim, 999);

    for &dataset_size in &[100, 1000, 10000] {
        let vectors = generate_vectors(dataset_size, dim, 42);

        // Brute force benchmark
        group.bench_with_input(
            BenchmarkId::new("brute_force", dataset_size),
            &dataset_size,
            |b, _| {
                b.iter(|| {
                    let mut results: Vec<(VectorId, f32)> = vectors
                        .iter()
                        .map(|(id, vec)| {
                            let score = cosine_similarity(&query.data, &vec.data).unwrap();
                            (*id, score)
                        })
                        .collect();
                    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                    black_box(&results[..10.min(results.len())]);
                });
            },
        );

        // HNSW benchmark
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

        let request = SearchRequest {
            vector: query.data.clone(),
            limit: 10,
            offset: 0,
            score_threshold: None,
            with_payload: None,
            with_vector: false,
            filter: None,
            params: None,
        };

        group.bench_with_input(BenchmarkId::new("hnsw", dataset_size), &dataset_size, |b, _| {
            b.iter(|| {
                let _results = index.search(black_box(&request));
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_distance_metrics,
    bench_hnsw_search,
    bench_topk_variations,
    bench_brute_force_vs_hnsw
);
criterion_main!(benches);
