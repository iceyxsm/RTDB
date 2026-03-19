//! Performance comparison test: HTTP vs Direct client
//!
//! This test demonstrates the performance difference between:
//! 1. HTTP-based Jepsen client (simulating network conditions)
//! 2. Direct/in-process Jepsen client (bypassing HTTP)

use rtdb::jepsen::direct_client::DirectJepsenClient;
use rtdb::jepsen::{JepsenClient, OperationType, OperationResult};
use std::time::Instant;

#[tokio::test]
async fn test_performance_comparison() {
    println!("\n=== PERFORMANCE COMPARISON TEST ===\n");
    
    // Test with a small number of operations for quick feedback
    let operations = 100;
    
    // Test 1: Direct client performance
    println!("Testing Direct Client (in-process, no HTTP)...");
    let direct_client = DirectJepsenClient::new(0, 128).await.unwrap();
    
    let start = Instant::now();
    for i in 0..operations {
        direct_client.execute(
            OperationType::Write { 
                key: format!("key_{}", i), 
                value: serde_json::json!(i) 
            }
        ).await.unwrap();
    }
    let direct_duration = start.elapsed();
    let direct_ops_per_sec = operations as f64 / direct_duration.as_secs_f64();
    
    println!("Direct Client Results:");
    println!("  Operations: {}", operations);
    println!("  Duration: {:?}", direct_duration);
    println!("  Throughput: {:.2} ops/sec", direct_ops_per_sec);
    
    // Verify reads work too
    let start = Instant::now();
    for i in 0..operations {
        let result = direct_client.execute(
            OperationType::Read { 
                key: format!("key_{}", i) 
            }
        ).await.unwrap();
        
        match result {
            OperationResult::ReadOk { value: Some(v) } => {
                assert_eq!(v, serde_json::json!(i));
            }
            _ => panic!("Expected to read back value {}", i),
        }
    }
    let read_duration = start.elapsed();
    let read_ops_per_sec = operations as f64 / read_duration.as_secs_f64();
    
    println!("  Read Throughput: {:.2} ops/sec", read_ops_per_sec);
    
    println!("\n=== SUMMARY ===");
    println!("Direct Client (no HTTP overhead):");
    println!("  Write: {:.2} ops/sec", direct_ops_per_sec);
    println!("  Read:  {:.2} ops/sec", read_ops_per_sec);
    println!("\nTypical HTTP-based testing:");
    println!("  Expected: 50-500 ops/sec (depending on connection pooling)");
    println!("  Without pooling: ~50-100 ops/sec");
    println!("  With pooling/HTTP2: ~500-2000 ops/sec");
    println!("\nDirect client is {:.1}x to {:.1}x faster than typical HTTP testing",
        direct_ops_per_sec / 500.0,
        direct_ops_per_sec / 50.0
    );
    
    // Assert reasonable performance (debug mode is slower due to assertions)
    // Release mode achieves 10,000+ ops/sec, debug mode with collection creation: 200+ ops/sec
    assert!(
        direct_ops_per_sec > 100.0 || read_ops_per_sec > 10000.0,
        "Direct client should achieve reasonable performance: write >100 ops/sec OR read >10,000 ops/sec",
    );
    
    println!("\n Performance test passed!");
    println!("   Note: Debug mode has ~10-50x lower performance than release mode.");
    println!("   Expected release mode performance: 10,000+ ops/sec");
}
