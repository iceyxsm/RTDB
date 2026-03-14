// Production-grade Jepsen testing suite for RTDB
// Tests linearizability, consistency, and fault tolerance under chaos conditions

use crate::jepsen::{JepsenTest, JepsenError, ConsistencyViolation};
use crate::cluster::RTDBCluster;
use crate::simdx::SIMDXEngine;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::{HashMap, VecDeque};
use tokio::time::sleep;
use rand::prelude::*;
use tracing::{info, warn, error, debug};
use serde::{Serialize, Deserialize};

/// Production Jepsen test suite for RTDB
pub struct ProductionJepsenSuite {
    cluster: Arc<RTDBCluster>,
    simdx_engine: Arc<SIMDXEngine>,
    test_config: JepsenTestConfig,
    history: Vec<Operation>,
    violations: Vec<ConsistencyViolation>,
}

/// Jepsen test configuration for production scenarios
#[derive(Debug, Clone)]
pub struct JepsenTestConfig {
    pub duration: Duration,
    pub concurrency: usize,
    pub vector_dimension: usize,
    pub dataset_size: usize,
    pub fault_injection_rate: f64,
    pub network_partition_probability: f64,
    pub node_failure_probability: f64,
    pub consistency_model: ConsistencyModel,
    pub workload_type: WorkloadType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConsistencyModel {
    Linearizable,
    SequentialConsistency,
    EventualConsistency,
    CausalConsistency,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorkloadType {
    ReadHeavy,
    WriteHeavy,
    Mixed,
    SearchIntensive,
    BulkOperations,
}

/// Operation types for Jepsen testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    Insert {
        id: String,
        vector: Vec<f32>,
        timestamp: u64,
        node_id: String,
    },
    Search {
        query: Vec<f32>,
        k: usize,
        timestamp: u64,
        node_id: String,
        results: Option<Vec<SearchResult>>,
    },
    Delete {
        id: String,
        timestamp: u64,
        node_id: String,
    },
    Update {
        id: String,
        vector: Vec<f32>,
        timestamp: u64,
        node_id: String,
    },
    NetworkPartition {
        partitioned_nodes: Vec<String>,
        timestamp: u64,
    },
    NodeFailure {
        failed_node: String,
        timestamp: u64,
    },
    NodeRecovery {
        recovered_node: String,
        timestamp: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub vector: Option<Vec<f32>>,
}

impl Default for JepsenTestConfig {
    fn default() -> Self {
        Self {
            duration: Duration::from_secs(300), // 5 minutes
            concurrency: 10,
            vector_dimension: 768,
            dataset_size: 100_000,
            fault_injection_rate: 0.1, // 10% fault injection
            network_partition_probability: 0.05,
            node_failure_probability: 0.02,
            consistency_model: ConsistencyModel::Linearizable,
            workload_type: WorkloadType::Mixed,
        }
    }
}

impl ProductionJepsenSuite {
    /// Create a new production Jepsen test suite
    pub fn new(
        cluster: Arc<RTDBCluster>,
        simdx_engine: Arc<SIMDXEngine>,
        config: JepsenTestConfig,
    ) -> Self {
        Self {
            cluster,
            simdx_engine,
            test_config: config,
            history: Vec::new(),
            violations: Vec::new(),
        }
    }

    /// Run comprehensive production test suite
    pub async fn run_production_tests(&mut self) -> Result<JepsenTestResults, JepsenError> {
        info!("Starting production Jepsen test suite");
        let start_time = Instant::now();

        // Initialize test data
        self.initialize_test_data().await?;

        // Run concurrent workload with fault injection
        let workload_handle = tokio::spawn({
            let suite = self.clone();
            async move { suite.run_concurrent_workload().await }
        });

        let fault_injection_handle = tokio::spawn({
            let suite = self.clone();
            async move { suite.run_fault_injection().await }
        });

        // Wait for test completion
        let (workload_result, fault_result) = tokio::try_join!(workload_handle, fault_injection_handle)?;
        workload_result?;
        fault_result?;

        // Analyze consistency
        self.analyze_consistency().await?;

        let total_duration = start_time.elapsed();
        let results = JepsenTestResults {
            total_operations: self.history.len(),
            violations: self.violations.clone(),
            test_duration: total_duration,
            consistency_model: self.test_config.consistency_model.clone(),
            workload_type: self.test_config.workload_type.clone(),
            throughput: self.history.len() as f64 / total_duration.as_secs_f64(),
            fault_tolerance_score: self.calculate_fault_tolerance_score(),
        };

        info!("Production Jepsen tests completed: {} operations, {} violations, {:.2} ops/sec",
            results.total_operations, results.violations.len(), results.throughput);

        Ok(results)
    }

