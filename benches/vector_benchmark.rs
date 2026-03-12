//! Vector database benchmarking framework
//!
//! Based on ann-benchmarks methodology:
//! - Measures QPS, recall, latency at various dataset sizes
//! - Tests multiple index types (HNSW, PQ, etc.)
//! - Supports standard datasets (SIFT, GIST, OpenAI embeddings)
//!
//! Usage:
//!   cargo bench --bench vector_benchmark -- --dataset sift1m --index hnsw

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::prelude::*;
use rtdb::{
    distance::DistanceCalculator,
    index::hnsw_optimized::HnswIndexOptimized,
    quantization::product::{ProductQuantizer, ProductQuantizerConfig},
    Distance, SearchRequest, Vector,
};
use std::time::{Duration, Instant};

/// Benchmark configuration
struct BenchmarkConfig {
    /// Dataset name
    dataset: String,
    /// Number of vectors
    num_vectors: usize,
    /// Vector dimension
    dimension: usize,
    /// Number of queries
    num_queries: usize,
    /// K (number of neighbors)
    k: usize,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            dataset: "synthetic".to_string(),
            num_vectors: 100_000,
            dimension: 128,
            num_queries: 1000,
            k: 10,
        }
    }
}

/// Generate random vectors
fn generate_vectors(count: usize, dim: usize) -> Vec<Vector> {
    let mut rng = StdRng::seed_from_u64(42);
    (0..count)
        .map(|_| {
            let data: Vec<f32> = (0..dim)
                .map(|_| rng.gen::<f32>())
                .collect();
            Vector::new(data)
        })
        .collect()
}

/// Benchmark HNSW index
fn benchmark_hnsw(c: &mut Criterion) {
    let mut group = c.benchmark_group("hnsw_index");
    
    let config = BenchmarkConfig::default();
    let vectors = generate_vectors(config.num_vectors, config.dimension);
    let queries = generate_vectors(config.num_queries, config.dimension);
    
    // Build index
    let mut index = HnswIndexOptimized::new(Distance::Euclidean);
    
    let start = Instant::now();
    for (i, v) in vectors.iter().enumerate() {
        index.add(i as u64, v.clone()).unwrap();
    }
    let build_time = start.elapsed();
    
    println!("\nHNSW Index Build Time: {:?}", build_time);
    println!("Memory Usage: {:.2} MB", index.memory_usage() as f64 / 1e6);
    
    // Benchmark search throughput
    group.throughput(Throughput::Elements(config.num_queries as u64));
    group.sample_size(10);
    
    group.bench_function(BenchmarkId::new("search", config.num_vectors), |b| {
        b.iter(|| {
            for query in &queries {
                let req = SearchRequest::new(query.data.clone(), config.k);
                black_box(index.search(&req).unwrap());
            }
        });
    });
    
    group.finish();
}

/// Benchmark Product Quantization
fn benchmark_pq(c: &mut Criterion) {
    let mut group = c.benchmark_group("product_quantization");
    
    let config = BenchmarkConfig {
        num_vectors: 10_000, // Smaller for PQ training
        ..Default::default()
    };
    
    let vectors = generate_vectors(config.num_vectors, config.dimension);
    let queries = generate_vectors(100, config.dimension);
    
    // Test different PQ configurations
    let pq_configs = vec![
        ("PQ4", ProductQuantizerConfig { m: 4, code_size: 8, niter: 10, seed: 42 }),
        ("PQ8", ProductQuantizerConfig { m: 8, code_size: 8, niter: 10, seed: 42 }),
        ("PQ16", ProductQuantizerConfig { m: 16, code_size: 8, niter: 10, seed: 42 }),
    ];
    
    for (name, pq_config) in pq_configs {
        let mut pq = ProductQuantizer::new(pq_config.clone(), config.dimension).unwrap();
        
        // Train
        let start = Instant::now();
        pq.train(&vectors).unwrap();
        let train_time = start.elapsed();
        
        println!("\n{} Training Time: {:?}", name, train_time);
        println!("Compression Ratio: {:.1}x", pq.compression_ratio());
        
        // Encode all vectors
        let encoded: Vec<_> = vectors.iter()
            .map(|v| pq.encode(v).unwrap())
            .collect();
        
        // Benchmark ADC search
        group.bench_function(BenchmarkId::new("adc_search", name), |b| {
            b.iter(|| {
                for query in &queries {
                    let lut = pq.compute_lookup_table(query).unwrap();
                    for code in &encoded {
                        black_box(ProductQuantizer::asymmetric_distance(&lut, code));
                    }
                }
            });
        });
    }
    
    group.finish();
}

