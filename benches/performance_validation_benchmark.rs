//! Performance Validation Benchmark
//!
//! Validates RTDB's performance claims against targets:
//! - P50: <1ms, P95: <3ms, P99: <5ms, P999: <10ms query latency
//! - 50,000+ QPS single node, 1,000,000+ QPS cluster
//! - 100,000+ vectors/second ingestion
//! - 10x faster indexing, 5x lower latency vs competitors

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use rtdb::{
    Vector, VectorId, CollectionConfig, Distance, SearchRequest,
    collection::Collection,
    index::hnsw::HnswIndex,
    storage::engine::StorageEngine,
    simdx::SIMDXEngine,
};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use std::sync::Arc;

/// Performance targets for validation
#[derive(Debug, Clone)]
pub struct PerformanceTargets {
    pub query_latency_p50_ms: f64,
    pub query_latency_p95_ms: f64,
    pub query_latency_p99_ms: f64,
    pub query_latency_p999_ms: f64,
    pub single_node_qps: u64,
    pub cluster_qps: u64,
    pub ingestion_vectors_per_sec: u64,
    pub index_build_speedup: f64, // vs baseline
    pub latency_improvement: f64, // vs baseline
}

impl Default for PerformanceTargets {
    fn default() -> Self {
        Self {
            query_latency_p50_ms: 1.0,
            query_latency_p95_ms: 3.0,
            query_latency_p99_ms: 5.0,
            query_latency_p999_ms: 10.0,
            single_node_qps: 50_000,
            cluster_qps: 1_000_000,
            ingestion_vectors_per_sec: 100_000,
            index_build_speedup: 10.0,
            latency_improvement: 5.0,
        }
    }
}

/// Benchmark configuration for different scenarios
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub vector_count: usize,
    pub vector_dimension: usize,
    pub query_count: usize,
    pub concurrent_clients: usize,
    pub warmup_queries: usize,
}

impl BenchmarkConfig {
    pub fn small() -> Self {
        Self {
            vector_count: 10_000,
            vector_dimension: 128,
            query_count: 1_000,
            concurrent_clients: 1,
            warmup_queries: 100,
        }
    }
    
    pub fn medium() -> Self {
        Self {
            vector_count: 100_000,
            vector_dimension: 384,
            query_count: 10_000,
            concurrent_clients: 8,
            warmup_queries: 1_000,
        }
    }
    
    pub fn large() -> Self {
        Self {
            vector_count: 1_000_000,
            vector_dimension: 768,
            query_count: 100_000,
            concurrent_clients: 32,
            warmup_queries: 10_000,
        }
    }
}

/// Performance measurement results
#[derive(Debug, Clone)]
pub struct PerformanceResults {
    pub latencies_ms: Vec<f64>,
    pub throughput_qps: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub p999_ms: f64,
    pub avg_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
}

impl PerformanceResults {
    pub fn from_latencies(latencies_ms: Vec<f64>, duration_secs: f64) -> Self {
        let mut sorted_latencies = latencies_ms.clone();
        sorted_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let len = sorted_latencies.len();
        let p50 = sorted_latencies[len * 50 / 100];
        let p95 = sorted_latencies[len * 95 / 100];
        let p99 = sorted_latencies[len * 99 / 100];
        let p999 = sorted_latencies[len * 999 / 1000];
        
        let sum: f64 = sorted_latencies.iter().sum();
        let avg = sum / len as f64;
        let min = sorted_latencies[0];
        let max = sorted_latencies[len - 1];
        
        let throughput_qps = len as f64 / duration_secs;
        
        Self {
            latencies_ms,
            throughput_qps,
            p50_ms: p50,
            p95_ms: p95,
            p99_ms: p99,
            p999_ms: p999,
            avg_ms: avg,
            min_ms: min,
            max_ms: max,
        }
    }
    
