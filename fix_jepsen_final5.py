import re

with open('tests/jepsen_tests.rs', 'r') as f:
    content = f.read()

# I misread how these are exposed in rtdb::jepsen
# In src/jepsen/mod.rs they are probably exported like:
# pub use nemesis::CombinedNemesis;
# Let's search src/jepsen/mod.rs to see what's actually there. Wait, no I can just try `rtdb::jepsen::CombinedNemesis` but wait, earlier it said `not found in jepsen`.

# Let's grep mod.rs again to see what is exported.
