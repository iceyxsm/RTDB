import re

with open('src/jepsen/mod.rs', 'r') as f:
    content = f.read()

# If the modules are not exposed, the tests cannot use them.
# The `nemesis.rs`, `checkers.rs`, `history.rs` modules are not public!
# Let's verify this by checking `mod nemesis;`
print(bool(re.search(r'mod nemesis;', content)))
print(bool(re.search(r'mod checkers;', content)))
print(bool(re.search(r'mod history;', content)))
