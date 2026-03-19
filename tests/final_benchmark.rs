//! Final Performance Benchmark: All Client Types Compared

use rtdb::jepsen::high_perf_store::UltraFastJepsenClient;
use rtdb::jepsen::direct_client::DirectJepsenClient;
use rtdb::jepsen::{JepsenClient, OperationType};
use std::time::Instant;

#[tokio::test]
async fn final_comprehensive_benchmark() {
    println!("\n╔════════════════════════════════════════════════════════════╗");
    println!("║    FINAL PERFORMANCE BENCHMARK: ALL CLIENT TYPES          ║");
    println!("╚════════════════════════════════════════════════════════════╝\n");
    
    // Test with fewer operations for standard client (it's very slow)
    let ultra_ops = 10000;
    let standard_ops = 500;
    
    // 1. ULTRA-FAST CLIENT (In-Memory HashMap, No HNSW, No Disk)
    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│ 1. ULTRA-FAST CLIENT (DashMap, No HNSW, No Disk I/O)       │");
    println!("└─────────────────────────────────────────────────────────────┘");
    let ultra = UltraFastJepsenClient::new(0);
    
    let start = Instant::now();
    for i in 0..ultra_ops {
        ultra.write(format!("key_{}", i), serde_json::json!(i)).unwrap();
    }
    let ultra_write_duration = start.elapsed();
    let ultra_write_ops = ultra_ops as f64 / ultra_write_duration.as_secs_f64();
    
    let start = Instant::now();
    for i in 0..ultra_ops {
        let _ = ultra.read(format!("key_{}", i));
    }
    let ultra_read_duration = start.elapsed();
    let ultra_read_ops = ultra_ops as f64 / ultra_read_duration.as_secs_f64();
    
    println!("  Operations: {}", ultra_ops);
    println!("  Write: {:>10.2} ops/sec  ({:?})", ultra_write_ops, ultra_write_duration);
    println!("  Read:  {:>10.2} ops/sec  ({:?})", ultra_read_ops, ultra_read_duration);
    
    // 2. STANDARD DIRECT CLIENT (CollectionManager + HNSW + Disk)
    println!("\n┌─────────────────────────────────────────────────────────────┐");
    println!("│ 2. STANDARD DIRECT CLIENT (HNSW Index + Disk Persistence)  │");
    println!("└─────────────────────────────────────────────────────────────┘");
    let direct = DirectJepsenClient::new(0, 128).await.unwrap();
    
    let start = Instant::now();
    for i in 0..standard_ops {
        direct.execute(OperationType::Write { 
            key: format!("key_{}", i), 
            value: serde_json::json!(i) 
        }).await.unwrap();
    }
    let direct_write_duration = start.elapsed();
    let direct_write_ops = standard_ops as f64 / direct_write_duration.as_secs_f64();
    
    let start = Instant::now();
    for i in 0..standard_ops {
        direct.execute(OperationType::Read { 
            key: format!("key_{}", i) 
        }).await.unwrap();
    }
    let direct_read_duration = start.elapsed();
    let direct_read_ops = standard_ops as f64 / direct_read_duration.as_secs_f64();
    
    println!("  Operations: {}", standard_ops);
    println!("  Write: {:>10.2} ops/sec  ({:?})", direct_write_ops, direct_write_duration);
    println!("  Read:  {:>10.2} ops/sec  ({:?})", direct_read_ops, direct_read_duration);
    
    // SUMMARY TABLE (normalize to same scale)
    println!("\n╔═══════════════════════════════════════════════════════════════╗");
    println!("║                      SUMMARY TABLE                            ║");
    println!("╠═══════════════════════════════════════════════════════════════╣");
    println!("║                   │  Ultra-Fast  │  Standard  │   Speedup    ║");
    println!("╠═══════════════════╪══════════════╪════════════╪══════════════╣");
    println!("║ Write (ops/sec)   │ {:>12.0} │ {:>10.0} │ {:>11.0}x ║", 
        ultra_write_ops, direct_write_ops, ultra_write_ops / direct_write_ops);
    println!("║ Read (ops/sec)    │ {:>12.0} │ {:>10.0} │ {:>11.1}x ║", 
        ultra_read_ops, direct_read_ops, ultra_read_ops / direct_read_ops);
    println!("╚═══════════════════╧══════════════╧════════════╧══════════════╝");
    
    println!("\n BENCHMARK COMPLETE");
    println!("   Ultra-fast client: {:.0}x faster for writes", ultra_write_ops / direct_write_ops);
    println!("   Ultra-fast client: {:.1}x faster for reads", ultra_read_ops / direct_read_ops);
}

#[test]
fn ultra_fast_sync_benchmark() {
    println!("\n=== SYNC ULTRA-FAST BENCHMARK (100K ops) ===\n");
    
    let client = UltraFastJepsenClient::new(0);
    let operations = 100000;
    
    // Writes
    let start = Instant::now();
    for i in 0..operations {
        client.write(format!("key_{}", i), serde_json::json!(i)).unwrap();
    }
    let write_duration = start.elapsed();
    let write_ops = operations as f64 / write_duration.as_secs_f64();
    
    // Reads
    let start = Instant::now();
    for i in 0..operations {
        let _ = client.read(format!("key_{}", i));
    }
    let read_duration = start.elapsed();
    let read_ops = operations as f64 / read_duration.as_secs_f64();
    
    println!("Write: {:>10.2} ops/sec ({:?})", write_ops, write_duration);
    println!("Read:  {:>10.2} ops/sec ({:?})", read_ops, read_duration);
    
    assert!(write_ops > 100000.0, "Should achieve >100K writes/sec");
    assert!(read_ops > 100000.0, "Should achieve >100K reads/sec");
}
