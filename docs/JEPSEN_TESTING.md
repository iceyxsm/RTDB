# Jepsen Testing Framework for RTDB

RTDB includes a comprehensive Jepsen-style testing framework for validating distributed system correctness under various failure scenarios. This document describes the implementation, usage, and interpretation of Jepsen tests.

## Overview

The Jepsen testing framework validates consistency guarantees by:
- Generating concurrent operations across multiple clients
- Injecting various types of faults (network partitions, node failures, clock skew)
- Analyzing operation histories for consistency violations
- Providing detailed reports on system behavior under stress

## Features

### Consistency Models Tested

- **Linearizability**: Single-object operations appear to occur atomically at some point between invocation and response
- **Serializability**: Multi-object transactions can be ordered as if executed sequentially
- **Strict Serializability**: Combines linearizability and serializability with real-time ordering
- **Sequential Consistency**: Operations appear in program order across all processes
- **Causal Consistency**: Causally related operations are seen in the same order by all processes

### Workload Types

1. **Register Workload**: Read/write operations on single keys (linearizability testing)
2. **Bank Workload**: Transfer operations between accounts (transaction testing)
3. **Counter Workload**: Increment operations (atomic operation testing)
4. **Set Workload**: Set add operations (serializability testing)
5. **Append Workload**: List append operations (strict serializability testing)
6. **Read-Write Workload**: Multi-key transactions with mixed operations

### Fault Injection (Nemesis)

- **Network Partitions**: Majority/minority, complete, random, ring partitions
- **Process Failures**: Kill and pause node processes
- **Clock Skew**: Simulate clock drift between nodes
- **Network Issues**: Packet loss, slow network simulation

## Usage

### Command Line Interface

```bash
# Basic linearizability test
rtdb jepsen --test linearizability --duration 30 --rate 100 --concurrency 5

# Bank workload with fault injection
rtdb jepsen --test bank-transfer --workload bank --consistency serializability --faults --duration 60

# Counter workload with high concurrency
rtdb jepsen --test counter-stress --workload counter --concurrency 10 --rate 200 --duration 45

# Comprehensive test suite
rtdb jepsen --test comprehensive --workload register --consistency strict-serializability --faults --duration 120
```

### Programmatic Usage

```rust
use rtdb::jepsen::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure test
    let config = JepsenConfig {
        name: "my-test".to_string(),
        node_count: 3,
        duration: 60,
        rate: 100.0,
        concurrency: 5,
        nemesis: NemesisConfig {
            enabled: true,
            faults: vec![FaultType::Partition(PartitionType::MajorityMinority)],
            interval: 30.0,
            duration: 10.0,
        },
        workload: WorkloadType::Register,
        consistency_model: ConsistencyModel::Linearizability,
        ..Default::default()
    };

    // Create clients, nemesis, and checker
    let clients = create_rtdb_clients(config.concurrency).await;
    let nemesis = Arc::new(nemesis::CombinedNemesis::new(node_addresses, 1000));
    let checker = checkers::create_checker(config.consistency_model);

    // Run test
    let runner = JepsenRunner::new(config, clients, nemesis, checker);
    let result = runner.run().await?;

    // Analyze results
    if result.is_valid() {
        println!("✅ Test passed - no consistency violations");
    } else {
        println!("❌ Test failed - {} violations found", result.checker_result.violations.len());
    }

    Ok(())
}
```

