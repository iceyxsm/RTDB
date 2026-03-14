//! Workload generators for Jepsen tests
//!
//! Provides various workload patterns to test different aspects of distributed systems:
//! - Register workload (linearizability testing)
//! - Set workload (serializability testing)
//! - Bank workload (transaction testing)
//! - Counter workload (increment operations)

use super::{OperationType, TransactionOp, WorkloadType};
use rand::Rng;
use serde_json::Value;

/// Trait for defining test workloads in Jepsen testing scenarios.
/// 
/// Workloads define the pattern of operations to be executed during testing,
/// including operation generation, naming, and expected consistency models.
pub trait Workload: Send + Sync {
    /// Generate a random operation based on workload characteristics
    fn generate_operation(&self, rng: &mut dyn rand::RngCore) -> OperationType;
    
    /// Get the human-readable name of this workload
    fn name(&self) -> &str;
    
    /// Get the expected consistency model for this workload
    fn consistency_model(&self) -> super::ConsistencyModel;
}

/// Register workload for linearizability testing of single-register operations.
/// 
/// Implements a classic Jepsen workload that performs read and write operations
/// on a set of registers (keys) with configurable read/write ratios.
pub struct RegisterWorkload {
    /// Keys to operate on during testing
    keys: Vec<String>,
    /// Read/write ratio (0.0 = all writes, 1.0 = all reads)
    read_ratio: f64,
}

impl RegisterWorkload {
    /// Create a new register workload with specified parameters.
    /// 
    /// # Arguments
    /// * `num_keys` - Number of keys to operate on during testing
    /// * `read_ratio` - Ratio of read to write operations (0.0 = all writes, 1.0 = all reads)
    pub fn new(num_keys: usize, read_ratio: f64) -> Self {
        let keys = (0..num_keys).map(|i| format!("key-{}", i)).collect();
        Self { keys, read_ratio }
    }
}

impl Workload for RegisterWorkload {
    fn generate_operation(&self, rng: &mut dyn rand::RngCore) -> OperationType {
        let key = self.keys[rng.gen_range(0..self.keys.len())].clone();
        
        if rng.gen::<f64>() < self.read_ratio {
            OperationType::Read { key }
        } else {
            let value = Value::Number(rng.gen::<u64>().into());
            OperationType::Write { key, value }
        }
    }

    fn name(&self) -> &str {
        "register"
    }

    fn consistency_model(&self) -> super::ConsistencyModel {
        super::ConsistencyModel::Linearizability
    }
}

/// Set workload for serializability testing
pub struct SetWorkload {
    /// Set keys to operate on
    keys: Vec<String>,
    /// Elements to add to sets
    elements: Vec<Value>,
}

impl SetWorkload {
    /// Create a new set workload for serializability testing.
    /// 
    /// # Arguments
    /// * `num_keys` - Number of set keys to operate on
    /// * `num_elements` - Number of elements available for set operations
    pub fn new(num_keys: usize, num_elements: usize) -> Self {
        let keys = (0..num_keys).map(|i| format!("set-{}", i)).collect();
        let elements = (0..num_elements).map(|i| Value::Number(i.into())).collect();
        Self { keys, elements }
    }
}

impl Workload for SetWorkload {
    fn generate_operation(&self, rng: &mut dyn rand::RngCore) -> OperationType {
        let key = self.keys[rng.gen_range(0..self.keys.len())].clone();
        let element = self.elements[rng.gen_range(0..self.elements.len())].clone();
        
        OperationType::SetAdd { key, element }
    }

    fn name(&self) -> &str {
        "set"
    }

    fn consistency_model(&self) -> super::ConsistencyModel {
        super::ConsistencyModel::Serializability
    }
}

/// Append workload for strict serializability testing
pub struct AppendWorkload {
    /// List keys to operate on
    keys: Vec<String>,
    /// Read/append ratio
    read_ratio: f64,
}

impl AppendWorkload {
    /// Create a new append workload for strict serializability testing.
    /// 
    /// # Arguments
    /// * `num_keys` - Number of list keys to operate on
    /// * `read_ratio` - Ratio of read to append operations (0.0 = all appends, 1.0 = all reads)
    pub fn new(num_keys: usize, read_ratio: f64) -> Self {
        let keys = (0..num_keys).map(|i| format!("list-{}", i)).collect();
        Self { keys, read_ratio }
    }
}

