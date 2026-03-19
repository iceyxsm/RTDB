import re

with open('src/jepsen/mod.rs', 'r') as f:
    content = f.read()

# I see that src/jepsen/nemesis.rs exists but isn't included in src/jepsen/mod.rs
# src/jepsen/checkers.rs exists but isn't included
# src/jepsen/history.rs exists but isn't included
# Let me add them to src/jepsen/mod.rs

with open('src/jepsen/mod.rs', 'r') as f:
    content = f.read()

# Add them after the existing pub mod lines
content = content.replace(
    'pub mod high_perf_store;',
    'pub mod high_perf_store;\npub mod nemesis;\npub mod checkers;\npub mod history;\npub mod workloads;\npub mod operations;\npub mod generators;'
)

with open('src/jepsen/mod.rs', 'w') as f:
    f.write(content)
