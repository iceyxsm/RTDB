//! Quick benchmark for Optimized Direct Jepsen Client

use rtdb::jepsen::direct_client_optimized::{OptimizedDirectJepsenClient, OptimizedClientConfig};
use rtdb::jepsen::{JepsenClient, OperationType};
use std::time::Instant;

#[tokio::test]
async fn quick_benchmark() {
    println!("\n=== QUICK OPTIMIZED BENCHMARK ===\n");
    
    let operations = 1000;
    
    // Test optimized client (1-dimension, batched)
    println!("Testing Optimized Client (1-dim, batch=100)...");
    let config = OptimizedClientConfig {
        vector_dim: 1,
        batch_size: 100,
        flush_interval_ms: 10,
        enable_pooling: true,
        in_memory_only: true,
    };
    let client = OptimizedDirectJepsenClient::with_config(0, config).await.unwrap();
    
    let start = Instant::now();
    for i in 0..operations {
        client.execute(
            OperationType::Write { 
                key: format!("key_{}", i), 
                value: serde_json::json!(i) 
            }
        ).await.unwrap();
    }
    client.flush().await.unwrap();
    let duration = start.elapsed();
    let ops = operations as f64 / duration.as_secs_f64();
    
    println!("  Operations: {}", operations);
    println!("  Duration: {:?}", duration);
    println!("  Throughput: {:.2} ops/sec", ops);
    
    // Standard client for comparison (smaller test)
    println!("\nTesting Standard Client (128-dim, no batching)...");
    let std_client = rtdb::jepsen::direct_client::DirectJepsenClient::new(1, 128).await.unwrap();
    
    let start = Instant::now();
    for i in 0..200 { // Fewer ops for standard
        std_client.execute(
            OperationType::Write { 
                key: format!("std_key_{}", i), 
                value: serde_json::json!(i) 
            }
        ).await.unwrap();
    }
    let std_duration = start.elapsed();
    let std_ops = 200.0 / std_duration.as_secs_f64();
    
    println!("  Operations: 200");
    println!("  Duration: {:?}", std_duration);
    println!("  Throughput: {:.2} ops/sec", std_ops);
    
    println!("\n=== RESULT ===");
    println!("Optimized: {:.2} ops/sec", ops);
    println!("Standard:  {:.2} ops/sec", std_ops);
    println!("Speedup:   {:.1}x", ops / std_ops);
}
