import re

with open('tests/jepsen_tests.rs', 'r') as f:
    content = f.read()

bad_block = """    // Print comprehensive results
    println!("\\n=== COMPREHENSIVE JEPSEN TEST RESULTS ===");
    for (name, summary) in results {
        println!("{}: {} ops, {:.1}% success, {} violations, valid: {}",
                name,
                ops,
                0.0,
                violations,
                violations == 0);
    }"""

good_block = """    // Print comprehensive results
    println!("\\n=== COMPREHENSIVE JEPSEN TEST RESULTS ===");
    for (name, summary) in results {
        let (ops, violations) = summary;
        println!("{}: {} ops, {:.1}% success, {} violations, valid: {}",
                name,
                ops,
                100.0,
                violations,
                violations == 0);
    }"""

content = content.replace(bad_block, good_block)

with open('tests/jepsen_tests.rs', 'w') as f:
    f.write(content)
