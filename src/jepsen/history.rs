//! History analysis and utilities for Jepsen tests

use super::{History, OperationType};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// Analyzer for extracting performance and consistency insights from test execution history.
/// 
/// Provides methods to analyze operation latencies, throughput patterns, concurrent operations,
/// and process sequences from Jepsen test execution histories.
pub struct HistoryAnalyzer;

impl HistoryAnalyzer {
    /// Analyze operation latencies
    pub fn analyze_latencies(history: &History) -> LatencyAnalysis {
        let mut latencies = Vec::new();
        
        for op in &history.operations {
            if let (Some(complete_time), invoke_time) = (op.complete_time, op.invoke_time) {
                if let Ok(duration) = complete_time.duration_since(invoke_time) {
                    latencies.push(duration);
                }
            }
        }
        
        latencies.sort();
        
        let count = latencies.len();
        if count == 0 {
            return LatencyAnalysis::default();
        }
        
        let min = latencies[0];
        let max = latencies[count - 1];
        let median = latencies[count / 2];
        let p95 = latencies[(count as f64 * 0.95) as usize];
        let p99 = latencies[(count as f64 * 0.99) as usize];
        
        let sum: Duration = latencies.iter().sum();
        let mean = sum / count as u32;
        
        LatencyAnalysis {
            count,
            min,
            max,
            mean,
            median,
            p95,
            p99,
        }
    }

    /// Analyze throughput over time
    pub fn analyze_throughput(history: &History, window_size: Duration) -> ThroughputAnalysis {
        let mut windows = Vec::new();
        
        if history.operations.is_empty() {
            return ThroughputAnalysis { windows };
        }
        
        let start_time = history.metadata.start_time;
        let end_time = history.metadata.end_time;
        
        let mut current_window_start = start_time;
        
        while current_window_start < end_time {
            let window_end = current_window_start + window_size;
            
            let ops_in_window = history.operations.iter()
                .filter(|op| {
                    op.invoke_time >= current_window_start && op.invoke_time < window_end
                })
                .count();
            
            let successful_ops = history.operations.iter()
                .filter(|op| {
                    op.invoke_time >= current_window_start && 
                    op.invoke_time < window_end &&
                    op.result.is_some()
                })
                .count();
            
            windows.push(ThroughputWindow {
                start_time: current_window_start,
                end_time: window_end,
                total_ops: ops_in_window,
                successful_ops,
                ops_per_second: ops_in_window as f64 / window_size.as_secs_f64(),
            });
            
            current_window_start = window_end;
        }
        
        ThroughputAnalysis { windows }
    }

    /// Find concurrent operations (overlapping in time)
    pub fn find_concurrent_operations(history: &History) -> Vec<ConcurrentGroup> {
        let mut concurrent_groups = Vec::new();
        let mut operations = history.operations.clone();
        
        // Sort by invoke time
        operations.sort_by_key(|op| op.invoke_time);
        
        let mut current_group = Vec::new();
        let mut group_end_time = SystemTime::UNIX_EPOCH;
        
        for op in operations {
            if let Some(complete_time) = op.complete_time {
                if current_group.is_empty() || op.invoke_time <= group_end_time {
                    // Operation overlaps with current group
                    current_group.push(op.id);
                    group_end_time = group_end_time.max(complete_time);
                } else {
                    // Start new group
                    if current_group.len() > 1 {
                        concurrent_groups.push(ConcurrentGroup {
                            operations: current_group.clone(),
                            start_time: history.operations.iter()
                                .find(|o| o.id == current_group[0])
                                .unwrap()
                                .invoke_time,
                            end_time: group_end_time,
                        });
                    }
                    
                    current_group = vec![op.id];
                    group_end_time = complete_time;
                }
            }
        }
        
        // Add final group if it has multiple operations
        if current_group.len() > 1 {
            concurrent_groups.push(ConcurrentGroup {
                operations: current_group,
                start_time: SystemTime::UNIX_EPOCH, // Would need to look up actual time
                end_time: group_end_time,
            });
        }
        
        concurrent_groups
    }

    /// Extract operation sequences per process
    pub fn extract_process_sequences(history: &History) -> HashMap<usize, Vec<uuid::Uuid>> {
        let mut sequences = HashMap::new();
        
        for op in &history.operations {
            sequences.entry(op.process).or_insert_with(Vec::new).push(op.id);
        }
        
        // Sort each sequence by invoke time
        for (_process_id, sequence) in sequences.iter_mut() {
            sequence.sort_by_key(|&op_id| {
                history.operations.iter()
                    .find(|op| op.id == op_id)
                    .map(|op| op.invoke_time)
                    .unwrap_or(SystemTime::UNIX_EPOCH)
            });
        }
        
        sequences
    }

