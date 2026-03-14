//! Tests for the Jepsen testing framework

use super::*;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;

/// Mock RTDB client implementation for Jepsen testing.
/// 
/// Provides a simplified in-memory implementation of the RTDB client interface
/// for testing consistency properties without requiring a full cluster setup.
struct MockRtdbClient {
    /// Unique identifier for this client instance
    id: usize,
    /// Shared state storage for simulating database operations
    state: Arc<Mutex<std::collections::HashMap<String, serde_json::Value>>>,
}

impl MockRtdbClient {
    fn new(id: usize) -> Self {
        Self {
            id,
            state: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl JepsenClient for MockRtdbClient {
    async fn execute(&self, op: OperationType) -> Result<OperationResult> {
        let mut state = self.state.lock().await;
        
        match op {
            OperationType::Read { key } => {
                let value = state.get(&key).cloned();
                Ok(OperationResult::ReadOk { value })
            }
            OperationType::Write { key, value } => {
                state.insert(key, value);
                Ok(OperationResult::WriteOk)
            }
            OperationType::Cas { key, old, new } => {
                let current = state.get(&key).cloned();
                let success = current == Some(old);
                if success {
                    state.insert(key, new);
                }
                Ok(OperationResult::CasOk { success })
            }
            _ => Err(crate::RTDBError::Config("Unsupported operation".to_string())),
        }
    }

    fn id(&self) -> usize {
        self.id
    }

    async fn is_healthy(&self) -> bool {
        true
    }
}

/// Mock nemesis for testing
struct MockNemesis;

#[async_trait::async_trait]
impl Nemesis for MockNemesis {
    async fn start(&self) -> Result<()> {
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }

    async fn inject_fault(&self, fault: FaultType, nodes: Vec<usize>) -> Result<FaultEvent> {
        Ok(FaultEvent {
            fault_type: fault,
            start_time: SystemTime::now(),
            end_time: None,
            affected_nodes: nodes,
        })
    }

    async fn recover(&self, _fault_id: uuid::Uuid) -> Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn test_linearizability_checker() {
    let mut history = History {
        operations: vec![
            Operation {
                id: uuid::Uuid::new_v4(),
                process: 0,
                op: OperationType::Write { 
                    key: "x".to_string(), 
                    value: serde_json::Value::Number(1.into()) 
                },
                invoke_time: SystemTime::now(),
                complete_time: Some(SystemTime::now()),
                result: Some(OperationResult::WriteOk),
                error: None,
            },
            Operation {
                id: uuid::Uuid::new_v4(),
                process: 1,
                op: OperationType::Read { key: "x".to_string() },
                invoke_time: SystemTime::now(),
                complete_time: Some(SystemTime::now()),
                result: Some(OperationResult::ReadOk { 
                    value: Some(serde_json::Value::Number(1.into())) 
                }),
                error: None,
            },
        ],
        metadata: HistoryMetadata {
            config: JepsenConfig::default(),
            start_time: SystemTime::now(),
            end_time: SystemTime::now(),
            total_ops: 2,
            successful_ops: 2,
            failed_ops: 0,
            faults_injected: Vec::new(),
        },
    };

    let checker = checkers::LinearizabilityChecker::new();
    let result = checker.check(&history);
    
    assert!(result.valid, "Simple read-after-write should be linearizable");
    assert_eq!(result.violations.len(), 0);
}

#[tokio::test]
async fn test_jepsen_runner() {
    let config = JepsenConfig {
        name: "test-run".to_string(),
        duration: 1, // 1 second test
        rate: 10.0,
        concurrency: 2,
        ..Default::default()
    };

    let clients: Vec<Arc<dyn JepsenClient>> = vec![
        Arc::new(MockRtdbClient::new(0)),
        Arc::new(MockRtdbClient::new(1)),
    ];

    let nemesis = Arc::new(MockNemesis);
    let checker = checkers::create_checker(ConsistencyModel::Linearizability);

    let runner = JepsenRunner::new(config, clients, nemesis, checker);
    let result = runner.run().await.unwrap();

    assert!(!result.config.name.is_empty());
    assert!(result.history.operations.len() > 0);
    println!("Test completed with {} operations", result.history.operations.len());
}

#[test]
fn test_operation_generator() {
    let keys = vec!["key1".to_string(), "key2".to_string()];
    let generator = operations::OperationGenerator::new(keys);
    let mut rng = rand::thread_rng();

    for _ in 0..10 {
        let op = generator.generate(&mut rng);
        match op {
            OperationType::Read { key } => assert!(key.starts_with("key")),
            OperationType::Write { key, .. } => assert!(key.starts_with("key")),
            _ => {}
        }
    }
}

#[test]
fn test_workload_generators() {
    let mut rng = rand::thread_rng();

    // Test register workload
    let register_workload = workloads::RegisterWorkload::new(3, 0.5);
    for _ in 0..5 {
        let op = register_workload.generate_operation(&mut rng);
        match op {
            OperationType::Read { .. } | OperationType::Write { .. } => {}
            _ => panic!("Register workload should only generate reads and writes"),
        }
    }

    // Test bank workload
    let bank_workload = workloads::BankWorkload::new(5, 100);
    for _ in 0..5 {
        let op = bank_workload.generate_operation(&mut rng);
        match op {
            OperationType::Read { .. } | OperationType::Transaction { .. } => {}
            _ => panic!("Bank workload should only generate reads and transactions"),
        }
    }
}

#[test]
fn test_history_analysis() {
    let history = History {
        operations: vec![
            Operation {
                id: uuid::Uuid::new_v4(),
                process: 0,
                op: OperationType::Write { 
                    key: "x".to_string(), 
                    value: serde_json::Value::Number(1.into()) 
                },
                invoke_time: SystemTime::now(),
                complete_time: Some(SystemTime::now() + Duration::from_millis(10)),
                result: Some(OperationResult::WriteOk),
                error: None,
            },
        ],
        metadata: HistoryMetadata {
            config: JepsenConfig::default(),
            start_time: SystemTime::now(),
            end_time: SystemTime::now() + Duration::from_secs(1),
            total_ops: 1,
            successful_ops: 1,
            failed_ops: 0,
            faults_injected: Vec::new(),
        },
    };

    let latency_analysis = history::HistoryAnalyzer::analyze_latencies(&history);
    assert_eq!(latency_analysis.count, 1);
    assert!(latency_analysis.min >= Duration::from_millis(10));

    let error_rates = history::HistoryAnalyzer::analyze_error_rates(&history);
    assert!(error_rates.contains_key("write"));
    assert_eq!(error_rates["write"].successes, 1);
    assert_eq!(error_rates["write"].errors, 0);
}