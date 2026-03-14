//! Jepsen Testing Framework for RTDB
//!
//! Production-grade distributed systems testing framework inspired by Jepsen.
//! Tests linearizability, serializability, and distributed system correctness
//! under various failure scenarios.
//!
//! Features:
//! - Linearizability checking for single-key operations
//! - Serializability checking for multi-key transactions
//! - Strict serializability validation
//! - Network partition simulation
//! - Node failure injection
//! - Clock skew testing
//! - Concurrent operation history analysis
//! - Property-based testing integration

use crate::{Result, RTDBError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;
use rand::Rng;
use rand_distr::Distribution;

pub mod checkers;
pub mod generators;
pub mod history;
pub mod nemesis;
pub mod operations;
pub mod workloads;

#[cfg(test)]
mod tests;

/// Jepsen test configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JepsenConfig {
    /// Test name
    pub name: String,
    /// Number of nodes in the cluster
    pub node_count: usize,
    /// Test duration in seconds
    pub duration: u64,
    /// Operations per second rate
    pub rate: f64,
    /// Number of concurrent clients
    pub concurrency: usize,
    /// Network latency simulation (milliseconds)
    pub latency: u64,
    /// Latency distribution type
    pub latency_dist: LatencyDistribution,
    /// Nemesis configuration
    pub nemesis: NemesisConfig,
    /// Workload type
    pub workload: WorkloadType,
    /// Consistency model to test
    pub consistency_model: ConsistencyModel,
    /// Random seed for reproducibility
    pub seed: Option<u64>,
}

/// Network latency distribution types for fault injection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LatencyDistribution {
    /// Constant latency
    Constant,
    /// Uniform distribution between min and max
    Uniform { 
        /// Minimum latency in milliseconds
        min: u64, 
        /// Maximum latency in milliseconds
        max: u64 
    },
    /// Normal (Gaussian) distribution
    Normal { 
        /// Mean latency in milliseconds
        mean: f64, 
        /// Standard deviation
        std_dev: f64 
    },
    /// Exponential distribution
    Exponential { 
        /// Lambda parameter (rate)
        lambda: f64 
    },
}

/// Nemesis (fault injection) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NemesisConfig {
    /// Types of faults to inject
    pub faults: Vec<FaultType>,
    /// Average interval between fault injections (seconds)
    pub interval: f64,
    /// Duration of each fault (seconds)
    pub duration: f64,
    /// Enable fault injection
    pub enabled: bool,
}

/// Types of faults that can be injected
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FaultType {
    /// Kill node processes
    Kill,
    /// Pause node processes
    Pause,
    /// Create network partitions
    Partition(PartitionType),
    /// Clock skew injection
    ClockSkew { 
        /// Maximum skew in milliseconds
        max_skew_ms: i64 
    },
    /// Slow network simulation
    SlowNetwork { 
        /// Slowdown factor (1.0 = normal, 2.0 = 2x slower)
        factor: f64 
    },
    /// Packet loss simulation
    PacketLoss { 
        /// Loss rate (0.0 = no loss, 1.0 = 100% loss)
        rate: f64 
    },
}

/// Network partition types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PartitionType {
    /// Split into majority/minority
    MajorityMinority,
    /// Complete network partition
    Complete,
    /// Random partitions
    Random,
    /// Ring partition
    Ring,
}

/// Workload types for testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkloadType {
    /// Single register (linearizability)
    Register,
    /// Set operations (serializability)
    Set,
    /// Append operations (strict serializability)
    Append,
    /// Multi-key read/write transactions
    ReadWrite,
    /// Bank transfer simulation
    Bank,
    /// Counter increment operations
    Counter,
    /// List operations
    List,
}

/// Consistency models to test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsistencyModel {
    /// Linearizability (single object)
    Linearizability,
    /// Serializability (multi-object transactions)
    Serializability,
    /// Strict serializability (linearizable + serializable)
    StrictSerializability,
    /// Sequential consistency
    SequentialConsistency,
    /// Causal consistency
    CausalConsistency,
}

