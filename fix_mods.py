import re

with open('src/jepsen/mod.rs', 'r') as f:
    content = f.read()

# I need to find where the nemesis module is defined or included
# Maybe it's included in another file?
