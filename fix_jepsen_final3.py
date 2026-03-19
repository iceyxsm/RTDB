import re

with open('tests/jepsen_tests.rs', 'r') as f:
    content = f.read()

# Fix RTDBError::Api
content = content.replace('rtdb::RTDBError::Api', 'rtdb::RTDBError::Internal')

# Fix nemesis module missing. The struct is in rtdb::jepsen::CombinedNemesis
content = content.replace('nemesis::CombinedNemesis', 'rtdb::jepsen::CombinedNemesis')

# Fix checkers module missing. It's rtdb::jepsen::create_checker
content = content.replace('rtdb::jepsen::checkers::create_checker', 'rtdb::jepsen::create_checker')

# Fix history::HistoryAnalyzer
content = content.replace('history::HistoryAnalyzer', 'rtdb::jepsen::HistoryAnalyzer')

# Fix result.is_valid() which was missed
content = content.replace('!result.is_valid()', '!result.checker_result.valid')
content = content.replace('result.is_valid()', 'result.checker_result.valid')

# For the comprehensive suite loop, make sure variables are in scope where needed
# Replace ops and violations back to the summary tuple components
content = content.replace(
    'println!("  ✓ Test completed: {} ops, {} violations", \n                        result.history.metadata.total_ops, result.checker_result.violations.len());\n                let summary = (result.history.metadata.total_ops, result.checker_result.violations.len());',
    'let summary_ops = result.history.metadata.total_ops;\n                let summary_violations = result.checker_result.violations.len();\n                println!("  ✓ Test completed: {} ops, {} violations", \n                        summary_ops, summary_violations);\n                let summary = (summary_ops, summary_violations);'
)

# the previous replacement did this:
# println!("  {:30} | Ops: {:5} | Violations: {:3} | {:?}",
#                  name, ops,
#                 0.0,
#                 violations,
#                 violations == 0);
# which breaks formatting. We can just redo this completely.
bad_loop_code = """    println!("\\n=== Jepsen Suite Results ===");
    let mut all_passed = true;
    for (name, summary) in results {
        let (ops, violations) = summary;
        println!("  {:30} | Ops: {:5} | Violations: {:3} | {:?}",
                 name, ops,
                0.0,
                violations,
                violations == 0);
        if violations > 0 {
            all_passed = false;
        }
    }"""
good_loop_code = """    println!("\\n=== Jepsen Suite Results ===");
    let mut all_passed = true;
    for (name, summary) in results {
        let (ops, violations) = summary;
        println!("  {:30} | Ops: {:5} | Violations: {:3} | {:?}",
                 name, ops, violations,
                 if violations == 0 { "PASS" } else { "FAIL" });
        if violations > 0 {
            all_passed = false;
        }
    }"""
content = content.replace(bad_loop_code, good_loop_code)


with open('tests/jepsen_tests.rs', 'w') as f:
    f.write(content)