/// Operation in the history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    /// Unique operation ID
    pub id: Uuid,
    /// Process/client ID that invoked the operation
    pub process: usize,
    /// Operation type and parameters
    pub op: OperationType,
    /// Operation start time
    pub invoke_time: SystemTime,
    /// Operation completion time (None if still pending)
    pub complete_time: Option<SystemTime>,
    /// Operation result (None if failed or pending)
    pub result: Option<OperationResult>,
    /// Error if operation failed
    pub error: Option<String>,
}

/// Types of operations that can be performed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationType {
    /// Read operation
    Read { 
        /// Key to read
        key: String 
    },
    /// Write operation
    Write { 
        /// Key to write
        key: String, 
        /// Value to write
        value: serde_json::Value 
    },
    /// Compare-and-set operation
    Cas { 
        /// Key to update
        key: String, 
        /// Expected old value
        old: serde_json::Value, 
        /// New value to set
        new: serde_json::Value 
    },
    /// Transaction with multiple operations
    Transaction { 
        /// Operations to execute atomically
        ops: Vec<TransactionOp> 
    },
    /// Append to list
    Append { 
        /// Key to append to
        key: String, 
        /// Value to append
        value: serde_json::Value 
    },
    /// Set add operation
    SetAdd { 
        /// Key of the set
        key: String, 
        /// Element to add
        element: serde_json::Value 
    },
    /// Counter increment
    Increment { 
        /// Key of the counter
        key: String, 
        /// Amount to increment by
        delta: i64 
    },
}

/// Transaction operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionOp {
    /// Read operation in transaction
    Read { 
        /// Key to read
        key: String 
    },
    /// Write operation in transaction
    Write { 
        /// Key to write
        key: String, 
        /// Value to write
        value: serde_json::Value 
    },
}

/// Operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationResult {
    /// Read result
    ReadOk { 
        /// Value read (None if key doesn't exist)
        value: Option<serde_json::Value> 
    },
    /// Write result
    WriteOk,
    /// CAS result
    CasOk { 
        /// Whether the compare-and-set succeeded
        success: bool 
    },
    /// Transaction result
    TransactionOk { 
        /// Results of individual operations in the transaction
        results: Vec<TransactionResult> 
    },
    /// Append result
    AppendOk,
    /// Set add result
    SetAddOk,
    /// Increment result
    IncrementOk { 
        /// New value after increment
        new_value: i64 
    },
}

/// Transaction operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionResult {
    /// Read operation result
    Read { 
        /// Value read (None if key doesn't exist)
        value: Option<serde_json::Value> 
    },
    /// Write operation result
    Write,
}

/// Test execution history
#[derive(Debug, Clone)]
pub struct History {
    /// All operations in chronological order
    pub operations: Vec<Operation>,
    /// Test metadata
    pub metadata: HistoryMetadata,
}

/// History metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryMetadata {
    /// Test configuration
    pub config: JepsenConfig,
    /// Test start time
    pub start_time: SystemTime,
    /// Test end time
    pub end_time: SystemTime,
    /// Total operations performed
    pub total_ops: usize,
    /// Successful operations
    pub successful_ops: usize,
    /// Failed operations
    pub failed_ops: usize,
    /// Faults injected during test
    pub faults_injected: Vec<FaultEvent>,
}

/// Fault injection event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultEvent {
    /// Fault type
    pub fault_type: FaultType,
    /// Fault start time
    pub start_time: SystemTime,
    /// Fault end time
    pub end_time: Option<SystemTime>,
    /// Affected nodes
    pub affected_nodes: Vec<usize>,
}

/// Jepsen test runner
pub struct JepsenRunner {
    config: JepsenConfig,
    history: Arc<Mutex<History>>,
    clients: Vec<Arc<dyn JepsenClient>>,
    nemesis: Arc<dyn Nemesis>,
    checker: Arc<dyn Checker>,
}