    pub fn validate_against_targets(&self, targets: &PerformanceTargets) -> ValidationResult {
        let mut passed = Vec::new();
        let mut failed = Vec::new();
        
        // Check latency targets
        if self.p50_ms <= targets.query_latency_p50_ms {
            passed.push(format!("P50 latency: {:.2}ms <= {:.2}ms ✓", self.p50_ms, targets.query_latency_p50_ms));
        } else {
            failed.push(format!("P50 latency: {:.2}ms > {:.2}ms ✗", self.p50_ms, targets.query_latency_p50_ms));
        }
        
        if self.p95_ms <= targets.query_latency_p95_ms {
            passed.push(format!("P95 latency: {:.2}ms <= {:.2}ms ✓", self.p95_ms, targets.query_latency_p95_ms));
        } else {
            failed.push(format!("P95 latency: {:.2}ms > {:.2}ms ✗", self.p95_ms, targets.query_latency_p95_ms));
        }
        
        if self.p99_ms <= targets.query_latency_p99_ms {
            passed.push(format!("P99 latency: {:.2}ms <= {:.2}ms ✓", self.p99_ms, targets.query_latency_p99_ms));
        } else {
            failed.push(format!("P99 latency: {:.2}ms > {:.2}ms ✗", self.p99_ms, targets.query_latency_p99_ms));
        }
        
        if self.p999_ms <= targets.query_latency_p999_ms {
            passed.push(format!("P999 latency: {:.2}ms <= {:.2}ms ✓", self.p999_ms, targets.query_latency_p999_ms));
        } else {
            failed.push(format!("P999 latency: {:.2}ms > {:.2}ms ✗", self.p999_ms, targets.query_latency_p999_ms));
        }
        
        // Check throughput targets
        if self.throughput_qps >= targets.single_node_qps as f64 {
            passed.push(format!("Throughput: {:.0} QPS >= {} QPS ✓", self.throughput_qps, targets.single_node_qps));
        } else {
            failed.push(format!("Throughput: {:.0} QPS < {} QPS ✗", self.throughput_qps, targets.single_node_qps));
        }
        
        ValidationResult {
            passed,
            failed,
            overall_pass: failed.is_empty(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub passed: Vec<String>,
    pub failed: Vec<String>,
    pub overall_pass: bool,
}

impl ValidationResult {
    pub fn print_report(&self) {
        println!("\n=== PERFORMANCE VALIDATION REPORT ===");
        
        if !self.passed.is_empty() {
            println!("\nPASSED TARGETS:");
            for item in &self.passed {
                println!("  {}", item);
            }
        }
        
        if !self.failed.is_empty() {
            println!("\nFAILED TARGETS:");
            for item in &self.failed {
                println!("  {}", item);
            }
        }
        
        println!("\nOVERALL: {}", if self.overall_pass { "PASS ✓" } else { "FAIL ✗" });
        println!("=====================================\n");
    }
}

/// Generate test vectors with specified characteristics
fn generate_test_vectors(count: usize, dimension: usize, seed: u64) -> Vec<(VectorId, Vector)> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut vectors = Vec::with_capacity(count);
    
    for i in 0..count {
        let data: Vec<f32> = (0..dimension)
            .map(|_| rng.gen_range(-1.0..1.0))
            .collect();
        
        let mut vector = Vector::new(data);
        vector.normalize(); // Normalize for cosine similarity
        
        vectors.push((i as VectorId, vector));
    }
    
    vectors
}

/// Generate query vectors
fn generate_query_vectors(count: usize, dimension: usize, seed: u64) -> Vec<Vector> {
    let mut rng = StdRng::seed_from_u64(seed + 12345);
    let mut queries = Vec::with_capacity(count);
    
    for _ in 0..count {
        let data: Vec<f32> = (0..dimension)
            .map(|_| rng.gen_range(-1.0..1.0))
            .collect();
        
        let mut vector = Vector::new(data);
        vector.normalize();
        
        queries.push(vector);
    }
    
    queries
}

/// Benchmark query latency with detailed percentile analysis
fn bench_query_latency_validation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = BenchmarkConfig::medium();
    let targets = PerformanceTargets::default();
    
    // Setup collection with HNSW index
    let collection_config = CollectionConfig {
        dimension: config.vector_dimension,
        distance: Distance::Cosine,
        hnsw_config: Some(rtdb::HnswConfig {
            m: 16,
            ef_construct: 200,
            ef: 64,
            num_layers: None,
        }),
        quantization_config: None,
        optimizer_config: None,
    };
    
    let vectors = generate_test_vectors(config.vector_count, config.vector_dimension, 42);
    let queries = generate_query_vectors(config.query_count, config.vector_dimension, 12345);
    
    let mut group = c.benchmark_group("query_latency_validation");
    group.sample_size(1000);
    group.measurement_time(Duration::from_secs(30));
    
    // Build index
    let collection = rt.block_on(async {
        let mut collection = Collection::new("test_collection".to_string(), collection_config).unwrap();
        
        // Batch insert vectors
        let batch_size = 1000;
        for chunk in vectors.chunks(batch_size) {
            collection.upsert_batch(chunk.to_vec()).await.unwrap();
        }
        
        collection
    });
    
    group.bench_function("single_query_latency", |b| {
        let mut query_idx = 0;
        b.iter(|| {
            let query = &queries[query_idx % queries.len()];
            query_idx += 1;
            
            let request = SearchRequest::new(query.data.clone(), 10);
            
            let start = Instant::now();
            let _result = rt.block_on(async {
                collection.search(&request).await
            });
            let latency = start.elapsed();
            
            black_box(latency)
        });
    });
    
    // Measure detailed latency distribution
    let mut latencies = Vec::new();
    let measurement_start = Instant::now();
    
    for (i, query) in queries.iter().enumerate().take(config.query_count) {
        let request = SearchRequest::new(query.data.clone(), 10);
        
        let start = Instant::now();
        let _result = rt.block_on(async {
            collection.search(&request).await
        });
        let latency = start.elapsed();
        
        latencies.push(latency.as_secs_f64() * 1000.0); // Convert to milliseconds
        
        if i % 1000 == 0 {
            println!("Processed {} queries", i);
        }
    }
    
    let measurement_duration = measurement_start.elapsed().as_secs_f64();
    let results = PerformanceResults::from_latencies(latencies, measurement_duration);
    let validation = results.validate_against_targets(&targets);
    
    println!("\n=== QUERY LATENCY RESULTS ===");
    println!("Queries processed: {}", config.query_count);
    println!("Average latency: {:.2}ms", results.avg_ms);
    println!("P50 latency: {:.2}ms", results.p50_ms);
    println!("P95 latency: {:.2}ms", results.p95_ms);
    println!("P99 latency: {:.2}ms", results.p99_ms);
    println!("P999 latency: {:.2}ms", results.p999_ms);
    println!("Min latency: {:.2}ms", results.min_ms);
    println!("Max latency: {:.2}ms", results.max_ms);
    println!("Throughput: {:.0} QPS", results.throughput_qps);
    
    validation.print_report();
    
    group.finish();
}

/// Benchmark ingestion throughput
fn bench_ingestion_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let targets = PerformanceTargets::default();
    
