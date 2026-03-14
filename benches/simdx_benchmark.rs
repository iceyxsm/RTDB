//! SIMDX Performance Benchmarks
//! 
//! Comprehensive benchmarks comparing SIMDX-optimized operations
//! against scalar implementations to demonstrate performance gains.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rtdb::simdx::{initialize_simdx, get_simdx_context};
use rtdb::index::distance::scalar;
use rand::prelude::*;
use std::time::Duration;

/// Generate random f32 vector
fn generate_random_vector(dim: usize, rng: &mut StdRng) -> Vec<f32> {
    (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect()
}

/// Generate random u8 vector for binary operations
fn generate_random_binary_vector(bytes: usize, rng: &mut StdRng) -> Vec<u8> {
    (0..bytes).map(|_| rng.gen()).collect()
}

/// Benchmark cosine distance computation
fn bench_cosine_distance(c: &mut Criterion) {
    let _ = initialize_simdx();
    let simdx_context = get_simdx_context();
    let mut rng = StdRng::seed_from_u64(42);
    
    let dimensions = [128, 256, 512, 1024, 2048];
    
    for &dim in &dimensions {
        let a = generate_random_vector(dim, &mut rng);
        let b = generate_random_vector(dim, &mut rng);
        
        let mut group = c.benchmark_group(format!("cosine_distance_dim_{}", dim));
        group.throughput(Throughput::Elements(dim as u64));
        
        // SIMDX implementation
        group.bench_function("simdx", |bench| {
            bench.iter(|| {
                black_box(simdx_context.cosine_distance(black_box(&a), black_box(&b)).unwrap())
            })
        });
        
        // Scalar implementation
        group.bench_function("scalar", |bench| {
            bench.iter(|| {
                black_box(scalar::cosine_similarity(black_box(&a), black_box(&b)).unwrap())
            })
        });
        
        group.finish();
    }
}

/// Benchmark Euclidean distance computation
fn bench_euclidean_distance(c: &mut Criterion) {
    let _ = initialize_simdx();
    let simdx_context = get_simdx_context();
    let mut rng = StdRng::seed_from_u64(42);
    
    let dimensions = [128, 256, 512, 1024, 2048];
    
    for &dim in &dimensions {
        let a = generate_random_vector(dim, &mut rng);
        let b = generate_random_vector(dim, &mut rng);
        
        let mut group = c.benchmark_group(format!("euclidean_distance_dim_{}", dim));
        group.throughput(Throughput::Elements(dim as u64));
        
        // SIMDX implementation
        group.bench_function("simdx", |bench| {
            bench.iter(|| {
                black_box(simdx_context.euclidean_distance(black_box(&a), black_box(&b)).unwrap())
            })
        });
        
        // Scalar implementation
        group.bench_function("scalar", |bench| {
            bench.iter(|| {
                black_box(scalar::l2_distance(black_box(&a), black_box(&b)).unwrap())
            })
        });
        
        group.finish();
    }
}

/// Benchmark dot product computation
fn bench_dot_product(c: &mut Criterion) {
    let _ = initialize_simdx();
    let simdx_context = get_simdx_context();
    let mut rng = StdRng::seed_from_u64(42);
    
    let dimensions = [128, 256, 512, 1024, 2048];
    
    for &dim in &dimensions {
        let a = generate_random_vector(dim, &mut rng);
        let b = generate_random_vector(dim, &mut rng);
        
        let mut group = c.benchmark_group(format!("dot_product_dim_{}", dim));
        group.throughput(Throughput::Elements(dim as u64));
        
        // SIMDX implementation
        group.bench_function("simdx", |bench| {
            bench.iter(|| {
                black_box(simdx_context.dot_product(black_box(&a), black_box(&b)).unwrap())
            })
        });
        
        // Scalar implementation
        group.bench_function("scalar", |bench| {
            bench.iter(|| {
                black_box(scalar::dot_product(black_box(&a), black_box(&b)).unwrap())
            })
        });
        
        group.finish();
    }
}

/// Benchmark batch cosine distance computation
fn bench_batch_cosine_distance(c: &mut Criterion) {
    let _ = initialize_simdx();
    let simdx_context = get_simdx_context();
    let mut rng = StdRng::seed_from_u64(42);
    
    let batch_sizes = [10, 100, 1000];
    let dim = 512;
    
    for &batch_size in &batch_sizes {
        let query = generate_random_vector(dim, &mut rng);
        let vectors: Vec<Vec<f32>> = (0..batch_size)
            .map(|_| generate_random_vector(dim, &mut rng))
            .collect();
        
        let mut group = c.benchmark_group(format!("batch_cosine_distance_size_{}", batch_size));
        group.throughput(Throughput::Elements(batch_size as u64));
        
        // SIMDX batch implementation
        group.bench_function("simdx_batch", |bench| {
            bench.iter(|| {
                black_box(simdx_context.batch_cosine_distance(black_box(&query), black_box(&vectors)).unwrap())
            })
        });
        
        // Scalar individual implementation
        group.bench_function("scalar_individual", |bench| {
            bench.iter(|| {
                let mut results = Vec::with_capacity(vectors.len());
                for vector in &vectors {
                    results.push(scalar::cosine_similarity(black_box(&query), black_box(vector)).unwrap());
                }
                black_box(results)
            })
        });
        
        group.finish();
    }
}

/// Benchmark vector normalization
fn bench_vector_normalization(c: &mut Criterion) {
    let _ = initialize_simdx();
    let simdx_context = get_simdx_context();
    let mut rng = StdRng::seed_from_u64(42);
    
    let dimensions = [128, 256, 512, 1024, 2048];
    
    for &dim in &dimensions {
        let mut vector_simdx = generate_random_vector(dim, &mut rng);
        let mut vector_scalar = vector_simdx.clone();
        
        let mut group = c.benchmark_group(format!("vector_normalization_dim_{}", dim));
        group.throughput(Throughput::Elements(dim as u64));
        
        // SIMDX implementation
        group.bench_function("simdx", |bench| {
            bench.iter(|| {
                let mut v = vector_simdx.clone();
                black_box(simdx_context.normalize_vector(black_box(&mut v)).unwrap());
                black_box(v)
            })
        });
        
        // Scalar implementation
        group.bench_function("scalar", |bench| {
            bench.iter(|| {
                let mut v = vector_scalar.clone();
                let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for val in &mut v {
                        *val /= norm;
                    }
                }
                black_box(v)
            })
        });
        
        group.finish();
    }
}

