//! Standalone SIMDX Performance Benchmarks
//! 
//! Direct benchmarks using SimSIMD without dependencies on the main RTDB library

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::prelude::*;
use std::time::Duration;
use simsimd::SpatialSimilarity;

/// Generate random f32 vector
fn generate_random_vector(dim: usize, rng: &mut StdRng) -> Vec<f32> {
    (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect()
}

/// Scalar cosine similarity implementation
fn cosine_similarity_scalar(a: &[f32], b: &[f32]) -> f32 {
    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
}

/// Scalar Euclidean distance implementation
fn euclidean_distance_scalar(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Scalar dot product implementation
fn dot_product_scalar(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Benchmark cosine distance computation
fn bench_cosine_distance(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(42);
    
    let dimensions = [128, 256, 512, 768, 1024, 1536, 2048];
    
    for &dim in &dimensions {
        let a = generate_random_vector(dim, &mut rng);
        let b = generate_random_vector(dim, &mut rng);
        
        let mut group = c.benchmark_group(format!("cosine_distance_dim_{}", dim));
        group.throughput(Throughput::Elements(dim as u64));
        
        // SimSIMD implementation
        group.bench_function("simsimd", |bench| {
            bench.iter(|| {
                black_box(<f32 as SpatialSimilarity>::cos(black_box(&a), black_box(&b)).unwrap_or(0.0) as f32)
            })
        });
        
        // Scalar implementation
        group.bench_function("scalar", |bench| {
            bench.iter(|| {
                black_box(cosine_similarity_scalar(black_box(&a), black_box(&b)))
            })
        });
        
        group.finish();
    }
}

/// Benchmark Euclidean distance computation
fn bench_euclidean_distance(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(42);
    
    let dimensions = [128, 256, 512, 768, 1024, 1536, 2048];
    
    for &dim in &dimensions {
        let a = generate_random_vector(dim, &mut rng);
        let b = generate_random_vector(dim, &mut rng);
        
        let mut group = c.benchmark_group(format!("euclidean_distance_dim_{}", dim));
        group.throughput(Throughput::Elements(dim as u64));
        
        // SimSIMD implementation
        group.bench_function("simsimd", |bench| {
            bench.iter(|| {
                let sq_dist = <f32 as SpatialSimilarity>::sqeuclidean(black_box(&a), black_box(&b)).unwrap_or(0.0);
                black_box((sq_dist as f32).sqrt())
            })
        });
        
        // Scalar implementation
        group.bench_function("scalar", |bench| {
            bench.iter(|| {
                black_box(euclidean_distance_scalar(black_box(&a), black_box(&b)))
            })
        });
        
        group.finish();
    }
}

/// Benchmark dot product computation
fn bench_dot_product(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(42);
    
    let dimensions = [128, 256, 512, 768, 1024, 1536, 2048];
    
    for &dim in &dimensions {
        let a = generate_random_vector(dim, &mut rng);
        let b = generate_random_vector(dim, &mut rng);
        
        let mut group = c.benchmark_group(format!("dot_product_dim_{}", dim));
        group.throughput(Throughput::Elements(dim as u64));
        
        // SimSIMD implementation
        group.bench_function("simsimd", |bench| {
            bench.iter(|| {
                black_box(<f32 as SpatialSimilarity>::dot(black_box(&a), black_box(&b)).unwrap_or(0.0) as f32)
            })
        });
        
        // Scalar implementation
        group.bench_function("scalar", |bench| {
            bench.iter(|| {
                black_box(dot_product_scalar(black_box(&a), black_box(&b)))
            })
        });
        
        group.finish();
    }
}

/// Benchmark batch operations with different sizes
fn bench_batch_operations(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(42);
    
    let batch_sizes = [10, 50, 100, 500, 1000, 5000];
    let dim = 512;
    
    for &batch_size in &batch_sizes {
        let query = generate_random_vector(dim, &mut rng);
        let vectors: Vec<Vec<f32>> = (0..batch_size)
            .map(|_| generate_random_vector(dim, &mut rng))
            .collect();
        
        let mut group = c.benchmark_group(format!("batch_cosine_size_{}", batch_size));
        group.throughput(Throughput::Elements(batch_size as u64));
        
        // SimSIMD batch implementation
        group.bench_function("simsimd_batch", |bench| {
            bench.iter(|| {
                let mut results = Vec::with_capacity(vectors.len());
                for vector in &vectors {
                    let distance = <f32 as SpatialSimilarity>::cos(black_box(&query), black_box(vector))
                        .unwrap_or(0.0) as f32;
                    results.push(distance);
                }
                black_box(results)
            })
        });
        
        // Scalar batch implementation
        group.bench_function("scalar_batch", |bench| {
            bench.iter(|| {
                let mut results = Vec::with_capacity(vectors.len());
                for vector in &vectors {
                    let distance = cosine_similarity_scalar(black_box(&query), black_box(vector));
                    results.push(distance);
                }
                black_box(results)
            })
        });
        
        group.finish();
    }
}

/// Benchmark vector normalization
fn bench_vector_normalization(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(42);
    
    let dimensions = [128, 256, 512, 1024, 2048];
    
    for &dim in &dimensions {
        let vector = generate_random_vector(dim, &mut rng);
        
        let mut group = c.benchmark_group(format!("vector_normalization_dim_{}", dim));
        group.throughput(Throughput::Elements(dim as u64));
        
        // SimSIMD-based normalization (using dot product for norm calculation)
        group.bench_function("simsimd_norm", |bench| {
            bench.iter(|| {
                let mut v = vector.clone();
                let norm_squared = <f32 as SpatialSimilarity>::dot(black_box(&v), black_box(&v)).unwrap_or(1.0) as f32;
                let norm = norm_squared.sqrt();
                if norm > 0.0 {
                    let inv_norm = 1.0 / norm;
                    for val in &mut v {
                        *val *= inv_norm;
                    }
                }
                black_box(v)
            })
        });
        
        // Scalar normalization
        group.bench_function("scalar_norm", |bench| {
            bench.iter(|| {
                let mut v = vector.clone();
                let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    let inv_norm = 1.0 / norm;
                    for val in &mut v {
                        *val *= inv_norm;
                    }
                }
                black_box(v)
            })
        });
        
        group.finish();
    }
}

/// Benchmark different vector dimensions for real-world scenarios
fn bench_real_world_dimensions(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(42);
    
    // Common embedding dimensions used in practice
    let real_world_dims = [
        (384, "sentence-transformers-small"),
        (512, "openai-ada-002-small"),
        (768, "bert-base"),
        (1024, "openai-ada-002"),
        (1536, "openai-text-embedding-3-small"),
        (3072, "openai-text-embedding-3-large"),
    ];
    
    for &(dim, name) in &real_world_dims {
        let a = generate_random_vector(dim, &mut rng);
        let b = generate_random_vector(dim, &mut rng);
        
        let mut group = c.benchmark_group(format!("real_world_{}", name));
        group.throughput(Throughput::Elements(dim as u64));
        
        // SimSIMD cosine similarity
        group.bench_function("simsimd_cosine", |bench| {
            bench.iter(|| {
                black_box(<f32 as SpatialSimilarity>::cos(black_box(&a), black_box(&b)).unwrap_or(0.0) as f32)
            })
        });
        
        // Scalar cosine similarity
        group.bench_function("scalar_cosine", |bench| {
            bench.iter(|| {
                black_box(cosine_similarity_scalar(black_box(&a), black_box(&b)))
            })
        });
        
        group.finish();
    }
}

criterion_group!(
    name = standalone_simdx_benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(15))
        .sample_size(200)
        .warm_up_time(Duration::from_secs(3));
    targets = 
        bench_cosine_distance,
        bench_euclidean_distance,
        bench_dot_product,
        bench_batch_operations,
        bench_vector_normalization,
        bench_real_world_dimensions
);

criterion_main!(standalone_simdx_benches);