/// Client interface for Jepsen tests
#[async_trait::async_trait]
pub trait JepsenClient: Send + Sync {
    /// Execute an operation
    async fn execute(&self, op: OperationType) -> Result<OperationResult>;
    
    /// Get client ID
    fn id(&self) -> usize;
    
    /// Check if client is healthy
    async fn is_healthy(&self) -> bool;
}

/// Nemesis interface for fault injection
#[async_trait::async_trait]
pub trait Nemesis: Send + Sync {
    /// Start fault injection
    async fn start(&self) -> Result<()>;
    
    /// Stop fault injection
    async fn stop(&self) -> Result<()>;
    
    /// Inject a specific fault
    async fn inject_fault(&self, fault: FaultType, nodes: Vec<usize>) -> Result<FaultEvent>;
    
    /// Recover from fault
    async fn recover(&self, fault_id: Uuid) -> Result<()>;
}

/// Checker interface for consistency validation
pub trait Checker: Send + Sync {
    /// Check history for consistency violations
    fn check(&self, history: &History) -> CheckerResult;
    
    /// Get checker name
    fn name(&self) -> &str;
    
    /// Get consistency model
    fn consistency_model(&self) -> ConsistencyModel;
}

/// Checker result
#[derive(Debug, Clone)]
pub struct CheckerResult {
    /// Whether the history is consistent
    pub valid: bool,
    /// Consistency model checked
    pub model: ConsistencyModel,
    /// Violations found (if any)
    pub violations: Vec<Violation>,
    /// Analysis metadata
    pub metadata: CheckerMetadata,
}

/// Consistency violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    /// Violation type
    pub violation_type: ViolationType,
    /// Operations involved in the violation
    pub operations: Vec<Uuid>,
    /// Human-readable description
    pub description: String,
    /// Additional context
    pub context: HashMap<String, serde_json::Value>,
}

/// Types of consistency violations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViolationType {
    /// Linearizability violation
    LinearizabilityViolation,
    /// Serializability violation
    SerializabilityViolation,
    /// Strict serializability violation
    StrictSerializabilityViolation,
    /// Causal consistency violation
    CausalConsistencyViolation,
    /// Data race
    DataRace,
    /// Lost update
    LostUpdate,
    /// Dirty read
    DirtyRead,
    /// Non-repeatable read
    NonRepeatableRead,
    /// Phantom read
    PhantomRead,
}

/// Checker metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckerMetadata {
    /// Time taken to check
    pub check_duration: Duration,
    /// Number of operations analyzed
    pub operations_analyzed: usize,
    /// Checker-specific statistics
    pub stats: HashMap<String, serde_json::Value>,
}

impl JepsenRunner {
    /// Create a new Jepsen test runner
    pub fn new(
        config: JepsenConfig,
        clients: Vec<Arc<dyn JepsenClient>>,
        nemesis: Arc<dyn Nemesis>,
        checker: Arc<dyn Checker>,
    ) -> Self {
        let history = Arc::new(Mutex::new(History {
            operations: Vec::new(),
            metadata: HistoryMetadata {
                config: config.clone(),
                start_time: SystemTime::now(),
                end_time: SystemTime::now(),
                total_ops: 0,
                successful_ops: 0,
                failed_ops: 0,
                faults_injected: Vec::new(),
            },
        }));

        Self {
            config,
            history,
            clients,
            nemesis,
            checker,
        }
    }

