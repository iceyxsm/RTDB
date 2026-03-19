use rtdb::jepsen::{JepsenConfig, JepsenTestExecutor, ConsistencyModel};
use std::time::Instant;

/// **Validates: Requirements 1.1, 1.2, 1.3, 1.4, 1.5**
/// 
/// Property 1: Bug Condition - Jepsen Performance and Consistency Violations
/// 
/// This test MUST FAIL on unfixed code - failure confirms the bug exists.
/// The test encodes the expected behavior and will validate the fix when it passes after implementation.
/// 
/// CRITICAL: This test is designed to surface counterexamples that demonstrate:
/// - Severe performance degradation (<20 ops/sec vs target >10,000 ops/sec)
/// - ReadAfterWrite consistency violations 
/// - Connection overhead >50% of operation time
/// - Search-based reads 50-100x slower than direct lookup
/// 
/// EXPECTED OUTCOME: Test FAILS (this is correct - it proves the bug exists)
#[cfg(test)]
mod jepsen_bug_condition_tests {
    use super::*;

    /// Property test that validates Jepsen operations achieve expected performance and consistency
    /// 
    /// This test encodes the EXPECTED behavior from the design document:
    /// - >10,000 ops/sec throughput (target: 50,000 ops/sec)
    /// - <100µs average latency (target: <10µs)
    /// - Zero ReadAfterWrite violations
    /// 
    /// When run on UNFIXED code, this test MUST FAIL to confirm the bug exists.
    /// When run on FIXED code, this test MUST PASS to confirm the fix works.
    #[tokio::test]
    async fn property_jepsen_performance_and_consistency_violations() {
        // Test cluster setup - single node for deterministic testing
        let cluster_nodes = vec!["localhost:8333".to_string()];
        
        // Use the same configuration as the working test to ensure reproducibility
        let config = JepsenConfig {
            client_count: 4,
            test_duration_secs: 10,
            operation_rate: 100, // 400 total ops/sec target
            partition_probability: 0.0, // No partitions for performance testing
            enable_simdx: true,
            consistency_model: ConsistencyModel::Linearizable,
            max_operation_latency_ms: 5000,
        };
        
        let mut executor = JepsenTestExecutor::new(config.clone());
        
        // Measure total test execution time for throughput calculation
        let start_time = Instant::now();
        
        let result = executor.execute_test_suite(cluster_nodes).await
            .expect("Jepsen test execution should not fail");
        
        let total_duration = start_time.elapsed();
        
        // CRITICAL ASSERTIONS: These encode the EXPECTED behavior
        // When these assertions fail on unfixed code, it proves the bug exists
        
        // Performance Requirement 2.1: >10,000 ops/sec throughput (target: 50,000 ops/sec)
        assert!(
            result.throughput_ops_per_sec > 10000.0,
            "PERFORMANCE BUG DETECTED: Throughput {:.2} ops/sec < 10,000 ops/sec target. \
             Expected >10,000 ops/sec (target: 50,000 ops/sec). \
             This indicates inefficient API usage patterns. \
             Actual results: {} operations in {:.2}s = {:.2} ops/sec",
            result.throughput_ops_per_sec,
            result.total_operations,
            total_duration.as_secs_f64(),
            result.total_operations as f64 / total_duration.as_secs_f64()
        );
        
        // Performance Requirement 2.3: <100µs average latency (target: <10µs)
        let avg_latency_us = (result.test_duration.as_secs_f64() * 1000.0) / (result.total_operations as f64) * 1000.0; // Convert to microseconds
        assert!(
            avg_latency_us < 100.0,
            "LATENCY BUG DETECTED: Average latency {:.2}µs > 100µs target. \
             Expected <100µs (target: <10µs based on HNSW benchmarks). \
             This indicates connection overhead and inefficient operations.",
            avg_latency_us
        );
        
        // Consistency Requirement 2.2: Zero ReadAfterWrite violations
        assert!(
            result.linearizability_result.violations.is_empty(),
            "CONSISTENCY BUG DETECTED: {} ReadAfterWrite violations found. \
             Expected zero violations. This indicates write-read synchronization issues. \
             Violations: {:?}",
            result.linearizability_result.violations.len(),
            result.linearizability_result.violations.iter()
                .map(|v| format!("{:?}", v.violation_type))
                .collect::<Vec<_>>()
        );
        
        // Additional performance validation: Connection efficiency
        // If operations are slow, it indicates connection overhead issues
        let expected_min_ops = config.client_count as f64 * config.operation_rate as f64 * config.test_duration_secs as f64 * 0.8; // 80% of target
        assert!(
            result.total_operations as f64 >= expected_min_ops,
            "OPERATION COUNT BUG DETECTED: Only {} operations completed, expected at least {:.0}. \
             This indicates severe performance bottlenecks preventing operation completion.",
            result.total_operations,
            expected_min_ops
        );
        
        println!(" EXPECTED BEHAVIOR VALIDATED:");
        println!("  - Throughput: {:.2} ops/sec (target: >10,000)", result.throughput_ops_per_sec);
        println!("  - Latency: {:.2}µs (target: <100µs)", avg_latency_us);
        println!("  - Consistency: {} violations (target: 0)", result.linearizability_result.violations.len());
        println!("  - Operations: {} completed", result.total_operations);
    }