/// Benchmark batch vector normalization
fn bench_batch_vector_normalization(c: &mut Criterion) {
    let _ = initialize_simdx();
    let simdx_context = get_simdx_context();
    let mut rng = StdRng::seed_from_u64(42);
    
    let batch_sizes = [10, 100, 1000];
    let dim = 512;
    
    for &batch_size in &batch_sizes {
        let vectors: Vec<Vec<f32>> = (0..batch_size)
            .map(|_| generate_random_vector(dim, &mut rng))
            .collect();
        
        let mut group = c.benchmark_group(format!("batch_vector_normalization_size_{}", batch_size));
        group.throughput(Throughput::Elements(batch_size as u64));
        
        // SIMDX batch implementation
        group.bench_function("simdx_batch", |bench| {
            bench.iter(|| {
                let mut v = vectors.clone();
                black_box(simdx_context.batch_normalize_vectors(black_box(&mut v)).unwrap());
                black_box(v)
            })
        });
        
        // Scalar individual implementation
        group.bench_function("scalar_individual", |bench| {
            bench.iter(|| {
                let mut v = vectors.clone();
                for vector in &mut v {
                    let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
                    if norm > 0.0 {
                        for val in vector {
                            *val /= norm;
                        }
                    }
                }
                black_box(v)
            })
        });
        
        group.finish();
    }
}

/// Benchmark Hamming distance for binary vectors
fn bench_hamming_distance(c: &mut Criterion) {
    let _ = initialize_simdx();
    let simdx_context = get_simdx_context();
    let mut rng = StdRng::seed_from_u64(42);
    
    let byte_sizes = [16, 32, 64, 128, 256]; // 128-2048 bits
    
    for &bytes in &byte_sizes {
        let a = generate_random_binary_vector(bytes, &mut rng);
        let b = generate_random_binary_vector(bytes, &mut rng);
        
        let mut group = c.benchmark_group(format!("hamming_distance_bytes_{}", bytes));
        group.throughput(Throughput::Bytes(bytes as u64));
        
        // SIMDX implementation
        group.bench_function("simdx", |bench| {
            bench.iter(|| {
                black_box(simdx_context.hamming_distance(black_box(&a), black_box(&b)).unwrap())
            })
        });
        
        // Scalar implementation
        group.bench_function("scalar", |bench| {
            bench.iter(|| {
                let mut distance = 0u32;
                for (&byte_a, &byte_b) in a.iter().zip(b.iter()) {
                    distance += (byte_a ^ byte_b).count_ones();
                }
                black_box(distance)
            })
        });
        
        group.finish();
    }
}

/// Benchmark quantization operations
fn bench_quantization(c: &mut Criterion) {
    let _ = initialize_simdx();
    let simdx_context = get_simdx_context();
    let mut rng = StdRng::seed_from_u64(42);
    
    let dimensions = [128, 256, 512, 1024];
    let scale = 255.0;
    let offset = 0.0;
    
    for &dim in &dimensions {
        let vector = generate_random_vector(dim, &mut rng);
        
        let mut group = c.benchmark_group(format!("quantization_dim_{}", dim));
        group.throughput(Throughput::Elements(dim as u64));
        
        // SIMDX int8 quantization
        group.bench_function("simdx_int8", |bench| {
            bench.iter(|| {
                black_box(simdx_context.quantize_to_int8(black_box(&vector), scale, offset).unwrap())
            })
        });
        
        // SIMDX binary quantization
        group.bench_function("simdx_binary", |bench| {
            bench.iter(|| {
                black_box(simdx_context.binary_quantize(black_box(&vector)).unwrap())
            })
        });
        
        // Scalar int8 quantization
        group.bench_function("scalar_int8", |bench| {
            bench.iter(|| {
                let mut quantized = Vec::with_capacity(vector.len());
                for &val in &vector {
                    let scaled = (val * scale + offset).round();
                    let clamped = scaled.max(-128.0).min(127.0) as i8;
                    quantized.push(clamped);
                }
                black_box(quantized)
            })
        });
        
        // Scalar binary quantization
        group.bench_function("scalar_binary", |bench| {
            bench.iter(|| {
                let mean: f32 = vector.iter().sum::<f32>() / vector.len() as f32;
                let mut binary_vector = Vec::with_capacity((vector.len() + 7) / 8);
                
                for chunk in vector.chunks(8) {
                    let mut byte = 0u8;
                    for (i, &val) in chunk.iter().enumerate() {
                        if val > mean {
                            byte |= 1 << i;
                        }
                    }
                    binary_vector.push(byte);
                }
                black_box(binary_vector)
            })
        });
        
        group.finish();
    }
}

criterion_group!(
    name = simdx_benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(10))
        .sample_size(100);
    targets = 
        bench_cosine_distance,
        bench_euclidean_distance,
        bench_dot_product,
        bench_batch_cosine_distance,
        bench_vector_normalization,
        bench_batch_vector_normalization,
        bench_hamming_distance,
        bench_quantization
);

criterion_main!(simdx_benches);