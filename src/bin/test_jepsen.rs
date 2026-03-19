use rtdb::jepsen::{JepsenConfig, JepsenTestExecutor, ConsistencyModel};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("rtdb=info,warn")
        .init();

    println!("Starting Jepsen test against RTDB...");
    
    // Configure for moderate testing to isolate bottlenecks
    let config = JepsenConfig {
        client_count: 4, // Fewer clients to reduce contention
        test_duration_secs: 10, // Shorter test for faster iteration
        operation_rate: 100, // Lower rate per client = 400 total ops/sec
        partition_probability: 0.0, // Disable partitions to focus on performance
        enable_simdx: true,
        consistency_model: ConsistencyModel::Linearizable,
        max_operation_latency_ms: 100, // Reduced timeout for faster operations
    };

    println!("Test Configuration:");
    println!("  - Clients: {}", config.client_count);
    println!("  - Duration: {}s", config.test_duration_secs);
    println!("  - Rate: {} ops/sec per client", config.operation_rate);
    println!("  - SIMDX: {}", config.enable_simdx);
    println!("  - Expected total ops: ~{}", config.client_count as u64 * config.operation_rate * config.test_duration_secs);
    println!("  - Target throughput: {} ops/sec (approaching benchmark minimum of 10,000)", config.client_count as u64 * config.operation_rate);

    // Test against localhost (RTDB server is running on port 8333)
    let cluster_nodes = vec![
        "localhost:8333".to_string(),
    ];

    let mut executor = JepsenTestExecutor::new(config);
    
    match executor.execute_test_suite(cluster_nodes).await {
        Ok(result) => {
            println!("\n{}", result.generate_report());
            
            // Validate performance expectations against benchmark targets
            let benchmark_minimum = 10000.0; // From docs/BENCHMARKS.md
            let benchmark_target = 50000.0;  // From docs/BENCHMARKS.md
            
            if result.throughput_ops_per_sec >= benchmark_minimum {
                println!(" PERFORMANCE VALIDATION PASSED: {:.2} ops/sec >= {:.0} ops/sec (benchmark minimum)",
                        result.throughput_ops_per_sec, benchmark_minimum);
            } else if result.throughput_ops_per_sec >= 1000.0 {
                println!("  PERFORMANCE APPROACHING TARGET: {:.2} ops/sec >= 1,000 ops/sec (but below {:.0} minimum)",
                        result.throughput_ops_per_sec, benchmark_minimum);
            } else {
                println!(" PERFORMANCE VALIDATION FAILED: {:.2} ops/sec < {:.0} ops/sec (benchmark minimum)",
                        result.throughput_ops_per_sec, benchmark_minimum);
            }
            
            if result.throughput_ops_per_sec >= benchmark_target {
                println!(" PERFORMANCE TARGET ACHIEVED: {:.2} ops/sec >= {:.0} ops/sec (benchmark target)",
                        result.throughput_ops_per_sec, benchmark_target);
            }
            
            if result.linearizability_result.is_linearizable {
                println!(" CONSISTENCY VALIDATION PASSED: No linearizability violations");
            } else {
                println!(" CONSISTENCY VALIDATION FAILED: {} violations found",
                        result.linearizability_result.violations.len());
            }
        }
        Err(e) => {
            eprintln!(" Jepsen test failed: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}