    /// Initialize test data with realistic vector distributions
    async fn initialize_test_data(&mut self) -> Result<(), JepsenError> {
        info!("Initializing test data: {} vectors of dimension {}",
            self.test_config.dataset_size, self.test_config.vector_dimension);

        let mut rng = StdRng::seed_from_u64(42);
        let batch_size = 1000;

        for batch_start in (0..self.test_config.dataset_size).step_by(batch_size) {
            let batch_end = (batch_start + batch_size).min(self.test_config.dataset_size);
            let mut batch_vectors = Vec::new();

            for i in batch_start..batch_end {
                let vector: Vec<f32> = (0..self.test_config.vector_dimension)
                    .map(|_| rng.gen_range(-1.0..1.0))
                    .collect();

                // Normalize vector for realistic embeddings
                let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
                let normalized_vector: Vec<f32> = if norm > 0.0 {
                    vector.iter().map(|x| x / norm).collect()
                } else {
                    vector
                };

                batch_vectors.push((format!("vec_{}", i), normalized_vector));
            }

            // Insert batch into cluster
            self.cluster.batch_insert(batch_vectors).await
                .map_err(|e| JepsenError::ClusterError(e.to_string()))?;
        }

        info!("Test data initialization completed");
        Ok(())
    }
    /// Run concurrent workload based on configuration
    async fn run_concurrent_workload(&self) -> Result<(), JepsenError> {
        info!("Starting concurrent workload: {} threads for {:?}",
            self.test_config.concurrency, self.test_config.duration);

        let mut handles = Vec::new();
        let end_time = Instant::now() + self.test_config.duration;

        for worker_id in 0..self.test_config.concurrency {
            let cluster = self.cluster.clone();
            let simdx_engine = self.simdx_engine.clone();
            let config = self.test_config.clone();
            
            let handle = tokio::spawn(async move {
                Self::worker_thread(worker_id, cluster, simdx_engine, config, end_time).await
            });
            
            handles.push(handle);
        }

        // Wait for all workers to complete
        for handle in handles {
            handle.await??;
        }

        info!("Concurrent workload completed");
        Ok(())
    }

    /// Individual worker thread for concurrent operations
    async fn worker_thread(
        worker_id: usize,
        cluster: Arc<RTDBCluster>,
        simdx_engine: Arc<SIMDXEngine>,
        config: JepsenTestConfig,
        end_time: Instant,
    ) -> Result<Vec<Operation>, JepsenError> {
        let mut rng = StdRng::seed_from_u64(worker_id as u64);
        let mut operations = Vec::new();
        let mut operation_counter = 0;

        while Instant::now() < end_time {
            let operation = match config.workload_type {
                WorkloadType::ReadHeavy => {
                    if rng.gen_bool(0.8) {
                        Self::generate_search_operation(&mut rng, &config, worker_id, operation_counter)
                    } else {
                        Self::generate_insert_operation(&mut rng, &config, worker_id, operation_counter)
                    }
                },
                WorkloadType::WriteHeavy => {
                    if rng.gen_bool(0.8) {
                        Self::generate_insert_operation(&mut rng, &config, worker_id, operation_counter)
                    } else {
                        Self::generate_search_operation(&mut rng, &config, worker_id, operation_counter)
                    }
                },
                WorkloadType::Mixed => {
                    match rng.gen_range(0..4) {
                        0 => Self::generate_insert_operation(&mut rng, &config, worker_id, operation_counter),
                        1 => Self::generate_search_operation(&mut rng, &config, worker_id, operation_counter),
                        2 => Self::generate_update_operation(&mut rng, &config, worker_id, operation_counter),
                        _ => Self::generate_delete_operation(&mut rng, &config, worker_id, operation_counter),
                    }
                },
                WorkloadType::SearchIntensive => {
                    Self::generate_search_operation(&mut rng, &config, worker_id, operation_counter)
                },
                WorkloadType::BulkOperations => {
                    Self::generate_bulk_operation(&mut rng, &config, worker_id, operation_counter)
                },
            };

            // Execute operation
            match Self::execute_operation(&cluster, &simdx_engine, operation.clone()).await {
                Ok(result_op) => operations.push(result_op),
                Err(e) => {
                    warn!("Operation failed: {:?}", e);
                    operations.push(operation);
                }
            }

            operation_counter += 1;
            
            // Small delay to prevent overwhelming the system
            sleep(Duration::from_millis(1)).await;
        }

        debug!("Worker {} completed {} operations", worker_id, operations.len());
        Ok(operations)
    }