    /// Calculate error rates by operation type
    pub fn analyze_error_rates(history: &History) -> HashMap<String, ErrorRate> {
        let mut error_rates = HashMap::new();
        
        for op in &history.operations {
            let op_type = match &op.op {
                OperationType::Read { .. } => "read",
                OperationType::Write { .. } => "write",
                OperationType::Cas { .. } => "cas",
                OperationType::Transaction { .. } => "transaction",
                OperationType::Append { .. } => "append",
                OperationType::SetAdd { .. } => "set_add",
                OperationType::Increment { .. } => "increment",
            };
            
            let entry = error_rates.entry(op_type.to_string()).or_insert(ErrorRate::default());
            entry.total += 1;
            
            if op.error.is_some() {
                entry.errors += 1;
            } else if op.result.is_some() {
                entry.successes += 1;
            }
        }
        
        // Calculate rates
        for error_rate in error_rates.values_mut() {
            if error_rate.total > 0 {
                error_rate.error_rate = error_rate.errors as f64 / error_rate.total as f64;
                error_rate.success_rate = error_rate.successes as f64 / error_rate.total as f64;
            }
        }
        
        error_rates
    }
}

/// Latency analysis results
#[derive(Debug, Clone, Default)]
pub struct LatencyAnalysis {
    /// Number of operations analyzed
    pub count: usize,
    /// Minimum latency observed
    pub min: Duration,
    /// Maximum latency observed
    pub max: Duration,
    /// Mean latency
    pub mean: Duration,
    /// Median latency
    pub median: Duration,
    /// 95th percentile latency
    pub p95: Duration,
    /// 99th percentile latency
    pub p99: Duration,
}

/// Throughput analysis results
#[derive(Debug, Clone)]
pub struct ThroughputAnalysis {
    /// Throughput measurements over time windows
    pub windows: Vec<ThroughputWindow>,
}

/// Throughput window
#[derive(Debug, Clone)]
pub struct ThroughputWindow {
    /// Window start time
    pub start_time: SystemTime,
    /// Window end time
    pub end_time: SystemTime,
    /// Total operations in window
    pub total_ops: usize,
    /// Successful operations in window
    pub successful_ops: usize,
    /// Operations per second in window
    pub ops_per_second: f64,
}

/// Group of concurrent operations
#[derive(Debug, Clone)]
pub struct ConcurrentGroup {
    /// Operation IDs in this concurrent group
    pub operations: Vec<uuid::Uuid>,
    /// Group start time
    pub start_time: SystemTime,
    /// Group end time
    pub end_time: SystemTime,
}

/// Error rate statistics
#[derive(Debug, Clone, Default)]
pub struct ErrorRate {
    /// Total operations
    pub total: usize,
    /// Successful operations
    pub successes: usize,
    /// Failed operations
    pub errors: usize,
    /// Success rate (0.0 to 1.0)
    pub success_rate: f64,
    /// Error rate (0.0 to 1.0)
    pub error_rate: f64,
}

/// History filtering utilities
pub mod filters {
    use super::super::{History, Operation, OperationType};
    use std::time::SystemTime;

    /// Filter operations by type
    pub fn by_operation_type<'a>(history: &'a History, op_type: &str) -> Vec<&'a Operation> {
        history.operations.iter()
            .filter(|op| match (&op.op, op_type) {
                (OperationType::Read { .. }, "read") => true,
                (OperationType::Write { .. }, "write") => true,
                (OperationType::Cas { .. }, "cas") => true,
                (OperationType::Transaction { .. }, "transaction") => true,
                (OperationType::Append { .. }, "append") => true,
                (OperationType::SetAdd { .. }, "set_add") => true,
                (OperationType::Increment { .. }, "increment") => true,
                _ => false,
            })
            .collect()
    }

    /// Filter operations by time range
    pub fn by_time_range(
        history: &History, 
        start: SystemTime, 
        end: SystemTime
    ) -> Vec<&Operation> {
        history.operations.iter()
            .filter(|op| op.invoke_time >= start && op.invoke_time <= end)
            .collect()
    }

    /// Filter operations by process
    pub fn by_process(history: &History, process_id: usize) -> Vec<&Operation> {
        history.operations.iter()
            .filter(|op| op.process == process_id)
            .collect()
    }

    /// Filter successful operations only
    pub fn successful_only(history: &History) -> Vec<&Operation> {
        history.operations.iter()
            .filter(|op| op.result.is_some())
            .collect()
    }

    /// Filter failed operations only
    pub fn failed_only(history: &History) -> Vec<&Operation> {
        history.operations.iter()
            .filter(|op| op.error.is_some())
            .collect()
    }

    /// Filter operations by key
    pub fn by_key<'a>(history: &'a History, key: &str) -> Vec<&'a Operation> {
        history.operations.iter()
            .filter(|op| {
                match &op.op {
                    OperationType::Read { key: k } => k == key,
                    OperationType::Write { key: k, .. } => k == key,
                    OperationType::Cas { key: k, .. } => k == key,
                    OperationType::Append { key: k, .. } => k == key,
                    OperationType::SetAdd { key: k, .. } => k == key,
                    OperationType::Increment { key: k, .. } => k == key,
                    OperationType::Transaction { ops } => {
                        ops.iter().any(|txn_op| match txn_op {
                            super::super::TransactionOp::Read { key: k } => k == key,
                            super::super::TransactionOp::Write { key: k, .. } => k == key,
                        })
                    }
                }
            })
            .collect()
    }
}