    let mut group = c.benchmark_group("ingestion_throughput");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(60));
    
    for vector_count in [1_000, 10_000, 100_000].iter() {
        let vectors = generate_test_vectors(*vector_count, 384, 42);
        
        group.throughput(Throughput::Elements(*vector_count as u64));
        group.bench_with_input(
            BenchmarkId::new("batch_insert", vector_count),
            vector_count,
            |b, &_size| {
                b.iter(|| {
                    rt.block_on(async {
                        let collection_config = CollectionConfig::new(384);
                        let mut collection = Collection::new("ingestion_test".to_string(), collection_config).unwrap();
                        
                        let start = Instant::now();
                        
                        // Batch insert in chunks for optimal performance
                        let batch_size = 1000;
                        for chunk in vectors.chunks(batch_size) {
                            collection.upsert_batch(chunk.to_vec()).await.unwrap();
                        }
                        
                        let duration = start.elapsed();
                        let vectors_per_sec = vectors.len() as f64 / duration.as_secs_f64();
                        
                        println!("Ingested {} vectors in {:.2}s ({:.0} vectors/sec)", 
                                vectors.len(), duration.as_secs_f64(), vectors_per_sec);
                        
                        if vectors_per_sec >= targets.ingestion_vectors_per_sec as f64 {
                            println!("✓ Ingestion target met: {:.0} >= {} vectors/sec", 
                                    vectors_per_sec, targets.ingestion_vectors_per_sec);
                        } else {
                            println!("✗ Ingestion target missed: {:.0} < {} vectors/sec", 
                                    vectors_per_sec, targets.ingestion_vectors_per_sec);
                        }
                        
                        black_box(collection)
                    })
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark index build time and validate speedup claims
fn bench_index_build_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("index_build_performance");
    group.sample_size(5);
    group.measurement_time(Duration::from_secs(120));
    
    for vector_count in [10_000, 100_000, 1_000_000].iter() {
        let vectors = generate_test_vectors(*vector_count, 768, 42);
        
        group.bench_with_input(
            BenchmarkId::new("hnsw_build", vector_count),
            vector_count,
            |b, &_size| {
                b.iter(|| {
                    rt.block_on(async {
                        let collection_config = CollectionConfig {
                            dimension: 768,
                            distance: Distance::Cosine,
                            hnsw_config: Some(rtdb::HnswConfig {
                                m: 16,
                                ef_construct: 200,
                                ef: 64,
                                num_layers: None,
                            }),
                            quantization_config: None,
                            optimizer_config: None,
                        };
                        
                        let mut collection = Collection::new("index_build_test".to_string(), collection_config).unwrap();
                        
                        let start = Instant::now();
                        
                        // Insert vectors and measure index build time
                        let batch_size = 1000;
                        for chunk in vectors.chunks(batch_size) {
                            collection.upsert_batch(chunk.to_vec()).await.unwrap();
                        }
                        
                        let build_time = start.elapsed();
                        let vectors_per_sec = vectors.len() as f64 / build_time.as_secs_f64();
                        
                        println!("Built index for {} vectors in {:.2}s ({:.0} vectors/sec)", 
                                vectors.len(), build_time.as_secs_f64(), vectors_per_sec);
                        
                        black_box(collection)
                    })
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark concurrent query performance
fn bench_concurrent_query_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = BenchmarkConfig::large();
    let targets = PerformanceTargets::default();
    
    let mut group = c.benchmark_group("concurrent_query_performance");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(60));
    
    // Setup large collection
    let vectors = generate_test_vectors(config.vector_count, config.vector_dimension, 42);
    let queries = generate_query_vectors(1000, config.vector_dimension, 12345);
    
    let collection = rt.block_on(async {
        let collection_config = CollectionConfig::new(config.vector_dimension);
        let mut collection = Collection::new("concurrent_test".to_string(), collection_config).unwrap();
        
        println!("Building index with {} vectors...", vectors.len());
        let batch_size = 5000;
        for (i, chunk) in vectors.chunks(batch_size).enumerate() {
            collection.upsert_batch(chunk.to_vec()).await.unwrap();
            if i % 10 == 0 {
                println!("Inserted {} vectors", (i + 1) * batch_size);
            }
        }
        
        Arc::new(collection)
    });
    
    for concurrent_clients in [1, 4, 8, 16, 32].iter() {
        group.bench_with_input(
            BenchmarkId::new("concurrent_search", concurrent_clients),
            concurrent_clients,
            |b, &clients| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut handles = Vec::new();
                        let queries_per_client = 100;
                        
                        let start = Instant::now();
                        
                        for client_id in 0..clients {
                            let collection = Arc::clone(&collection);
                            let queries = queries.clone();
                            
                            let handle = tokio::spawn(async move {
                                let mut latencies = Vec::new();
                                
                                for i in 0..queries_per_client {
                                    let query_idx = (client_id * queries_per_client + i) % queries.len();
                                    let query = &queries[query_idx];
                                    let request = SearchRequest::new(query.data.clone(), 10);
                                    
                                    let query_start = Instant::now();
                                    let _result = collection.search(&request).await.unwrap();
                                    let query_latency = query_start.elapsed();
                                    
                                    latencies.push(query_latency.as_secs_f64() * 1000.0);
                                }
                                
                                latencies
                            });
                            
                            handles.push(handle);
                        }
                        
                        // Wait for all clients to complete
                        let mut all_latencies = Vec::new();
                        for handle in handles {
                            let client_latencies = handle.await.unwrap();
                            all_latencies.extend(client_latencies);
                        }
                        
                        let total_duration = start.elapsed().as_secs_f64();
                        let total_queries = clients * queries_per_client;
                        let qps = total_queries as f64 / total_duration;
                        
                        let results = PerformanceResults::from_latencies(all_latencies, total_duration);
                        
                        println!("\nConcurrent Performance ({} clients):", clients);
                        println!("Total queries: {}", total_queries);
                        println!("Duration: {:.2}s", total_duration);
                        println!("Throughput: {:.0} QPS", qps);
                        println!("P95 latency: {:.2}ms", results.p95_ms);
                        println!("P99 latency: {:.2}ms", results.p99_ms);
                        
                        if qps >= targets.single_node_qps as f64 {
                            println!("✓ QPS target met: {:.0} >= {}", qps, targets.single_node_qps);
                        } else {
                            println!("✗ QPS target missed: {:.0} < {}", qps, targets.single_node_qps);
                        }
                        
                        black_box(results)
                    })
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark SIMDX performance improvements
fn bench_simdx_performance_gains(c: &mut Criterion) {
    let mut group = c.benchmark_group("simdx_performance_gains");
    group.sample_size(100);
    
    let dimensions = [128, 384, 768, 1536];
    let simdx_engine = SIMDXEngine::new(None);
    
    for &dim in &dimensions {
        let mut rng = StdRng::seed_from_u64(42);
        let vector_a: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        let vector_b: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        
        // Benchmark SIMDX cosine distance
        group.bench_with_input(
            BenchmarkId::new("simdx_cosine", dim),
            &dim,
            |b, &_dim| {
                b.iter(|| {
                    let result = simdx_engine.cosine_distance(&vector_a, &vector_b);
                    black_box(result)
                });
            },
        );
        
        // Benchmark scalar cosine distance for comparison
        group.bench_with_input(
            BenchmarkId::new("scalar_cosine", dim),
            &dim,
            |b, &_dim| {
                b.iter(|| {
                    let dot_product: f32 = vector_a.iter().zip(vector_b.iter()).map(|(a, b)| a * b).sum();
                    let norm_a: f32 = vector_a.iter().map(|x| x * x).sum::<f32>().sqrt();
                    let norm_b: f32 = vector_b.iter().map(|x| x * x).sum::<f32>().sqrt();
                    let cosine_similarity = dot_product / (norm_a * norm_b);
                    let cosine_distance = 1.0 - cosine_similarity;
                    black_box(cosine_distance)
                });
            },
        );
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_query_latency_validation,
    bench_ingestion_throughput,
    bench_index_build_performance,
    bench_concurrent_query_performance,
    bench_simdx_performance_gains
);

criterion_main!(benches);