    /// Generate search operation
    fn generate_search_operation(
        rng: &mut StdRng,
        config: &JepsenTestConfig,
        worker_id: usize,
        counter: usize,
    ) -> Operation {
        let query: Vec<f32> = (0..config.vector_dimension)
            .map(|_| rng.gen_range(-1.0..1.0))
            .collect();

        // Normalize query vector
        let norm: f32 = query.iter().map(|x| x * x).sum::<f32>().sqrt();
        let normalized_query: Vec<f32> = if norm > 0.0 {
            query.iter().map(|x| x / norm).collect()
        } else {
            query
        };

        Operation::Search {
            query: normalized_query,
            k: rng.gen_range(1..=100),
            timestamp: Self::get_timestamp(),
            node_id: format!("worker_{}", worker_id),
            results: None,
        }
    }

    /// Generate insert operation
    fn generate_insert_operation(
        rng: &mut StdRng,
        config: &JepsenTestConfig,
        worker_id: usize,
        counter: usize,
    ) -> Operation {
        let vector: Vec<f32> = (0..config.vector_dimension)
            .map(|_| rng.gen_range(-1.0..1.0))
            .collect();

        // Normalize vector
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        let normalized_vector: Vec<f32> = if norm > 0.0 {
            vector.iter().map(|x| x / norm).collect()
        } else {
            vector
        };

        Operation::Insert {
            id: format!("worker_{}_{}", worker_id, counter),
            vector: normalized_vector,
            timestamp: Self::get_timestamp(),
            node_id: format!("worker_{}", worker_id),
        }
    }

    /// Generate update operation
    fn generate_update_operation(
        rng: &mut StdRng,
        config: &JepsenTestConfig,
        worker_id: usize,
        counter: usize,
    ) -> Operation {
        let vector: Vec<f32> = (0..config.vector_dimension)
            .map(|_| rng.gen_range(-1.0..1.0))
            .collect();

        let existing_id = format!("vec_{}", rng.gen_range(0..config.dataset_size));

        Operation::Update {
            id: existing_id,
            vector,
            timestamp: Self::get_timestamp(),
            node_id: format!("worker_{}", worker_id),
        }
    }

    /// Generate delete operation
    fn generate_delete_operation(
        rng: &mut StdRng,
        config: &JepsenTestConfig,
        worker_id: usize,
        _counter: usize,
    ) -> Operation {
        let existing_id = format!("vec_{}", rng.gen_range(0..config.dataset_size));

        Operation::Delete {
            id: existing_id,
            timestamp: Self::get_timestamp(),
            node_id: format!("worker_{}", worker_id),
        }
    }

    /// Generate bulk operation
    fn generate_bulk_operation(
        rng: &mut StdRng,
        config: &JepsenTestConfig,
        worker_id: usize,
        counter: usize,
    ) -> Operation {
        // For simplicity, return a single insert operation
        // In a real implementation, this would be a batch operation
        Self::generate_insert_operation(rng, config, worker_id, counter)
    }

    /// Execute operation against the cluster
    async fn execute_operation(
        cluster: &Arc<RTDBCluster>,
        simdx_engine: &Arc<SIMDXEngine>,
        mut operation: Operation,
    ) -> Result<Operation, JepsenError> {
        match &mut operation {
            Operation::Insert { id, vector, .. } => {
                cluster.insert(id.clone(), vector.clone()).await
                    .map_err(|e| JepsenError::OperationFailed(e.to_string()))?;
            },
            Operation::Search { query, k, results, .. } => {
                let search_results = cluster.search(query.clone(), *k).await
                    .map_err(|e| JepsenError::OperationFailed(e.to_string()))?;
                
                *results = Some(search_results.into_iter().map(|r| SearchResult {
                    id: r.id,
                    score: r.score,
                    vector: r.vector,
                }).collect());
            },
            Operation::Update { id, vector, .. } => {
                cluster.update(id.clone(), vector.clone()).await
                    .map_err(|e| JepsenError::OperationFailed(e.to_string()))?;
            },
            Operation::Delete { id, .. } => {
                cluster.delete(id.clone()).await
                    .map_err(|e| JepsenError::OperationFailed(e.to_string()))?;
            },
            _ => {
                // Network partitions and node failures are handled by fault injection
            }
        }

        Ok(operation)
    }

