import re

with open('tests/jepsen_tests.rs', 'r') as f:
    content = f.read()

# Fix rtdb::jepsen::CombinedNemesis visibility. It's actually not exported through mod.rs
# Let's replace it with an empty nemesis or something that implements Nemesis since nemesis module is not exported directly.
# Wait, let's look at mod.rs: `pub mod nemesis;` or similar? Let's assume `rtdb::jepsen::nemesis::CombinedNemesis` might work if nemesis is pub.
content = content.replace('rtdb::jepsen::CombinedNemesis', 'rtdb::jepsen::nemesis::CombinedNemesis')
content = content.replace('rtdb::jepsen::create_checker', 'rtdb::jepsen::checkers::create_checker')

# Also fix the `history::HistoryAnalyzer` -> `rtdb::jepsen::history::HistoryAnalyzer`
content = content.replace('rtdb::jepsen::HistoryAnalyzer', 'rtdb::jepsen::history::HistoryAnalyzer')

# Fix the missing `ops` and `violations` in the comprehensive loop which somehow got messed up again
bad_loop_code2 = """    println!("\\n=== Jepsen Suite Results ===");
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
good_loop_code2 = """    println!("\\n=== Jepsen Suite Results ===");
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
content = content.replace(bad_loop_code2, good_loop_code2)

# One more try for formatting that might have got replaced
bad_loop_code3 = """        let (ops, violations) = summary;
        println!("  {:30} | Ops: {:5} | Violations: {:3} | {:?}",
                 name, ops, violations,
                 if violations == 0 { "PASS" } else { "FAIL" });
        if violations > 0 {"""
# If the previous search replace missed because of line breaks
content = re.sub(
    r'println!\("  \{:30\} \| Ops: \{:5\} \| Violations: \{:3\} \| \{\:\?\}",\s*name,\s*ops,\s*0\.0,\s*violations,\s*violations == 0\);',
    r'println!("  {:30} | Ops: {:5} | Violations: {:3} | {:?}", name, ops, violations, if violations == 0 { "PASS" } else { "FAIL" });',
    content
)

with open('tests/jepsen_tests.rs', 'w') as f:
    f.write(content)
