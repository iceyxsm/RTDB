//! SIMD-optimized migration benchmarks
//!
//! Benchmarks the performance of different migration strategies:
//! - SIMD vs scalar vector processing
//! - Parallel vs sequential processing
//! - Different batch sizes and memory configurations

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rtdb::migration::simd_optimized::*;
use rtdb::{Vector, VectorRecord};
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Generate test vectors for benchmarking
fn generate_test_vectors(count: usize, dimension: usize) -> Vec<Vector> {
    (0..count)
        .map(|i| {
            let data: Vec<f32> = (0..dimension)
                .map(|j| ((i * dimension + j) as f32).sin())
                .collect();
            Vector { data, payload: None }
        })
        .collect()
}

/// Generate test records for benchmarking
fn generate_test_records(count: usize, dimension: usize) -> Vec<VectorRecord> {
    let vectors = generate_test_vectors(count, dimension);
    vectors
        .into_iter()
        .enumerate()
        .map(|(i, vector)| VectorRecord {
            id: i as u64,
            vector,
            payload: None,
        })
        .collect()
}

/// Benchmark SIMD vector processing
fn bench_simd_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("simd_processing");
    
    // Test different vector dimensions
    for dimension in [128, 384, 768, 1536].iter() {
        // Test different batch sizes
        for batch_size in [64, 256, 1024, 4096].iter() {
            let vectors = generate_test_vectors(*batch_size, *dimension);
            let records = generate_test_records(*batch_size, *dimension);
            
            group.throughput(Throughput::Elements(*batch_size as u64));
            
            // Benchmark AVX-512 processing (if available)
            #[cfg(target_arch = "x86_64")]
            {
                if is_x86_feature_detected!("avx512f") {
                    group.bench_with_input(
                        BenchmarkId::new("avx512", format!("{}d_{}batch", dimension, batch_size)),
                        &(*dimension, *batch_size),
                        |b, &(dim, batch)| {
                            let vectors = generate_test_vectors(batch, dim);
                            let records = generate_test_records(batch, dim);
                            
                            b.iter(|| {
                                black_box(
                                    rtdb::migration::simd_optimized::process_vectors_avx512(
                                        &vectors, &records
                                    ).unwrap()
                                )
                            });
                        },
                    );
                }
            }
            
            // Benchmark AVX2 processing
            #[cfg(target_arch = "x86_64")]
            {
                if is_x86_feature_detected!("avx2") {
                    group.bench_with_input(
                        BenchmarkId::new("avx2", format!("{}d_{}batch", dimension, batch_size)),
                        &(*dimension, *batch_size),
                        |b, &(dim, batch)| {
                            let vectors = generate_test_vectors(batch, dim);
                            let records = generate_test_records(batch, dim);
                            
                            b.iter(|| {
                                black_box(
                                    rtdb::migration::simd_optimized::process_vectors_avx2(
                                        &vectors, &records
                                    ).unwrap()
                                )
                            });
                        },
                    );
                }
            }
            
            // Benchmark NEON processing
            #[cfg(target_arch = "aarch64")]
            {
                group.bench_with_input(
                    BenchmarkId::new("neon", format!("{}d_{}batch", dimension, batch_size)),
                    &(*dimension, *batch_size),
                    |b, &(dim, batch)| {
                        let vectors = generate_test_vectors(batch, dim);
                        let records = generate_test_records(batch, dim);
                        
                        b.iter(|| {
                            black_box(
                                rtdb::migration::simd_optimized::process_vectors_neon(
                                    &vectors, &records
                                ).unwrap()
                            )
                        });
                    },
                );
            }
            
            // Benchmark scalar processing (baseline)
            group.bench_with_input(
                BenchmarkId::new("scalar", format!("{}d_{}batch", dimension, batch_size)),
                &(*dimension, *batch_size),
                |b, &(dim, batch)| {
                    let vectors = generate_test_vectors(batch, dim);
                    let records = generate_test_records(batch, dim);
                    
                    b.iter(|| {
                        black_box(
                            rtdb::migration::simd_optimized::process_vectors_scalar(
                                &vectors, &records
                            ).unwrap()
                        )
                    });
                },
            );
        }
    }
    
    group.finish();
}
/// Benchmark parallel migration processing
fn bench_parallel_migration(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("parallel_migration");
    
    // Test different worker counts
    for workers in [1, 2, 4, 8, 16].iter() {
        // Test different total record counts
        for total_records in [10_000, 100_000, 1_000_000].iter() {
            group.throughput(Throughput::Elements(*total_records as u64));
            
            group.bench_with_input(
                BenchmarkId::new("workers", format!("{}_workers_{}k_records", workers, total_records / 1000)),
                &(*workers, *total_records),
                |b, &(worker_count, record_count)| {
                    b.to_async(&rt).iter(|| async {
                        let config = SimdMigrationConfig {
                            worker_threads: worker_count,
                            batch_size: 1024,
                            memory_limit_per_worker: 128 * 1024 * 1024, // 128MB
                            enable_simd: true,
                            checkpoint_interval: 50_000,
                            max_retries: 3,
                            operation_timeout_secs: 30,
                        };
                        
                        let engine = SimdMigrationEngine::new(config, record_count as u64);
                        let source = MockMigrationSource::new(record_count, 768);
                        let target = MockMigrationTarget::new();
                        
                        black_box(engine.migrate(source, target).await.unwrap())
                    });
                },
            );
        }
    }
    
    group.finish();
}

