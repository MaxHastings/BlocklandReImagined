#!/usr/bin/env python3
"""Print every fn in crates/client/src/app/*.rs over N lines (default 150)."""
import glob, re, sys
limit = int(sys.argv[1]) if len(sys.argv) > 1 else 150
for path in sorted(glob.glob("crates/client/src/app/*.rs")):
    lines = open(path).read().split("\n")
    for i, l in enumerate(lines):
        m = re.match(r"^(\s*)(pub(\([a-z]+\))? )?fn (\w+)", l)
        if not m:
            continue
        ind = m.group(1)
        for j in range(i + 1, len(lines)):
            if lines[j] == ind + "}":
                break
        if j - i > limit:
            print(f"{j - i:5} {path}:{i + 1} {m.group(4)}")
