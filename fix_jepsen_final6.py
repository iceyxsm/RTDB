import re

with open('tests/jepsen_tests.rs', 'r') as f:
    content = f.read()

# I see it missed some configs. Let's find exactly the line printing the summary and fix it properly.
lines = content.split('\n')
for i, line in enumerate(lines):
    if 'println!("  {:30} | Ops: {:5} | Violations: {:3} | {:?}", ' in line:
        print(f"Line {i}: {line}")
        print(f"Line {i+1}: {lines[i+1]}")
        print(f"Line {i+2}: {lines[i+2]}")
        print(f"Line {i+3}: {lines[i+3]}")
        print(f"Line {i+4}: {lines[i+4]}")