/// Benchmark memory usage patterns
fn bench_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage");
    
    // Test different memory limits per worker
    for memory_limit_mb in [64, 128, 256, 512, 1024].iter() {
        // Test different batch sizes
        for batch_size in [256, 512, 1024, 2048, 4096].iter() {
            let memory_limit = memory_limit_mb * 1024 * 1024;
            
            group.bench_with_input(
                BenchmarkId::new("memory_limit", format!("{}mb_{}batch", memory_limit_mb, batch_size)),
                &(memory_limit, *batch_size),
                |b, &(mem_limit, batch)| {
                    let vectors = generate_test_vectors(batch, 1536); // OpenAI embedding size
                    let records = generate_test_records(batch, 1536);
                    
                    b.iter(|| {
                        let mut simd_batch = SimdVectorBatch::new(
                            0,
                            "source".to_string(),
                            "target".to_string(),
                        );
                        
                        for (vector, record) in vectors.iter().zip(records.iter()) {
                            simd_batch.add_vector(vector.clone(), record.clone());
                            
                            // Check memory usage
                            if simd_batch.memory_usage() >= mem_limit {
                                break;
                            }
                        }
                        
                        black_box(simd_batch.memory_usage())
                    });
                },
            );
        }
    }
    
    group.finish();
}

/// Benchmark checkpoint performance
fn bench_checkpoint_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("checkpoint_performance");
    
    // Test different checkpoint intervals
    for interval in [1_000, 10_000, 100_000, 1_000_000].iter() {
        group.bench_with_input(
            BenchmarkId::new("checkpoint_interval", format!("{}_records", interval)),
            interval,
            |b, &checkpoint_interval| {
                b.to_async(&rt).iter(|| async {
                    let mut checkpoint_manager = CheckpointManager::new();
                    
                    // Simulate creating checkpoints
                    for i in (0..checkpoint_interval).step_by(checkpoint_interval / 10) {
                        black_box(
                            checkpoint_manager.create_checkpoint(i as u64).await.unwrap()
                        );
                    }
                });
            },
        );
    }
    
    group.finish();
}

/// Mock migration source for benchmarking
struct MockMigrationSource {
    records: Vec<VectorRecord>,
    current_index: usize,
}

impl MockMigrationSource {
    fn new(count: usize, dimension: usize) -> Self {
        Self {
            records: generate_test_records(count, dimension),
            current_index: 0,
        }
    }
}

#[async_trait::async_trait]
impl MigrationSource for MockMigrationSource {
    async fn next_record(&mut self) -> Result<Option<VectorRecord>, rtdb::RTDBError> {
        if self.current_index < self.records.len() {
            let record = self.records[self.current_index].clone();
            self.current_index += 1;
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }
    
    async fn total_count(&self) -> Result<Option<u64>, rtdb::RTDBError> {
        Ok(Some(self.records.len() as u64))
    }
}

/// Mock migration target for benchmarking
#[derive(Clone)]
struct MockMigrationTarget {
    records_written: Arc<std::sync::atomic::AtomicU64>,
}

impl MockMigrationTarget {
    fn new() -> Self {
        Self {
            records_written: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }
}

#[async_trait::async_trait]
impl MigrationTarget for MockMigrationTarget {
    async fn write_batch(&self, records: &[VectorRecord]) -> Result<(), rtdb::RTDBError> {
        // Simulate write latency
        tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;
        
        self.records_written.fetch_add(
            records.len() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        
        Ok(())
    }
    
    async fn flush(&self) -> Result<(), rtdb::RTDBError> {
        // Simulate flush latency
        tokio::time::sleep(tokio::time::Duration::from_micros(50)).await;
        Ok(())
    }
}

criterion_group!(
    benches,
    bench_simd_processing,
    bench_parallel_migration,
    bench_memory_usage,
    bench_checkpoint_performance
);
criterion_main!(benches);