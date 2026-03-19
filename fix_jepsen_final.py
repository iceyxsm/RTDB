import re

with open('tests/jepsen_tests.rs', 'r') as f:
    content = f.read()

# Fix the remain config regexes
content = re.sub(
    r'name: "linearizability-register"\.to_string\(\),\s*node_count: (\d+),\s*duration: (\d+),\s*rate: ([\d\.]+),\s*concurrency: (\d+),\s*nemesis: NemesisConfig \{[^}]+\},\s*workload: [^,]+,',
    r'client_count: \4,\n        test_duration_secs: \2,\n        operation_rate: \3 as u64,\n        partition_probability: 0.0,',
    content
)

content = re.sub(
    r'name: "serializability-bank"\.to_string\(\),\s*node_count: (\d+),\s*duration: (\d+),\s*rate: ([\d\.]+),\s*concurrency: (\d+),\s*nemesis: NemesisConfig \{[^}]+\},\s*workload: [^,]+,',
    r'client_count: \4,\n        test_duration_secs: \2,\n        operation_rate: \3 as u64,\n        partition_probability: 0.0,',
    content
)

content = re.sub(
    r'name: "counter-workload"\.to_string\(\),\s*node_count: (\d+),\s*duration: (\d+),\s*rate: ([\d\.]+),\s*concurrency: (\d+),\s*nemesis: NemesisConfig \{[^}]+\},\s*workload: [^,]+,',
    r'client_count: \4,\n        test_duration_secs: \2,\n        operation_rate: \3 as u64,\n        partition_probability: 0.0,',
    content
)


# Re-do the comprehensive test config that was missed
content = re.sub(
    r'name: name\.to_string\(\),\s*node_count: if with_faults \{ 3 \} else \{ 1 \},\s*duration: (\d+),\s*rate: ([\d\.]+),\s*concurrency: (\d+),\s*nemesis: NemesisConfig \{[^}]+\},\s*workload,',
    r'client_count: \3,\n            test_duration_secs: \1,\n            operation_rate: \2 as u64,\n            partition_probability: if with_faults { 0.1 } else { 0.0 },',
    content
)

# And one last time for partition tolerance if it missed
content = re.sub(
    r'name: "partition-tolerance"\.to_string\(\),\s*node_count: (\d+),\s*duration: (\d+),\s*rate: ([\d\.]+),\s*concurrency: (\d+),\s*nemesis: NemesisConfig \{[^}]+\},\s*workload: [^,]+,',
    r'client_count: \4,\n        test_duration_secs: \2,\n        operation_rate: \3 as u64,\n        partition_probability: 0.1,',
    content
)


# Now fix the comprehensive suite result printing code
content = content.replace(
    'summary.total_operations,\n                (summary.successful_operations as f64 / summary.total_operations as f64) * 100.0,\n                summary.consistency_violations,\n                summary.is_valid',
    'ops,\n                0.0,\n                violations,\n                violations == 0'
)

with open('tests/jepsen_tests.rs', 'w') as f:
    f.write(content)