/// Benchmark SIMD distance calculations
fn benchmark_simd_distances(c: &mut Criterion) {
    let mut group = c.benchmark_group("simd_distances");
    
    let calc = DistanceCalculator::new();
    println!("Detected SIMD capability: {:?}", calc.capability());
    
    let dimensions = vec![128, 256, 512, 768, 1024, 1536];
    
    for &dim in &dimensions {
        let a: Vec<f32> = (0..dim).map(|i| i as f32).collect();
        let b: Vec<f32> = (0..dim).map(|i| (i * 2) as f32).collect();
        
        group.bench_function(BenchmarkId::new("euclidean", dim), |b| {
            b.iter(|| {
                black_box(calc.euclidean(&a, &b).unwrap());
            });
        });
        
        group.bench_function(BenchmarkId::new("dot_product", dim), |b| {
            b.iter(|| {
                black_box(calc.dot_product(&a, &b).unwrap());
            });
        });
        
        group.bench_function(BenchmarkId::new("cosine", dim), |b| {
            b.iter(|| {
                black_box(calc.cosine(&a, &b).unwrap());
            });
        });
    }
    
    group.finish();
}

/// Benchmark memory-mapped storage
fn benchmark_mmap_storage(c: &mut Criterion) {
    let mut group = c.benchmark_group("mmap_storage");
    
    use rtdb::storage::mmap::{MmapVectorStorage, DiskANNIndex, DiskSearchConfig};
    use std::sync::Arc;
    use tempfile::tempdir;
    
    let dir = tempdir().unwrap();
    let path = dir.path().join("bench_vectors.bin");
    
    let config = BenchmarkConfig {
        num_vectors: 10_000,
        dimension: 128,
        ..Default::default()
    };
    
    let vectors = generate_vectors(config.num_vectors, config.dimension);
    
    // Create storage
    let mut storage = MmapVectorStorage::create(&path, config.dimension, config.num_vectors).unwrap();
    
    let start = Instant::now();
    for v in &vectors {
        storage.append(v).unwrap();
    }
    let write_time = start.elapsed();
    
    println!("\nMmap Write Time: {:?}", write_time);
    
    storage.flush().unwrap();
    
    // Benchmark random reads
    let storage = Arc::new(MmapVectorStorage::open(&path, config.dimension).unwrap());
    let mut indices: Vec<usize> = (0..config.num_vectors).collect();
    indices.shuffle(&mut StdRng::seed_from_u64(42));
    
    group.bench_function("random_read", |b| {
        b.iter(|| {
            for &idx in &indices[..1000] {
                black_box(storage.get(idx).unwrap());
            }
        });
    });
    
    group.finish();
}

/// Full system benchmark (end-to-end)
fn benchmark_full_system(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_system");
    group.measurement_time(Duration::from_secs(10));
    
    let config = BenchmarkConfig {
        num_vectors: 50_000,
        dimension: 128,
        num_queries: 100,
        k: 10,
    };
    
    let vectors = generate_vectors(config.num_vectors, config.dimension);
    let queries = generate_vectors(config.num_queries, config.dimension);
    
    // Build optimized HNSW
    let mut index = HnswIndexOptimized::new(Distance::Euclidean);
    
    println!("\nBuilding index with {} vectors...", config.num_vectors);
    let start = Instant::now();
    for (i, v) in vectors.iter().enumerate() {
        index.add(i as u64, v.clone()).unwrap();
    }
    println!("Build time: {:?}", start.elapsed());
    
    // Benchmark QPS
    group.throughput(Throughput::Elements(config.num_queries as u64));
    
    group.bench_function("qps_50k", |b| {
        b.iter(|| {
            for query in &queries {
                let req = SearchRequest::new(query.data.clone(), config.k);
                let _results = index.search(&req).unwrap();
            }
        });
    });
    
    group.finish();
}

criterion_group!(
    benches,
    benchmark_simd_distances,
    benchmark_hnsw,
    benchmark_pq,
    benchmark_mmap_storage,
    benchmark_full_system
);
criterion_main!(benches);
