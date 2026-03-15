//! Microbenchmark for DirectJepsenClient
//! 
//! This runs longer operations to get more accurate measurements in debug mode.

use rtdb::jepsen::direct_client::DirectJepsenClient;
use rtdb::jepsen::{JepsenClient, OperationType};
use std::time::Instant;

#[tokio::test]
async fn microbenchmark_write_throughput() {
    println!("\n=== MICROBENCHMARK: WRITE THROUGHPUT ===\n");
    
    // Test different batch sizes
    let operations = 500;
    
    // Test with smaller vector dimension
    println!("Testing with 16-dimension vectors (smaller payload)...");
    let client_small = DirectJepsenClient::new(0, 16).await.unwrap();
    
    let start = Instant::now();
    for i in 0..operations {
        client_small.execute(
            OperationType::Write { 
                key: format!("key_{}", i), 
                value: serde_json::json!(i) 
            }
        ).await.unwrap();
    }
    let duration_small = start.elapsed();
    let ops_small = operations as f64 / duration_small.as_secs_f64();
    
    println!("  Vector dim: 16");
    println!("  Operations: {}", operations);
    println!("  Duration: {:?}", duration_small);
    println!("  Throughput: {:.2} ops/sec", ops_small);
    println!("  Latency: {:.2} ms/op", duration_small.as_millis() as f64 / operations as f64);
    
    // Test with standard 128-dimension vectors
    println!("\nTesting with 128-dimension vectors (standard)...");
    let client_standard = DirectJepsenClient::new(1, 128).await.unwrap();
    
    let start = Instant::now();
    for i in 0..operations {
        client_standard.execute(
            OperationType::Write { 
                key: format!("key_{}", i), 
                value: serde_json::json!(i) 
            }
        ).await.unwrap();
    }
    let duration_standard = start.elapsed();
    let ops_standard = operations as f64 / duration_standard.as_secs_f64();
    
    println!("  Vector dim: 128");
    println!("  Operations: {}", operations);
    println!("  Duration: {:?}", duration_standard);
    println!("  Throughput: {:.2} ops/sec", ops_standard);
    println!("  Latency: {:.2} ms/op", duration_standard.as_millis() as f64 / operations as f64);
    
    // Calculate overhead per dimension
    let dim_overhead = (duration_standard.as_secs_f64() - duration_small.as_secs_f64()) / (128.0 - 16.0);
    println!("\n=== ANALYSIS ===");
    println!("  Per-dimension overhead: {:.4} ms", dim_overhead * 1000.0);
    println!("  128-dim is {:.1}x slower than 16-dim", duration_standard.as_secs_f64() / duration_small.as_secs_f64());
    
    // Estimate release mode performance (typically 10-20x faster)
    println!("\n=== ESTIMATED RELEASE MODE PERFORMANCE ===");
    println!("  16-dim vectors:   {:.0} - {:.0} ops/sec", ops_small * 10.0, ops_small * 20.0);
    println!("  128-dim vectors:  {:.0} - {:.0} ops/sec", ops_standard * 10.0, ops_standard * 20.0);
}

#[tokio::test]
async fn microbenchmark_read_vs_write() {
    println!("\n=== MICROBENCHMARK: READ vs WRITE ===\n");
    
    let operations = 1000;
    let client = DirectJepsenClient::new(2, 128).await.unwrap();
    
    // Pre-populate data
    println!("Pre-populating {} records...", operations);
    for i in 0..operations {
        client.execute(
            OperationType::Write { 
                key: format!("key_{}", i), 
                value: serde_json::json!(i) 
            }
        ).await.unwrap();
    }
    
    // Benchmark reads
    println!("\nBenchmarking reads...");
    let start = Instant::now();
    for i in 0..operations {
        client.execute(
            OperationType::Read { 
                key: format!("key_{}", i) 
            }
        ).await.unwrap();
    }
    let read_duration = start.elapsed();
    let read_ops = operations as f64 / read_duration.as_secs_f64();
    
    println!("  Read throughput: {:.2} ops/sec", read_ops);
    println!("  Read latency: {:.3} ms/op", read_duration.as_millis() as f64 / operations as f64);
    
    // Benchmark writes to same keys (updates)
    println!("\nBenchmarking writes (updates to existing keys)...");
    let start = Instant::now();
    for i in 0..operations {
        client.execute(
            OperationType::Write { 
                key: format!("key_{}", i), 
                value: serde_json::json!(i * 2) 
            }
        ).await.unwrap();
    }
    let write_duration = start.elapsed();
    let write_ops = operations as f64 / write_duration.as_secs_f64();
    
    println!("  Write throughput: {:.2} ops/sec", write_ops);
    println!("  Write latency: {:.3} ms/op", write_duration.as_millis() as f64 / operations as f64);
    
    println!("\n=== RATIO ===");
    println!("  Read is {:.1}x faster than write", read_ops / write_ops);
}