    /// Run the Jepsen test
    pub async fn run(&self) -> Result<JepsenTestResult> {
        tracing::info!("Starting Jepsen test: {}", self.config.name);
        
        let start_time = Instant::now();
        
        // Initialize history
        {
            let mut history = self.history.lock().await;
            history.metadata.start_time = SystemTime::now();
        }

        // Start nemesis if enabled
        if self.config.nemesis.enabled {
            self.nemesis.start().await?;
        }

        // Start client workers
        let mut client_handles = Vec::new();
        let (op_tx, mut op_rx) = mpsc::channel::<Operation>(1000);

        // Spawn client workers
        for client in &self.clients {
            let client = client.clone();
            let config = self.config.clone();
            let tx = op_tx.clone();
            
            let handle = tokio::spawn(async move {
                Self::client_worker(client, config, tx).await
            });
            
            client_handles.push(handle);
        }

        // Spawn nemesis worker
        let nemesis_handle = if self.config.nemesis.enabled {
            let nemesis = self.nemesis.clone();
            let config = self.config.nemesis.clone();
            let history = self.history.clone();
            
            Some(tokio::spawn(async move {
                Self::nemesis_worker(nemesis, config, history).await
            }))
        } else {
            None
        };

        // Collect operations
        let history_handle = {
            let history = self.history.clone();
            tokio::spawn(async move {
                while let Some(op) = op_rx.recv().await {
                    let mut hist = history.lock().await;
                    hist.operations.push(op);
                }
            })
        };

        // Wait for test duration
        tokio::time::sleep(Duration::from_secs(self.config.duration)).await;

        // Stop nemesis
        if self.config.nemesis.enabled {
            self.nemesis.stop().await?;
        }

        // Cancel client workers
        for handle in client_handles {
            handle.abort();
        }

        // Cancel nemesis worker
        if let Some(handle) = nemesis_handle {
            handle.abort();
        }

        // Finalize history
        drop(op_tx);
        history_handle.await.map_err(|e| RTDBError::Config(e.to_string()))?;

        {
            let mut history = self.history.lock().await;
            history.metadata.end_time = SystemTime::now();
            history.metadata.total_ops = history.operations.len();
            history.metadata.successful_ops = history.operations.iter()
                .filter(|op| op.result.is_some())
                .count();
            history.metadata.failed_ops = history.operations.iter()
                .filter(|op| op.error.is_some())
                .count();
        }

        // Run consistency checker
        let history = self.history.lock().await.clone();
        let checker_result = self.checker.check(&history);

        let test_duration = start_time.elapsed();
        
        tracing::info!(
            "Jepsen test completed in {:?}. Operations: {}, Violations: {}",
            test_duration,
            history.metadata.total_ops,
            checker_result.violations.len()
        );

        Ok(JepsenTestResult {
            config: self.config.clone(),
            history,
            checker_result,
            duration: test_duration,
        })
    }

    /// Client worker that generates and executes operations
    async fn client_worker(
        client: Arc<dyn JepsenClient>,
        config: JepsenConfig,
        tx: mpsc::Sender<Operation>,
    ) -> Result<()> {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::from_entropy();
        let workload = workloads::create_workload(config.workload.clone());
        
        let ops_per_second = config.rate / config.concurrency as f64;
        let interval = Duration::from_secs_f64(1.0 / ops_per_second);
        
        let mut next_op_time = Instant::now();
        
        loop {
            // Wait for next operation time
            if let Some(sleep_duration) = next_op_time.checked_duration_since(Instant::now()) {
                tokio::time::sleep(sleep_duration).await;
            }
            next_op_time += interval;

            // Generate operation
            let op_type = workload.generate_operation(&mut rng);
            let op_id = Uuid::new_v4();
            
            let mut operation = Operation {
                id: op_id,
                process: client.id(),
                op: op_type.clone(),
                invoke_time: SystemTime::now(),
                complete_time: None,
                result: None,
                error: None,
            };

            // Execute operation
            match client.execute(op_type).await {
                Ok(result) => {
                    operation.complete_time = Some(SystemTime::now());
                    operation.result = Some(result);
                }
                Err(e) => {
                    operation.complete_time = Some(SystemTime::now());
                    operation.error = Some(e.to_string());
                }
            }

            // Send to history collector
            if tx.send(operation).await.is_err() {
                break; // Channel closed, test is ending
            }
        }

        Ok(())
    }

