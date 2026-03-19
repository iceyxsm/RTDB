import re

with open('tests/jepsen_tests.rs', 'r') as f:
    content = f.read()

# Fix JepsenConfig initialization
# JepsenConfig has fields: client_count, test_duration_secs, operation_rate, partition_probability, enable_simdx, consistency_model, max_operation_latency_ms
content = re.sub(
    r'name: [^,]+,\s*node_count: [^,]+,\s*duration: (\d+),\s*rate: ([\d\.]+),\s*concurrency: (\d+),\s*nemesis: NemesisConfig \{[^}]+\},\s*workload: [^,]+,',
    r'client_count: \3,\n        test_duration_secs: \1,\n        operation_rate: \2 as u64,\n        partition_probability: 0.0,',
    content
)

# For the comprehensive suite which has a dynamic `with_faults` logic
content = re.sub(
    r'name: name\.to_string\(\),\s*node_count: if with_faults \{ 3 \} else \{ 1 \},\s*duration: (\d+),\s*rate: ([\d\.]+),\s*concurrency: (\d+),\s*nemesis: NemesisConfig \{[^}]+\},\s*workload,',
    r'client_count: \3,\n            test_duration_secs: \1,\n            operation_rate: \2 as u64,\n            partition_probability: if with_faults { 0.1 } else { 0.0 },',
    content
)

# Fix missing `checkers::create_checker` module. It is exported from `rtdb::jepsen::checkers` module, but the file just imports `rtdb::jepsen::*`.
content = content.replace('checkers::create_checker', 'rtdb::jepsen::checkers::create_checker')

# Remove `result.summary()` call and replace with getting total ops from metadata
content = content.replace(
    'let summary = result.summary();\n                println!("  ✓ Test completed: {} ops, {} violations", \n                        summary.total_operations, summary.consistency_violations);',
    'println!("  ✓ Test completed: {} ops, {} violations", \n                        result.history.metadata.total_ops, result.checker_result.violations.len());\n                let summary = (result.history.metadata.total_ops, result.checker_result.violations.len());'
)

# Fix tuple unpacking for the results loop
content = content.replace(
    'println!("\n=== Jepsen Suite Results ===");\n    let mut all_passed = true;\n    for (name, summary) in results {',
    'println!("\\n=== Jepsen Suite Results ===");\n    let mut all_passed = true;\n    for (name, summary) in results {\n        let (ops, violations) = summary;'
)

content = content.replace(
    'println!("  {:30} | Ops: {:5} | Violations: {:3} | {:?}", \n                 name, summary.total_operations, summary.consistency_violations, \n                 if summary.consistency_violations == 0 { "PASS" } else { "FAIL" });',
    'println!("  {:30} | Ops: {:5} | Violations: {:3} | {:?}", \n                 name, ops, violations, \n                 if violations == 0 { "PASS" } else { "FAIL" });'
)

content = content.replace(
    'if summary.consistency_violations > 0 {',
    'if violations > 0 {'
)

# Fix the test_configs tuple setup
content = content.replace(
    'let test_configs = vec![\n        ("register-linearizability", WorkloadType::Register, ConsistencyModel::Linearizability, false),\n        ("append-strict-serializability", WorkloadType::Append, ConsistencyModel::StrictSerializability, false),\n        ("set-serializability", WorkloadType::Set, ConsistencyModel::Serializability, false),\n        ("register-with-faults", WorkloadType::Register, ConsistencyModel::Linearizability, true),\n    ];',
    'let test_configs = vec![\n        ("register-linearizability", ConsistencyModel::Linearizability, false),\n        ("append-strict-serializability", ConsistencyModel::StrictSerializability, false),\n        ("set-serializability", ConsistencyModel::Serializability, false),\n        ("register-with-faults", ConsistencyModel::Linearizability, true),\n    ];'
)

content = content.replace(
    'for (name, workload, consistency, with_faults) in test_configs {',
    'for (name, consistency, with_faults) in test_configs {'
)

with open('tests/jepsen_tests.rs', 'w') as f:
    f.write(content)
