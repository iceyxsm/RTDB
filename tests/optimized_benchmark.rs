//! Benchmark for Optimized Direct Jepsen Client

use rtdb::jepsen::direct_client_optimized::{OptimizedDirectJepsenClient, OptimizedClientConfig};
use rtdb::jepsen::{JepsenClient, OperationType};
use std::time::Instant;

#[tokio::test]
async fn benchmark_optimized_vs_standard() {
    println!("\n=== OPTIMIZED vs STANDARD PERFORMANCE ===\n");
    
    let operations = 5000;
    
    // Test optimized client (1-dimension, batched)
    println!("Testing Optimized Client (1-dim, batched)...");
    let optimized_config = OptimizedClientConfig {
        vector_dim: 1,
        batch_size: 100,
        flush_interval_ms: 10,
        enable_pooling: true,
        in_memory_only: true,
    };
    let optimized = OptimizedDirectJepsenClient::with_config(0, optimized_config).await.unwrap();
    
    let start = Instant::now();
    for i in 0..operations {
        optimized.execute(
            OperationType::Write { 
                key: format!("opt_key_{}", i), 
                value: serde_json::json!(i) 
            }
        ).await.unwrap();
    }
    optimized.flush().await.unwrap();
    let opt_duration = start.elapsed();
    let opt_ops = operations as f64 / opt_duration.as_secs_f64();
    
    println!("  Operations: {}", operations);
    println!("  Duration: {:?}", opt_duration);
    println!("  Throughput: {:.2} ops/sec", opt_ops);
    println!("  Latency: {:.3} ms/op", opt_duration.as_millis() as f64 / operations as f64);
    
    // Verify reads work
    let start = Instant::now();
    for i in 0..100 {
        let result = optimized.execute(
            OperationType::Read { 
                key: format!("opt_key_{}", i) 
            }
        ).await.unwrap();
        
        match result {
            rtdb::jepsen::OperationResult::ReadOk { value: Some(v) } => {
                assert_eq!(v, serde_json::json!(i));
            }
            _ => panic!("Expected to read back value {}", i),
        }
    }
    let read_duration = start.elapsed();
    let read_ops = 100.0 / read_duration.as_secs_f64();
    
    println!("  Read Throughput (100 ops): {:.2} ops/sec", read_ops);
    
    // Test standard client (128-dim, no batching)
    println!("\nTesting Standard Direct Client (128-dim, no batching)...");
    let standard = rtdb::jepsen::direct_client::DirectJepsenClient::new(1, 128).await.unwrap();
    
    let start = Instant::now();
    for i in 0..operations {
        standard.execute(
            OperationType::Write { 
                key: format!("std_key_{}", i), 
                value: serde_json::json!(i) 
            }
        ).await.unwrap();
    }
    let std_duration = start.elapsed();
    let std_ops = operations as f64 / std_duration.as_secs_f64();
    
    println!("  Operations: {}", operations);
    println!("  Duration: {:?}", std_duration);
    println!("  Throughput: {:.2} ops/sec", std_ops);
    println!("  Latency: {:.3} ms/op", std_duration.as_millis() as f64 / operations as f64);
    
    // Summary
    println!("\n=== SUMMARY ===");
    println!("Optimized Client:  {:.2} ops/sec", opt_ops);
    println!("Standard Client:   {:.2} ops/sec", std_ops);
    println!("Speedup:           {:.1}x", opt_ops / std_ops);
    
    // The optimized client should be significantly faster
    assert!(
        opt_ops > std_ops * 2.0,
        "Optimized client should be at least 2x faster than standard client"
    );
}

#[tokio::test]
async fn benchmark_batch_sizes() {
    println!("\n=== BATCH SIZE COMPARISON ===\n");
    
    let operations = 2000;
    let batch_sizes = vec![1, 10, 50, 100, 200];
    
    for batch_size in batch_sizes {
        let config = OptimizedClientConfig {
            vector_dim: 1,
            batch_size,
            flush_interval_ms: 100, // Longer interval to allow batching
            enable_pooling: true,
            in_memory_only: true,
        };
        
        let client = OptimizedDirectJepsenClient::with_config(batch_size, config).await.unwrap();
        
        let start = Instant::now();
        for i in 0..operations {
            client.execute(
                OperationType::Write { 
                    key: format!("key_{}_{}", batch_size, i), 
                    value: serde_json::json!(i) 
                }
            ).await.unwrap();
        }
        client.flush().await.unwrap();
        let duration = start.elapsed();
        let ops = operations as f64 / duration.as_secs_f64();
        
        println!("Batch size {:3}: {:8.2} ops/sec  ({:?})", batch_size, ops, duration);
    }
}

#[tokio::test]
async fn benchmark_vector_dimensions() {
    println!("\n=== VECTOR DIMENSION COMPARISON ===\n");
    
    let operations = 1000;
    let dimensions = vec![1, 4, 16, 64, 128, 256];
    
    for dim in dimensions {
        let config = OptimizedClientConfig {
            vector_dim: dim,
            batch_size: 50,
            flush_interval_ms: 10,
            enable_pooling: true,
            in_memory_only: true,
        };
        
        let client = OptimizedDirectJepsenClient::with_config(dim, config).await.unwrap();
        
        let start = Instant::now();
        for i in 0..operations {
            client.execute(
                OperationType::Write { 
                    key: format!("key_{}_{}", dim, i), 
                    value: serde_json::json!(i) 
                }
            ).await.unwrap();
        }
        client.flush().await.unwrap();
        let duration = start.elapsed();
        let ops = operations as f64 / duration.as_secs_f64();
        
        println!("Dimension {:3}: {:8.2} ops/sec  ({:?})", dim, ops, duration);
    }
}