impl Workload for AppendWorkload {
    fn generate_operation(&self, rng: &mut dyn rand::RngCore) -> OperationType {
        let key = self.keys[rng.gen_range(0..self.keys.len())].clone();
        
        if rng.gen::<f64>() < self.read_ratio {
            OperationType::Read { key }
        } else {
            let value = Value::Number(rng.gen::<u64>().into());
            OperationType::Append { key, value }
        }
    }

    fn name(&self) -> &str {
        "append"
    }

    fn consistency_model(&self) -> super::ConsistencyModel {
        super::ConsistencyModel::StrictSerializability
    }
}

/// Bank workload for transaction testing
pub struct BankWorkload {
    /// Account keys
    accounts: Vec<String>,
    /// Maximum transfer amount
    max_transfer: u64,
}

impl BankWorkload {
    /// Create a new bank workload for transaction testing.
    /// 
    /// # Arguments
    /// * `num_accounts` - Number of bank accounts to create for testing
    /// * `max_transfer` - Maximum amount that can be transferred in a single transaction
    pub fn new(num_accounts: usize, max_transfer: u64) -> Self {
        let accounts = (0..num_accounts).map(|i| format!("account-{}", i)).collect();
        Self { accounts, max_transfer }
    }
}

impl Workload for BankWorkload {
    fn generate_operation(&self, rng: &mut dyn rand::RngCore) -> OperationType {
        if rng.gen::<f64>() < 0.3 {
            // Read balance
            let account = self.accounts[rng.gen_range(0..self.accounts.len())].clone();
            OperationType::Read { key: account }
        } else {
            // Transfer between accounts
            let from_idx = rng.gen_range(0..self.accounts.len());
            let mut to_idx = rng.gen_range(0..self.accounts.len());
            while to_idx == from_idx {
                to_idx = rng.gen_range(0..self.accounts.len());
            }
            
            let from_account = self.accounts[from_idx].clone();
            let to_account = self.accounts[to_idx].clone();
            let amount = rng.gen_range(1..=self.max_transfer);
            
            let ops = vec![
                TransactionOp::Read { key: from_account.clone() },
                TransactionOp::Read { key: to_account.clone() },
                TransactionOp::Write { 
                    key: from_account, 
                    value: Value::String(format!("subtract-{}", amount))
                },
                TransactionOp::Write { 
                    key: to_account, 
                    value: Value::String(format!("add-{}", amount))
                },
            ];
            
            OperationType::Transaction { ops }
        }
    }

    fn name(&self) -> &str {
        "bank"
    }

    fn consistency_model(&self) -> super::ConsistencyModel {
        super::ConsistencyModel::StrictSerializability
    }
}

/// Counter workload for increment operations
pub struct CounterWorkload {
    /// Counter keys
    counters: Vec<String>,
    /// Maximum increment value
    max_increment: i64,
}

impl CounterWorkload {
    /// Create a new counter workload for increment operations testing.
    /// 
    /// # Arguments
    /// * `num_counters` - Number of counters to create for testing
    /// * `max_increment` - Maximum value that can be added to a counter in one operation
    pub fn new(num_counters: usize, max_increment: i64) -> Self {
        let counters = (0..num_counters).map(|i| format!("counter-{}", i)).collect();
        Self { counters, max_increment }
    }
}

impl Workload for CounterWorkload {
    fn generate_operation(&self, rng: &mut dyn rand::RngCore) -> OperationType {
        let key = self.counters[rng.gen_range(0..self.counters.len())].clone();
        
        if rng.gen::<f64>() < 0.2 {
            // Read counter
            OperationType::Read { key }
        } else {
            // Increment counter
            let delta = rng.gen_range(1..=self.max_increment);
            OperationType::Increment { key, delta }
        }
    }

    fn name(&self) -> &str {
        "counter"
    }

    fn consistency_model(&self) -> super::ConsistencyModel {
        super::ConsistencyModel::Linearizability
    }
}

