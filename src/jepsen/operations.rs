//! Operation generators and utilities for Jepsen tests

use super::{OperationType, TransactionOp};
use rand::Rng;
use serde_json::Value;

/// Generator for random database operations used in Jepsen testing.
/// 
/// Creates weighted random operations (read, write, CAS, transactions, etc.)
/// against a configurable set of keys for comprehensive system testing.
pub struct OperationGenerator {
    /// Available keys for operations
    keys: Vec<String>,
    /// Operation type weights
    weights: OperationWeights,
}

/// Weights for different operation types in test generation.
/// 
/// Controls the probability distribution of operation types generated
/// during Jepsen testing, allowing customization of workload characteristics.
#[derive(Debug, Clone)]
pub struct OperationWeights {
    /// Weight for read operations
    pub read: f64,
    /// Weight for write operations
    pub write: f64,
    /// Weight for compare-and-swap operations
    pub cas: f64,
    /// Weight for transaction operations
    pub transaction: f64,
    /// Weight for append operations
    pub append: f64,
    /// Weight for increment operations
    pub increment: f64,
}

impl Default for OperationWeights {
    fn default() -> Self {
        Self {
            read: 0.4,
            write: 0.3,
            cas: 0.1,
            transaction: 0.1,
            append: 0.05,
            increment: 0.05,
        }
    }
}

impl OperationGenerator {
    /// Create a new operation generator with default weights.
    /// 
    /// # Arguments
    /// * `keys` - Vector of keys that operations can target
    pub fn new(keys: Vec<String>) -> Self {
        Self {
            keys,
            weights: OperationWeights::default(),
        }
    }

    /// Create a new operation generator with custom weights.
    /// 
    /// # Arguments
    /// * `keys` - Vector of keys that operations can target
    /// * `weights` - Custom weights for different operation types
    pub fn with_weights(keys: Vec<String>, weights: OperationWeights) -> Self {
        Self { keys, weights }
    }

    /// Generate a random operation
    pub fn generate(&self, rng: &mut dyn rand::RngCore) -> OperationType {
        let total_weight = self.weights.read + self.weights.write + self.weights.cas 
            + self.weights.transaction + self.weights.append + self.weights.increment;
        
        let mut threshold = rng.gen::<f64>() * total_weight;
        
        threshold -= self.weights.read;
        if threshold <= 0.0 {
            return self.generate_read(rng);
        }
        
        threshold -= self.weights.write;
        if threshold <= 0.0 {
            return self.generate_write(rng);
        }
        
        threshold -= self.weights.cas;
        if threshold <= 0.0 {
            return self.generate_cas(rng);
        }
        
        threshold -= self.weights.transaction;
        if threshold <= 0.0 {
            return self.generate_transaction(rng);
        }
        
        threshold -= self.weights.append;
        if threshold <= 0.0 {
            return self.generate_append(rng);
        }
        
        self.generate_increment(rng)
    }

    fn generate_read(&self, rng: &mut dyn rand::RngCore) -> OperationType {
        let key = self.random_key(rng);
        OperationType::Read { key }
    }

    fn generate_write(&self, rng: &mut dyn rand::RngCore) -> OperationType {
        let key = self.random_key(rng);
        let value = self.random_value(rng);
        OperationType::Write { key, value }
    }

    fn generate_cas(&self, rng: &mut dyn rand::RngCore) -> OperationType {
        let key = self.random_key(rng);
        let old = self.random_value(rng);
        let new = self.random_value(rng);
        OperationType::Cas { key, old, new }
    }

    fn generate_transaction(&self, rng: &mut dyn rand::RngCore) -> OperationType {
        let txn_size = rng.gen_range(1..=5);
        let mut ops = Vec::new();
        
        for _ in 0..txn_size {
            let key = self.random_key(rng);
            
            if rng.gen::<f64>() < 0.6 {
                ops.push(TransactionOp::Read { key });
            } else {
                let value = self.random_value(rng);
                ops.push(TransactionOp::Write { key, value });
            }
        }
        
        OperationType::Transaction { ops }
    }

    fn generate_append(&self, rng: &mut dyn rand::RngCore) -> OperationType {
        let key = self.random_key(rng);
        let value = self.random_value(rng);
        OperationType::Append { key, value }
    }

    fn generate_increment(&self, rng: &mut dyn rand::RngCore) -> OperationType {
        let key = self.random_key(rng);
        let delta = rng.gen_range(-10..=10);
        OperationType::Increment { key, delta }
    }

    fn random_key(&self, rng: &mut dyn rand::RngCore) -> String {
        self.keys[rng.gen_range(0..self.keys.len())].clone()
    }

    fn random_value(&self, rng: &mut dyn rand::RngCore) -> Value {
        match rng.gen_range(0..4) {
            0 => Value::Number(rng.gen::<u64>().into()),
            1 => Value::String(format!("value-{}", rng.gen::<u32>())),
            2 => Value::Bool(rng.gen()),
            _ => Value::Null,
        }
    }
}

/// Utility functions for operation analysis
pub mod analysis {
    use super::super::{Operation, OperationType};
    use std::collections::{HashMap, HashSet};