    /// Focused test for ReadAfterWrite consistency violations
    /// 
    /// This test specifically targets the consistency bug condition where writes
    /// are not immediately available for subsequent reads.
    #[tokio::test]
    async fn property_read_after_write_consistency_violations() {
        // Use minimal configuration for focused testing
        let config = JepsenConfig {
            client_count: 2,
            test_duration_secs: 15,
            operation_rate: 20,
            partition_probability: 0.0,
            enable_simdx: true,
            consistency_model: ConsistencyModel::Linearizable,
            max_operation_latency_ms: 1000,
        };
        
        let cluster_nodes = vec!["localhost:8333".to_string()];
        let mut executor = JepsenTestExecutor::new(config);
        
        let result = executor.execute_test_suite(cluster_nodes).await
            .expect("ReadAfterWrite test should not fail");
        
        // CRITICAL: This assertion encodes the expected behavior
        // On unfixed code, this MUST FAIL due to write-read synchronization issues
        assert_eq!(
            result.linearizability_result.violations.len(), 0,
            "CONSISTENCY BUG CONFIRMED: {} ReadAfterWrite violations detected. \
             Expected behavior: zero violations. \
             This proves the bug exists in write-read synchronization. \
             Violations: {:?}",
            result.linearizability_result.violations.len(),
            result.linearizability_result.violations.iter()
                .map(|v| format!("{:?}: {}", v.violation_type, v.description))
                .collect::<Vec<_>>()
        );
        
        println!(" ReadAfterWrite consistency validated: 0 violations");
    }

    /// Focused test for throughput performance degradation
    /// 
    /// This test specifically targets the performance bug condition where
    /// operations achieve <20 ops/sec instead of expected >10,000 ops/sec.
    #[tokio::test]
    async fn property_throughput_performance_degradation() {
        // Configuration optimized for throughput measurement
        let config = JepsenConfig {
            client_count: 4,
            test_duration_secs: 20,
            operation_rate: 50, // 200 total ops/sec target
            partition_probability: 0.0,
            enable_simdx: true,
            consistency_model: ConsistencyModel::Linearizable,
            max_operation_latency_ms: 2000,
        };
        
        let cluster_nodes = vec!["localhost:8333".to_string()];
        let mut executor = JepsenTestExecutor::new(config);
        
        let start_time = Instant::now();
        let result = executor.execute_test_suite(cluster_nodes).await
            .expect("Throughput test should not fail");
        let total_time = start_time.elapsed();
        
        // CRITICAL: This assertion encodes the expected behavior
        // On unfixed code, this MUST FAIL due to inefficient API patterns
        assert!(
            result.throughput_ops_per_sec > 10000.0,
            "THROUGHPUT BUG CONFIRMED: Achieved {:.2} ops/sec < 10,000 ops/sec target. \
             Expected >10,000 ops/sec (target: 50,000 ops/sec based on benchmarks). \
             Total time: {:.2}s, Operations: {}, Average latency: {:.2}ms. \
             This proves inefficient API usage patterns exist.",
            result.throughput_ops_per_sec,
            total_time.as_secs_f64(),
            result.total_operations,
            (result.test_duration.as_secs_f64() * 1000.0) / (result.total_operations as f64)
        );
        
        println!(" Throughput performance validated: {:.2} ops/sec", result.throughput_ops_per_sec);
    }

    /// Test for latency performance degradation
    /// 
    /// This test validates that individual operations complete within microsecond timeframes
    /// as expected based on the HNSW benchmark performance (8.5µs).
    #[tokio::test]
    async fn property_latency_performance_degradation() {
        let config = JepsenConfig {
            client_count: 1, // Single client for precise latency measurement
            test_duration_secs: 10,
            operation_rate: 10,
            partition_probability: 0.0,
            enable_simdx: true,
            consistency_model: ConsistencyModel::Linearizable,
            max_operation_latency_ms: 1000,
        };
        
        let cluster_nodes = vec!["localhost:8333".to_string()];
        let mut executor = JepsenTestExecutor::new(config);
        
        let result = executor.execute_test_suite(cluster_nodes).await
            .expect("Latency test should not fail");
        
        let avg_latency_us = (result.test_duration.as_secs_f64() * 1000.0) / (result.total_operations as f64) * 1000.0;
        
        // CRITICAL: This assertion encodes the expected behavior
        // On unfixed code, this MUST FAIL due to connection overhead and inefficient operations
        assert!(
            avg_latency_us < 100.0,
            "LATENCY BUG CONFIRMED: Average latency {:.2}µs > 100µs target. \
             Expected <100µs (target: <10µs based on HNSW 8.5µs benchmarks). \
             This proves connection overhead and API inefficiency issues exist.",
            avg_latency_us
        );
        
        println!(" Latency performance validated: {:.2}µs average", avg_latency_us);
    }
}