/// Read-write workload for multi-key transactions
pub struct ReadWriteWorkload {
    /// Keys to operate on
    keys: Vec<String>,
    /// Maximum transaction size
    max_txn_size: usize,
    /// Read ratio within transactions
    read_ratio: f64,
}

impl ReadWriteWorkload {
    /// Create a new read-write workload for multi-key transaction testing.
    /// 
    /// # Arguments
    /// * `num_keys` - Number of keys available for transaction operations
    /// * `max_txn_size` - Maximum number of operations per transaction
    /// * `read_ratio` - Ratio of read to write operations within transactions
    pub fn new(num_keys: usize, max_txn_size: usize, read_ratio: f64) -> Self {
        let keys = (0..num_keys).map(|i| format!("rw-key-{}", i)).collect();
        Self { keys, max_txn_size, read_ratio }
    }
}

impl Workload for ReadWriteWorkload {
    fn generate_operation(&self, rng: &mut dyn rand::RngCore) -> OperationType {
        let txn_size = rng.gen_range(1..=self.max_txn_size);
        let mut ops = Vec::new();
        
        for _ in 0..txn_size {
            let key = self.keys[rng.gen_range(0..self.keys.len())].clone();
            
            if rng.gen::<f64>() < self.read_ratio {
                ops.push(TransactionOp::Read { key });
            } else {
                let value = Value::Number(rng.gen::<u64>().into());
                ops.push(TransactionOp::Write { key, value });
            }
        }
        
        OperationType::Transaction { ops }
    }

    fn name(&self) -> &str {
        "read-write"
    }

    fn consistency_model(&self) -> super::ConsistencyModel {
        super::ConsistencyModel::Serializability
    }
}

/// Create a workload generator for the specified workload type with default parameters.
/// 
/// # Arguments
/// * `workload_type` - The type of workload to create
/// 
/// # Returns
/// A boxed workload generator implementing the Workload trait
pub fn create_workload(workload_type: WorkloadType) -> Box<dyn Workload> {
    match workload_type {
        WorkloadType::Register => Box::new(RegisterWorkload::new(5, 0.5)),
        WorkloadType::Set => Box::new(SetWorkload::new(3, 10)),
        WorkloadType::Append => Box::new(AppendWorkload::new(3, 0.3)),
        WorkloadType::ReadWrite => Box::new(ReadWriteWorkload::new(10, 4, 0.6)),
        WorkloadType::Bank => Box::new(BankWorkload::new(5, 100)),
        WorkloadType::Counter => Box::new(CounterWorkload::new(3, 10)),
        WorkloadType::List => Box::new(AppendWorkload::new(3, 0.3)), // Alias for append
    }
}

/// Create a workload with custom parameters from a configuration map.
/// 
/// # Arguments
/// * `workload_type` - The type of workload to create
/// * `params` - HashMap containing custom parameters for the workload
/// 
/// # Returns
/// A boxed workload generator configured with the specified parameters
pub fn create_custom_workload(
    workload_type: WorkloadType,
    params: &std::collections::HashMap<String, serde_json::Value>,
) -> Box<dyn Workload> {
    match workload_type {
        WorkloadType::Register => {
            let num_keys = params.get("num_keys")
                .and_then(|v| v.as_u64())
                .unwrap_or(5) as usize;
            let read_ratio = params.get("read_ratio")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5);
            Box::new(RegisterWorkload::new(num_keys, read_ratio))
        }
        WorkloadType::Set => {
            let num_keys = params.get("num_keys")
                .and_then(|v| v.as_u64())
                .unwrap_or(3) as usize;
            let num_elements = params.get("num_elements")
                .and_then(|v| v.as_u64())
                .unwrap_or(10) as usize;
            Box::new(SetWorkload::new(num_keys, num_elements))
        }
        WorkloadType::Bank => {
            let num_accounts = params.get("num_accounts")
                .and_then(|v| v.as_u64())
                .unwrap_or(5) as usize;
            let max_transfer = params.get("max_transfer")
                .and_then(|v| v.as_u64())
                .unwrap_or(100);
            Box::new(BankWorkload::new(num_accounts, max_transfer))
        }
        _ => create_workload(workload_type),
    }
}