    /// Nemesis worker that injects faults
    async fn nemesis_worker(
        nemesis: Arc<dyn Nemesis>,
        config: NemesisConfig,
        history: Arc<Mutex<History>>,
    ) -> Result<()> {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::from_entropy();
        
        loop {
            // Wait for next fault injection
            let wait_time = Duration::from_secs_f64(
                rand_distr::Exp::new(1.0 / config.interval)
                    .unwrap()
                    .sample(&mut rng)
            );
            tokio::time::sleep(wait_time).await;

            // Select random fault type
            if let Some(fault_type) = config.faults.get(rng.gen_range(0..config.faults.len())) {
                // Select random nodes to affect
                let num_nodes = match fault_type {
                    FaultType::Partition(PartitionType::MajorityMinority) => 2, // Will be split
                    _ => rand::thread_rng().gen_range(1..=3), // Affect 1-3 nodes
                };
                
                let affected_nodes: Vec<usize> = (0..num_nodes).collect();
                
                // Inject fault
                match nemesis.inject_fault(fault_type.clone(), affected_nodes).await {
                    Ok(fault_event) => {
                        let mut hist = history.lock().await;
                        hist.metadata.faults_injected.push(fault_event);
                    }
                    Err(e) => {
                        tracing::error!("Failed to inject fault: {}", e);
                    }
                }

                // Wait for fault duration
                tokio::time::sleep(Duration::from_secs_f64(config.duration)).await;

                // Recover from fault (implementation depends on fault type)
                // This is a simplified recovery - real implementation would track fault IDs
            }
        }
    }
}

/// Jepsen test result
#[derive(Debug, Clone)]
pub struct JepsenTestResult {
    /// Test configuration
    pub config: JepsenConfig,
    /// Execution history
    pub history: History,
    /// Consistency checker result
    pub checker_result: CheckerResult,
    /// Test duration
    pub duration: Duration,
}

impl JepsenTestResult {
    /// Check if the test passed (no consistency violations)
    pub fn is_valid(&self) -> bool {
        self.checker_result.valid
    }

    /// Get summary statistics
    pub fn summary(&self) -> TestSummary {
        TestSummary {
            test_name: self.config.name.clone(),
            duration: self.duration,
            total_operations: self.history.metadata.total_ops,
            successful_operations: self.history.metadata.successful_ops,
            failed_operations: self.history.metadata.failed_ops,
            faults_injected: self.history.metadata.faults_injected.len(),
            consistency_violations: self.checker_result.violations.len(),
            is_valid: self.is_valid(),
        }
    }
}

/// Test summary statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSummary {
    /// Name of the test
    pub test_name: String,
    /// Total test duration
    pub duration: Duration,
    /// Total number of operations attempted
    pub total_operations: usize,
    /// Number of successful operations
    pub successful_operations: usize,
    /// Number of failed operations
    pub failed_operations: usize,
    /// Number of faults injected during test
    pub faults_injected: usize,
    /// Number of consistency violations detected
    pub consistency_violations: usize,
    /// Whether the test passed (no consistency violations)
    pub is_valid: bool,
}

/// Default configuration for common test scenarios
impl Default for JepsenConfig {
    fn default() -> Self {
        Self {
            name: "default-jepsen-test".to_string(),
            node_count: 3,
            duration: 60,
            rate: 100.0,
            concurrency: 10,
            latency: 10,
            latency_dist: LatencyDistribution::Constant,
            nemesis: NemesisConfig {
                faults: vec![
                    FaultType::Partition(PartitionType::MajorityMinority),
                    FaultType::Kill,
                    FaultType::Pause,
                ],
                interval: 30.0,
                duration: 10.0,
                enabled: true,
            },
            workload: WorkloadType::Register,
            consistency_model: ConsistencyModel::Linearizability,
            seed: None,
        }
    }
}

// Re-export commonly used types
pub use checkers::*;
pub use generators::*;
pub use history::*;
pub use nemesis::*;
pub use operations::*;
pub use workloads::*;