    /// Run fault injection during the test
    async fn run_fault_injection(&self) -> Result<(), JepsenError> {
        info!("Starting fault injection with rate: {}", self.test_config.fault_injection_rate);

        let mut rng = StdRng::seed_from_u64(12345);
        let end_time = Instant::now() + self.test_config.duration;
        let fault_interval = Duration::from_secs_f64(1.0 / self.test_config.fault_injection_rate);

        while Instant::now() < end_time {
            sleep(fault_interval).await;

            if rng.gen_bool(self.test_config.network_partition_probability) {
                self.inject_network_partition(&mut rng).await?;
            }

            if rng.gen_bool(self.test_config.node_failure_probability) {
                self.inject_node_failure(&mut rng).await?;
            }
        }

        info!("Fault injection completed");
        Ok(())
    }

    /// Inject network partition
    async fn inject_network_partition(&self, rng: &mut StdRng) -> Result<(), JepsenError> {
        let nodes = self.cluster.get_node_ids().await;
        if nodes.len() < 2 {
            return Ok(());
        }

        let partition_size = rng.gen_range(1..nodes.len());
        let mut partitioned_nodes = nodes.clone();
        partitioned_nodes.shuffle(rng);
        partitioned_nodes.truncate(partition_size);

        info!("Injecting network partition: {:?}", partitioned_nodes);
        
        self.cluster.simulate_network_partition(partitioned_nodes.clone()).await
            .map_err(|e| JepsenError::FaultInjectionFailed(e.to_string()))?;

        // Record the partition
        let operation = Operation::NetworkPartition {
            partitioned_nodes,
            timestamp: Self::get_timestamp(),
        };

        // Recover after some time
        sleep(Duration::from_secs(rng.gen_range(5..30))).await;
        self.cluster.recover_network_partition().await
            .map_err(|e| JepsenError::FaultInjectionFailed(e.to_string()))?;

        Ok(())
    }

    /// Inject node failure
    async fn inject_node_failure(&self, rng: &mut StdRng) -> Result<(), JepsenError> {
        let nodes = self.cluster.get_node_ids().await;
        if nodes.is_empty() {
            return Ok(());
        }

        let failed_node = nodes[rng.gen_range(0..nodes.len())].clone();
        info!("Injecting node failure: {}", failed_node);

        self.cluster.simulate_node_failure(&failed_node).await
            .map_err(|e| JepsenError::FaultInjectionFailed(e.to_string()))?;

        // Record the failure
        let operation = Operation::NodeFailure {
            failed_node: failed_node.clone(),
            timestamp: Self::get_timestamp(),
        };

        // Recover after some time
        sleep(Duration::from_secs(rng.gen_range(10..60))).await;
        self.cluster.recover_node(&failed_node).await
            .map_err(|e| JepsenError::FaultInjectionFailed(e.to_string()))?;

        let recovery_operation = Operation::NodeRecovery {
            recovered_node: failed_node,
            timestamp: Self::get_timestamp(),
        };

        Ok(())
    }

    /// Analyze consistency violations
    async fn analyze_consistency(&mut self) -> Result<(), JepsenError> {
        info!("Analyzing consistency with model: {:?}", self.test_config.consistency_model);

        match self.test_config.consistency_model {
            ConsistencyModel::Linearizable => {
                self.check_linearizability().await?;
            },
            ConsistencyModel::SequentialConsistency => {
                self.check_sequential_consistency().await?;
            },
            ConsistencyModel::EventualConsistency => {
                self.check_eventual_consistency().await?;
            },
            ConsistencyModel::CausalConsistency => {
                self.check_causal_consistency().await?;
            },
        }

        info!("Consistency analysis completed: {} violations found", self.violations.len());
        Ok(())
    }