    /// Extract all keys accessed by an operation for conflict analysis.
    /// 
    /// # Arguments
    /// * `op` - The operation to analyze
    /// 
    /// # Returns
    /// A set of all keys that the operation accesses
    pub fn extract_keys(op: &OperationType) -> HashSet<String> {
        let mut keys = HashSet::new();
        
        match op {
            OperationType::Read { key } => {
                keys.insert(key.clone());
            }
            OperationType::Write { key, .. } => {
                keys.insert(key.clone());
            }
            OperationType::Cas { key, .. } => {
                keys.insert(key.clone());
            }
            OperationType::Transaction { ops } => {
                for txn_op in ops {
                    match txn_op {
                        super::super::TransactionOp::Read { key } => {
                            keys.insert(key.clone());
                        }
                        super::super::TransactionOp::Write { key, .. } => {
                            keys.insert(key.clone());
                        }
                    }
                }
            }
            OperationType::Append { key, .. } => {
                keys.insert(key.clone());
            }
            OperationType::SetAdd { key, .. } => {
                keys.insert(key.clone());
            }
            OperationType::Increment { key, .. } => {
                keys.insert(key.clone());
            }
        }
        
        keys
    }

    /// Check if two operations conflict (access same keys with at least one write).
    /// 
    /// Operations conflict if they access overlapping keys and at least one is a write operation.
    /// 
    /// # Arguments
    /// * `op1` - First operation to check
    /// * `op2` - Second operation to check
    /// 
    /// # Returns
    /// True if the operations conflict, false otherwise
    pub fn operations_conflict(op1: &OperationType, op2: &OperationType) -> bool {
        let keys1 = extract_keys(op1);
        let keys2 = extract_keys(op2);
        
        // Check if they access overlapping keys
        if keys1.is_disjoint(&keys2) {
            return false;
        }
        
        // Check if at least one is a write operation
        is_write_operation(op1) || is_write_operation(op2)
    }

    /// Check if an operation is a write operation that modifies data.
    /// 
    /// # Arguments
    /// * `op` - The operation to check
    /// 
    /// # Returns
    /// True if the operation modifies data, false for read-only operations
    pub fn is_write_operation(op: &OperationType) -> bool {
        match op {
            OperationType::Read { .. } => false,
            OperationType::Write { .. } => true,
            OperationType::Cas { .. } => true, // CAS can write
            OperationType::Transaction { ops } => {
                ops.iter().any(|txn_op| matches!(txn_op, super::super::TransactionOp::Write { .. }))
            }
            OperationType::Append { .. } => true,
            OperationType::SetAdd { .. } => true,
            OperationType::Increment { .. } => true,
        }
    }

    /// Group operations by the keys they access for analysis.
    /// 
    /// Creates a mapping from keys to all operations that access those keys,
    /// useful for analyzing key-specific operation patterns and conflicts.
    /// 
    /// # Arguments
    /// * `operations` - Slice of operations to group
    /// 
    /// # Returns
    /// HashMap mapping each key to the operations that access it
    pub fn group_by_keys(operations: &[Operation]) -> HashMap<String, Vec<&Operation>> {
        let mut groups = HashMap::new();
        
        for op in operations {
            let keys = extract_keys(&op.op);
            for key in keys {
                groups.entry(key).or_insert_with(Vec::new).push(op);
            }
        }
        
        groups
    }

    /// Calculate operation statistics for test analysis and reporting.
    /// 
    /// Analyzes a collection of operations to compute counts by type,
    /// success/failure rates, and other statistical metrics.
    /// 
    /// # Arguments
    /// * `operations` - Slice of operations to analyze
    /// 
    /// # Returns
    /// OperationStats containing detailed statistics about the operations
    pub fn calculate_stats(operations: &[Operation]) -> OperationStats {
        let mut stats = OperationStats::default();
        
        for op in operations {
            match &op.op {
                OperationType::Read { .. } => stats.reads += 1,
                OperationType::Write { .. } => stats.writes += 1,
                OperationType::Cas { .. } => stats.cas_ops += 1,
                OperationType::Transaction { .. } => stats.transactions += 1,
                OperationType::Append { .. } => stats.appends += 1,
                OperationType::SetAdd { .. } => stats.set_adds += 1,
                OperationType::Increment { .. } => stats.increments += 1,
            }
            
            if op.result.is_some() {
                stats.successful += 1;
            } else if op.error.is_some() {
                stats.failed += 1;
            }
        }
        
        stats.total = operations.len();
        stats
    }

    /// Operation statistics for analyzing test execution patterns and performance.
    /// 
    /// Tracks counts of different operation types and their success/failure rates
    /// for comprehensive analysis of Jepsen test execution.
    #[derive(Debug, Default, Clone)]
    pub struct OperationStats {
        /// Total number of operations executed
        pub total: usize,
        /// Number of successful operations
        pub successful: usize,
        /// Number of failed operations
        pub failed: usize,
        /// Number of read operations
        pub reads: usize,
        /// Number of write operations
        pub writes: usize,
        /// Number of compare-and-swap operations
        pub cas_ops: usize,
        /// Number of transaction operations
        pub transactions: usize,
        /// Number of append operations
        pub appends: usize,
        /// Number of set add operations
        pub set_adds: usize,
        /// Number of increment operations
        pub increments: usize,
    }
}