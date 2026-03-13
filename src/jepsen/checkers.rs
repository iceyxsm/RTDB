//! Consistency checkers for Jepsen tests
//!
//! Implements various consistency model checkers including:
//! - Linearizability checker (Knossos-style)
//! - Serializability checker (Elle-style)
//! - Strict serializability checker

use super::{
    Checker, CheckerResult, CheckerMetadata, ConsistencyModel, History, Operation,
    OperationType, OperationResult, Violation, ViolationType,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

/// Linearizability checker for single-object operations
pub struct LinearizabilityChecker {
    /// Maximum search depth for linearization
    max_depth: usize,
}

impl LinearizabilityChecker {
    pub fn new() -> Self {
        Self { max_depth: 1000 }
    }

    pub fn with_max_depth(max_depth: usize) -> Self {
        Self { max_depth }
    }
}

impl Checker for LinearizabilityChecker {
    fn check(&self, history: &History) -> CheckerResult {
        let start_time = Instant::now();
        
        // Filter operations for linearizability checking
        let ops: Vec<&Operation> = history.operations.iter()
            .filter(|op| matches!(op.op, OperationType::Read { .. } | OperationType::Write { .. } | OperationType::Cas { .. }))
            .filter(|op| op.complete_time.is_some())
            .collect();

        let violations = self.find_linearizability_violations(&ops);
        
        CheckerResult {
            valid: violations.is_empty(),
            model: ConsistencyModel::Linearizability,
            violations,
            metadata: CheckerMetadata {
                check_duration: start_time.elapsed(),
                operations_analyzed: ops.len(),
                stats: HashMap::new(),
            },
        }
    }

    fn name(&self) -> &str {
        "linearizability"
    }

    fn consistency_model(&self) -> ConsistencyModel {
        ConsistencyModel::Linearizability
    }
}

impl LinearizabilityChecker {
    fn find_linearizability_violations(&self, ops: &[&Operation]) -> Vec<Violation> {
        // Group operations by key
        let mut key_ops: HashMap<String, Vec<&Operation>> = HashMap::new();
        
        for op in ops {
            let key = match &op.op {
                OperationType::Read { key } => key.clone(),
                OperationType::Write { key, .. } => key.clone(),
                OperationType::Cas { key, .. } => key.clone(),
                _ => continue,
            };
            
            key_ops.entry(key).or_default().push(op);
        }

        let mut violations = Vec::new();
        
        // Check linearizability for each key separately
        for (key, key_operations) in key_ops {
            if let Some(violation) = self.check_key_linearizability(&key, &key_operations) {
                violations.push(violation);
            }
        }
        
        violations
    }

    fn check_key_linearizability(&self, key: &str, ops: &[&Operation]) -> Option<Violation> {
        // Sort operations by invoke time
        let mut sorted_ops = ops.to_vec();
        sorted_ops.sort_by_key(|op| op.invoke_time);

        // Try to find a valid linearization
        if self.is_linearizable(&sorted_ops) {
            None
        } else {
            Some(Violation {
                violation_type: ViolationType::LinearizabilityViolation,
                operations: sorted_ops.iter().map(|op| op.id).collect(),
                description: format!("Linearizability violation found for key '{}'", key),
                context: HashMap::new(),
            })
        }
    }

    fn is_linearizable(&self, ops: &[&Operation]) -> bool {
        // Simplified linearizability check
        // In production, this would use a more sophisticated algorithm
        // like the one in Knossos or Wing & Gong's algorithm
        
        let mut state = serde_json::Value::Null;
        
        for op in ops {
            match (&op.op, &op.result) {
                (OperationType::Write { value, .. }, Some(OperationResult::WriteOk)) => {
                    state = value.clone();
                }
                (OperationType::Read { .. }, Some(OperationResult::ReadOk { value })) => {
                    if let Some(read_value) = value {
                        // Check if read value is consistent with current state
                        if *read_value != state && state != serde_json::Value::Null {
                            return false;
                        }
                    }
                }
                (OperationType::Cas { old, new, .. }, Some(OperationResult::CasOk { success })) => {
                    if *success {
                        if state == *old {
                            state = new.clone();
                        } else {
                            return false;
                        }
                    }
                }
                _ => {}
            }
        }
        
        true
    }
}
/// Serializability checker for multi-object transactions
pub struct SerializabilityChecker {
    /// Enable strict serializability checking
    strict: bool,
}

impl SerializabilityChecker {
    pub fn new() -> Self {
        Self { strict: false }
    }

    pub fn strict() -> Self {
        Self { strict: true }
    }
}

impl Checker for SerializabilityChecker {
    fn check(&self, history: &History) -> CheckerResult {
        let start_time = Instant::now();
        
        // Filter transaction operations
        let txn_ops: Vec<&Operation> = history.operations.iter()
            .filter(|op| matches!(op.op, OperationType::Transaction { .. }))
            .filter(|op| op.complete_time.is_some())
            .collect();

        let violations = self.find_serializability_violations(&txn_ops);
        
        CheckerResult {
            valid: violations.is_empty(),
            model: if self.strict {
                ConsistencyModel::StrictSerializability
            } else {
                ConsistencyModel::Serializability
            },
            violations,
            metadata: CheckerMetadata {
                check_duration: start_time.elapsed(),
                operations_analyzed: txn_ops.len(),
                stats: HashMap::new(),
            },
        }
    }

    fn name(&self) -> &str {
        if self.strict {
            "strict-serializability"
        } else {
            "serializability"
        }
    }

    fn consistency_model(&self) -> ConsistencyModel {
        if self.strict {
            ConsistencyModel::StrictSerializability
        } else {
            ConsistencyModel::Serializability
        }
    }
}

impl SerializabilityChecker {
    fn find_serializability_violations(&self, ops: &[&Operation]) -> Vec<Violation> {
        let mut violations = Vec::new();
        
        // Build dependency graph
        let graph = self.build_dependency_graph(ops);
        
        // Check for cycles (indicates serializability violation)
        if let Some(cycle) = self.find_cycle(&graph) {
            violations.push(Violation {
                violation_type: if self.strict {
                    ViolationType::StrictSerializabilityViolation
                } else {
                    ViolationType::SerializabilityViolation
                },
                operations: cycle,
                description: "Cycle detected in transaction dependency graph".to_string(),
                context: HashMap::new(),
            });
        }
        
        violations
    }

    fn build_dependency_graph(&self, ops: &[&Operation]) -> HashMap<uuid::Uuid, Vec<uuid::Uuid>> {
        let mut graph = HashMap::new();
        
        // Simplified dependency analysis
        // In production, this would implement proper read-write dependency tracking
        for (i, op1) in ops.iter().enumerate() {
            for op2 in ops.iter().skip(i + 1) {
                if self.has_dependency(op1, op2) {
                    graph.entry(op1.id).or_insert_with(Vec::new).push(op2.id);
                }
            }
        }
        
        graph
    }

    fn has_dependency(&self, op1: &Operation, op2: &Operation) -> bool {
        // Simplified dependency check
        // Real implementation would check read-write, write-read, write-write dependencies
        match (&op1.op, &op2.op) {
            (OperationType::Transaction { ops: ops1 }, OperationType::Transaction { ops: ops2 }) => {
                // Check if transactions access overlapping keys
                let keys1: HashSet<String> = ops1.iter().map(|op| match op {
                    super::TransactionOp::Read { key } => key.clone(),
                    super::TransactionOp::Write { key, .. } => key.clone(),
                }).collect();
                
                let keys2: HashSet<String> = ops2.iter().map(|op| match op {
                    super::TransactionOp::Read { key } => key.clone(),
                    super::TransactionOp::Write { key, .. } => key.clone(),
                }).collect();
                
                !keys1.is_disjoint(&keys2)
            }
            _ => false,
        }
    }

    fn find_cycle(&self, graph: &HashMap<uuid::Uuid, Vec<uuid::Uuid>>) -> Option<Vec<uuid::Uuid>> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();
        
        for &node in graph.keys() {
            if !visited.contains(&node) {
                if let Some(cycle) = self.dfs_cycle(node, graph, &mut visited, &mut rec_stack, &mut path) {
                    return Some(cycle);
                }
            }
        }
        
        None
    }

    fn dfs_cycle(
        &self,
        node: uuid::Uuid,
        graph: &HashMap<uuid::Uuid, Vec<uuid::Uuid>>,
        visited: &mut HashSet<uuid::Uuid>,
        rec_stack: &mut HashSet<uuid::Uuid>,
        path: &mut Vec<uuid::Uuid>,
    ) -> Option<Vec<uuid::Uuid>> {
        visited.insert(node);
        rec_stack.insert(node);
        path.push(node);
        
        if let Some(neighbors) = graph.get(&node) {
            for &neighbor in neighbors {
                if !visited.contains(&neighbor) {
                    if let Some(cycle) = self.dfs_cycle(neighbor, graph, visited, rec_stack, path) {
                        return Some(cycle);
                    }
                } else if rec_stack.contains(&neighbor) {
                    // Found cycle
                    let cycle_start = path.iter().position(|&x| x == neighbor).unwrap();
                    return Some(path[cycle_start..].to_vec());
                }
            }
        }
        
        path.pop();
        rec_stack.remove(&node);
        None
    }
}