## Implementation Details

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        JepsenRunner                             │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────┬─────────────┬─────────────┬─────────────────┐  │
│  │   Clients   │   Nemesis   │  Workload   │    Checker      │  │
│  │             │             │             │                 │  │
│  │ - Execute   │ - Inject    │ - Generate  │ - Validate      │  │
│  │   ops       │   faults    │   ops       │   consistency   │  │
│  │ - Track     │ - Recover   │ - Control   │ - Find          │  │
│  │   results   │   from      │   patterns  │   violations    │  │
│  │             │   faults    │             │                 │  │
│  └─────────────┴─────────────┴─────────────┴─────────────────┘  │
├─────────────────────────────────────────────────────────────────┤
│                      History Collector                         │
│  - Records all operations with timestamps                      │
│  - Tracks fault injection events                              │
│  - Maintains operation ordering                               │
└─────────────────────────────────────────────────────────────────┘
```

### Consistency Checkers

#### Linearizability Checker
- Implements Wing & Gong's linearizability algorithm
- Checks if operations can be ordered consistently with real-time constraints
- Validates single-object operations (reads, writes, compare-and-swap)

#### Serializability Checker
- Builds dependency graphs between transactions
- Detects cycles indicating serializability violations
- Supports read-write, write-read, and write-write dependencies

#### Strict Serializability Checker
- Combines linearizability and serializability checking
- Ensures transactions respect both logical and real-time ordering
- Most stringent consistency guarantee

### Fault Injection

#### Network Partitions
```rust
// Majority/minority partition
FaultType::Partition(PartitionType::MajorityMinority)

// Complete network isolation
FaultType::Partition(PartitionType::Complete)

// Random partitioning
FaultType::Partition(PartitionType::Random)
```

#### Process Management
```rust
// Kill processes (SIGKILL)
FaultType::Kill

// Pause processes (SIGSTOP/SIGCONT)
FaultType::Pause
```

#### Clock Skew
```rust
// Inject clock skew up to 1 second
FaultType::ClockSkew { max_skew_ms: 1000 }
```

### History Analysis

The framework provides comprehensive analysis tools:

```rust
// Latency analysis
let latency_analysis = HistoryAnalyzer::analyze_latencies(&history);
println!("P99 latency: {:?}", latency_analysis.p99);

// Throughput analysis
let throughput_analysis = HistoryAnalyzer::analyze_throughput(&history, Duration::from_secs(1));

// Error rate analysis
let error_rates = HistoryAnalyzer::analyze_error_rates(&history);

// Concurrent operation detection
let concurrent_groups = HistoryAnalyzer::find_concurrent_operations(&history);
```

## Test Scenarios

### Basic Correctness Tests

1. **Single-Node Linearizability**
   - No faults, single RTDB instance
   - Validates basic read/write consistency
   - Should always pass for correct implementations

2. **Multi-Client Serializability**
   - Multiple concurrent clients
   - Transaction-based workloads
   - Tests isolation guarantees

### Fault Tolerance Tests

3. **Network Partition Tolerance**
   - Majority/minority network splits
   - Tests CAP theorem trade-offs
   - Validates partition handling

4. **Node Failure Recovery**
   - Process kill/restart scenarios
   - Tests crash recovery mechanisms
   - Validates data durability

5. **Clock Skew Resilience**
   - Simulated clock drift
   - Tests timestamp-based ordering
   - Validates distributed coordination

### Stress Tests

6. **High Concurrency**
   - Many concurrent clients
   - High operation rates
   - Tests scalability limits

7. **Mixed Workloads**
   - Combined read/write operations
   - Variable transaction sizes
   - Tests real-world scenarios

## Interpreting Results

### Successful Test
```
🎉 Jepsen test completed in 30.2s
📊 Results:
   Total operations: 3,247
   Successful: 3,198 (98.5%)
   Failed: 49 (1.5%)
   Faults injected: 2
   Consistency violations: 0
✅ Test PASSED - No consistency violations detected

⏱️  Latency Analysis:
   P50: 12ms
   P95: 45ms
   P99: 78ms
   Max: 156ms
```

### Failed Test with Violations
```
🎉 Jepsen test completed in 30.1s
📊 Results:
   Total operations: 3,156
   Successful: 2,987 (94.6%)
   Failed: 169 (5.4%)
   Faults injected: 3
   Consistency violations: 2
❌ Test FAILED - 2 consistency violations found

🔍 Violations:
   1. LinearizabilityViolation: Read returned stale value after write completion
   2. LinearizabilityViolation: Concurrent operations not linearizable