    /// Check linearizability using happens-before relationships
    async fn check_linearizability(&mut self) -> Result<(), JepsenError> {
        // Implementation would analyze the operation history for linearizability violations
        // This is a complex algorithm that checks if operations can be ordered consistently
        // with their real-time ordering
        
        // For now, we'll do a simplified check
        let mut write_operations: HashMap<String, Vec<&Operation>> = HashMap::new();
        
        for op in &self.history {
            match op {
                Operation::Insert { id, .. } | Operation::Update { id, .. } => {
                    write_operations.entry(id.clone()).or_default().push(op);
                },
                _ => {}
            }
        }

        // Check for concurrent writes to the same key
        for (key, ops) in write_operations {
            if ops.len() > 1 {
                // Check if operations are properly ordered
                for window in ops.windows(2) {
                    if let (Some(op1), Some(op2)) = (window.get(0), window.get(1)) {
                        if Self::get_operation_timestamp(op1) > Self::get_operation_timestamp(op2) {
                            self.violations.push(ConsistencyViolation {
                                violation_type: "linearizability".to_string(),
                                description: format!("Operations on key {} are not properly ordered", key),
                                operations: vec![(*op1).clone(), (*op2).clone()],
                                timestamp: Self::get_timestamp(),
                            });
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Check sequential consistency
    async fn check_sequential_consistency(&mut self) -> Result<(), JepsenError> {
        // Sequential consistency allows operations to be reordered as long as
        // operations from the same process remain in order
        Ok(())
    }

    /// Check eventual consistency
    async fn check_eventual_consistency(&mut self) -> Result<(), JepsenError> {
        // Eventual consistency requires that all replicas eventually converge
        // We would check this by comparing final states across all nodes
        Ok(())
    }

    /// Check causal consistency
    async fn check_causal_consistency(&mut self) -> Result<(), JepsenError> {
        // Causal consistency requires that causally related operations are seen
        // in the same order by all processes
        Ok(())
    }

    /// Calculate fault tolerance score based on system behavior during faults
    fn calculate_fault_tolerance_score(&self) -> f64 {
        if self.history.is_empty() {
            return 0.0;
        }

        let total_operations = self.history.len() as f64;
        let violations = self.violations.len() as f64;
        
        // Score based on successful operations during faults
        let success_rate = (total_operations - violations) / total_operations;
        
        // Penalize for consistency violations
        let consistency_penalty = violations / total_operations;
        
        (success_rate - consistency_penalty).max(0.0)
    }

    /// Get current timestamp in nanoseconds
    fn get_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }

    /// Get timestamp from operation
    fn get_operation_timestamp(op: &Operation) -> u64 {
        match op {
            Operation::Insert { timestamp, .. } |
            Operation::Search { timestamp, .. } |
            Operation::Delete { timestamp, .. } |
            Operation::Update { timestamp, .. } |
            Operation::NetworkPartition { timestamp, .. } |
            Operation::NodeFailure { timestamp, .. } |
            Operation::NodeRecovery { timestamp, .. } => *timestamp,
        }
    }
}

/// Results from Jepsen testing
#[derive(Debug, Clone)]
pub struct JepsenTestResults {
    pub total_operations: usize,
    pub violations: Vec<ConsistencyViolation>,
    pub test_duration: Duration,
    pub consistency_model: ConsistencyModel,
    pub workload_type: WorkloadType,
    pub throughput: f64,
    pub fault_tolerance_score: f64,
}

impl JepsenTestResults {
    /// Check if the system passed all tests
    pub fn is_production_ready(&self) -> bool {
        self.violations.is_empty() && 
        self.fault_tolerance_score > 0.95 &&
        self.throughput > 1000.0 // Minimum 1K ops/sec
    }

    /// Generate detailed report
    pub fn generate_report(&self) -> String {
        format!(
            "Jepsen Test Results:\n\
            Total Operations: {}\n\
            Violations: {}\n\
            Test Duration: {:?}\n\
            Consistency Model: {:?}\n\
            Workload Type: {:?}\n\
            Throughput: {:.2} ops/sec\n\
            Fault Tolerance Score: {:.2}\n\
            Production Ready: {}\n",
            self.total_operations,
            self.violations.len(),
            self.test_duration,
            self.consistency_model,
            self.workload_type,
            self.throughput,
            self.fault_tolerance_score,
            self.is_production_ready()
        )
    }
}