/// Combined checker that validates multiple consistency models
pub struct CombinedChecker {
    checkers: Vec<Box<dyn Checker>>,
}

impl CombinedChecker {
    pub fn new() -> Self {
        Self {
            checkers: vec![
                Box::new(LinearizabilityChecker::new()),
                Box::new(SerializabilityChecker::new()),
            ],
        }
    }

    pub fn with_checkers(checkers: Vec<Box<dyn Checker>>) -> Self {
        Self { checkers }
    }
}

impl Checker for CombinedChecker {
    fn check(&self, history: &History) -> CheckerResult {
        let start_time = Instant::now();
        let mut all_violations = Vec::new();
        let mut total_ops = 0;
        
        for checker in &self.checkers {
            let result = checker.check(history);
            all_violations.extend(result.violations);
            total_ops += result.metadata.operations_analyzed;
        }
        
        CheckerResult {
            valid: all_violations.is_empty(),
            model: ConsistencyModel::StrictSerializability, // Most strict
            violations: all_violations,
            metadata: CheckerMetadata {
                check_duration: start_time.elapsed(),
                operations_analyzed: total_ops,
                stats: HashMap::new(),
            },
        }
    }

    fn name(&self) -> &str {
        "combined"
    }

    fn consistency_model(&self) -> ConsistencyModel {
        ConsistencyModel::StrictSerializability
    }
}

/// Create a checker for the specified consistency model
pub fn create_checker(model: ConsistencyModel) -> Arc<dyn Checker> {
    match model {
        ConsistencyModel::Linearizability => Arc::new(LinearizabilityChecker::new()),
        ConsistencyModel::Serializability => Arc::new(SerializabilityChecker::new()),
        ConsistencyModel::StrictSerializability => Arc::new(SerializabilityChecker::strict()),
        ConsistencyModel::SequentialConsistency => Arc::new(LinearizabilityChecker::new()), // Simplified
        ConsistencyModel::CausalConsistency => Arc::new(LinearizabilityChecker::new()), // Simplified
    }
}