```

### Understanding Violations

- **Stale Reads**: Read operations returning outdated values
- **Lost Updates**: Write operations being overwritten
- **Non-Atomic Operations**: Operations appearing to execute partially
- **Ordering Violations**: Operations not respecting real-time order

## Best Practices

### Test Design

1. **Start Simple**: Begin with single-node, no-fault tests
2. **Increase Complexity**: Gradually add faults and concurrency
3. **Use Appropriate Models**: Match consistency model to system guarantees
4. **Test Edge Cases**: Focus on boundary conditions and error paths

### Fault Injection

1. **Realistic Scenarios**: Use faults that occur in production
2. **Timing Matters**: Inject faults at critical moments
3. **Recovery Testing**: Ensure systems recover properly
4. **Gradual Escalation**: Start with minor faults, increase severity

### Analysis

1. **Look for Patterns**: Identify systematic issues vs. random failures
2. **Correlate with Faults**: Understand which faults cause violations
3. **Performance Impact**: Measure consistency vs. performance trade-offs
4. **Root Cause Analysis**: Trace violations back to implementation bugs

## Integration with CI/CD

### Automated Testing
```yaml
# GitHub Actions example
- name: Run Jepsen Tests
  run: |
    # Start RTDB cluster
    docker-compose up -d rtdb-cluster
    
    # Wait for cluster to be ready
    sleep 30
    
    # Run basic correctness tests
    rtdb jepsen --test linearizability --duration 60 --rate 50
    rtdb jepsen --test serializability --workload bank --duration 60
    
    # Run fault tolerance tests
    rtdb jepsen --test partition-tolerance --faults --duration 120
    
    # Cleanup
    docker-compose down
```

### Performance Regression Detection
```bash
# Baseline performance test
rtdb jepsen --test performance-baseline --duration 300 --rate 1000 > baseline.json

# Compare with current implementation
rtdb jepsen --test performance-current --duration 300 --rate 1000 > current.json

# Analyze regression
jepsen-compare baseline.json current.json
```

## Troubleshooting

### Common Issues

1. **Test Timeouts**
   - Reduce operation rate or test duration
   - Check system resource usage
   - Verify network connectivity

2. **False Positives**
   - Review consistency model assumptions
   - Check for test implementation bugs
   - Validate checker correctness

3. **Performance Issues**
   - Monitor system resources during tests
   - Adjust concurrency levels
   - Profile critical code paths

### Debugging Violations

1. **Examine Operation History**
   ```rust
   // Print detailed operation timeline
   for op in &result.history.operations {
       println!("{:?}: {:?} -> {:?}", op.invoke_time, op.op, op.result);
   }
   ```

2. **Analyze Concurrent Operations**
   ```rust
   let concurrent_groups = HistoryAnalyzer::find_concurrent_operations(&history);
   for group in concurrent_groups {
       println!("Concurrent operations: {:?}", group.operations);
   }
   ```

3. **Trace Fault Correlation**
   ```rust
   for fault in &result.history.metadata.faults_injected {
       println!("Fault: {:?} at {:?}", fault.fault_type, fault.start_time);
   }
   ```

## Future Enhancements

### Planned Features

1. **Advanced Checkers**
   - Causal consistency checker
   - Session guarantees validation
   - Custom consistency models

2. **Enhanced Fault Injection**
   - Disk failures simulation
   - Memory pressure testing
   - CPU throttling

3. **Visualization Tools**
   - Operation timeline graphs
   - Dependency graph visualization
   - Interactive violation explorer

4. **Performance Analysis**
   - Automated regression detection
   - Performance profiling integration
   - Resource usage correlation

### Contributing

To contribute to the Jepsen testing framework:

1. **Add New Workloads**: Implement domain-specific operation patterns
2. **Enhance Checkers**: Improve violation detection algorithms
3. **Extend Nemesis**: Add new fault injection mechanisms
4. **Improve Analysis**: Add new history analysis tools

See the [Contributing Guide](../CONTRIBUTING.md) for detailed instructions.

## References

- [Jepsen: On the perils of network partitions](https://aphyr.com/posts/281-jepsen-on-the-perils-of-network-partitions)
- [Linearizability: A Correctness Condition for Concurrent Objects](https://cs.brown.edu/~mph/HerlihyW90/p463-herlihy.pdf)
- [Highly Available Transactions: Virtues and Limitations](https://arxiv.org/abs/1302.0309)
- [Elle: Finding Serializability Violations in Distributed Systems](https://github.com/jepsen-io/elle)