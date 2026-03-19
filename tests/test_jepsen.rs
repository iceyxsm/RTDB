use rtdb::jepsen::{JepsenConfig, JepsenTestExecutor, ConsistencyModel};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("rtdb=info,warn")
        .init();

    println!("Starting Jepsen test against RTDB...");
    
    // Configure a shorter test for quick validation
    let config = JepsenConfig {
        client_count: 4,
        test_duration_secs: 30, // 30 seconds
        operation_rate: 50, // 50 ops/sec per client = 200 total ops/sec
        partition_probability: 0.1,
        enable_simdx: true,
        consistency_model: ConsistencyModel::Linearizable,
        max_operation_latency_ms: 5000,
    };

    println!("Test Configuration:");
    println!("  - Clients: {}", config.client_count);
    println!("  - Duration: {}s", config.test_duration_secs);
    println!("  - Rate: {} ops/sec per client", config.operation_rate);
    println!("  - SIMDX: {}", config.enable_simdx);
    println!("  - Expected total ops: ~{}", config.client_count * (config.operation_rate as usize) * (config.test_duration_secs as usize));

    // Test against localhost (RTDB server is running on port 8333)
    let cluster_nodes = vec![
        "localhost:8333".to_string(),
    ];

    let mut executor = JepsenTestExecutor::new(config);
    
    match executor.execute_test_suite(cluster_nodes).await {
        Ok(result) => {
            println!("\n{}", result.generate_report());
            
            // Validate performance expectations
            if result.throughput_ops_per_sec > 100.0 {
                println!("✅ PERFORMANCE VALIDATION PASSED: {:.2} ops/sec > 100 ops/sec", result.throughput_ops_per_sec);
            } else {
                println!("❌ PERFORMANCE VALIDATION FAILED: {:.2} ops/sec < 100 ops/sec", result.throughput_ops_per_sec);
            }
            
            if result.linearizability_result.is_linearizable {
                println!("✅ CONSISTENCY VALIDATION PASSED: No linearizability violations");
            } else {
                println!("❌ CONSISTENCY VALIDATION FAILED: {} violations found", 
                        result.linearizability_result.violations.len());
            }
        }
        Err(e) => {
            eprintln!("❌ Jepsen test failed: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}