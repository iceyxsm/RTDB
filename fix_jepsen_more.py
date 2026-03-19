import re

with open('tests/jepsen_tests.rs', 'r') as f:
    content = f.read()

# Fix result.is_valid() call
content = content.replace('!result.is_valid()', '!result.checker_result.valid')

# Fix violation_type formatting issue (derive Display is missing, use Debug)
content = content.replace('violation.violation_type', 'format!("{:?}", violation.violation_type)')

# Fix more Config initializations that were not caught
content = re.sub(
    r'name: "partition-tolerance"\.to_string\(\),\s*node_count: (\d+),\s*duration: (\d+),\s*rate: ([\d\.]+),\s*concurrency: (\d+),\s*nemesis: NemesisConfig \{[^}]+\},\s*workload: [^,]+,',
    r'client_count: \4,\n        test_duration_secs: \2,\n        operation_rate: \3 as u64,\n        partition_probability: 0.1,',
    content
)

content = re.sub(
    r'name: name\.to_string\(\),\s*node_count: if with_faults \{ 3 \} else \{ 1 \},\s*duration: (\d+),\s*rate: ([\d\.]+),\s*concurrency: (\d+),\s*nemesis: NemesisConfig \{[^}]+\},\s*workload,',
    r'client_count: \3,\n            test_duration_secs: \1,\n            operation_rate: \2 as u64,\n            partition_probability: if with_faults { 0.1 } else { 0.0 },',
    content
)

# Replace the summary print logic that used summary struct
content = content.replace(
    'println!("    Total Operations: {}", summary.total_operations);',
    '// Removed'
)
content = content.replace(
    'println!("    Success Rate: {:.2}%", \n                 (summary.successful_operations as f64 / summary.total_operations as f64) * 100.0);',
    '// Removed'
)
content = content.replace(
    'println!("    Violations: {}", summary.consistency_violations);',
    '// Removed'
)
content = content.replace(
    'println!("    Status: {}", if summary.is_valid { "PASS" } else { "FAIL" });',
    '// Removed'
)
content = content.replace(
    'if !summary.is_valid {',
    'if violations > 0 {'
)

content = content.replace('println!("    Total Operations: {}", result.history.metadata.total_ops);\n                println!("    Success Rate: {:.2}%", \n                 (result.history.metadata.successful_ops as f64 / result.history.metadata.total_ops as f64) * 100.0);\n                println!("    Violations: {}", result.checker_result.violations.len());\n                println!("    Status: {}", if result.checker_result.valid { "PASS" } else { "FAIL" });\n                if !result.checker_result.valid {\n                    all_passed = false;\n                }',
'''println!("    Total Operations: {}", ops);
                println!("    Violations: {}", violations);
                println!("    Status: {}", if violations == 0 { "PASS" } else { "FAIL" });
                if violations > 0 {
                    all_passed = false;
                }'''
)

with open('tests/jepsen_tests.rs', 'w') as f:
    f.write(content)
