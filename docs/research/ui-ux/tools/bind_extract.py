#!/usr/bin/env python3
"""Extract ActionMap binds with their enclosing if/else conditions and lines.
Usage: bind_extract.py <script> <start_line> <end_line>
Prints TSV: line, map, device, key, command, conditions. Read-only."""
import re, sys
p, a, b = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
lines = open(p, 'rb').read().decode('utf-8', 'replace').replace('\r', '').split('\n')
BIND = re.compile(r'(\w+)\.(bind|bindCmd)\(\s*([^,]+),\s*("?[^",]*"?)\s*,\s*(.*)\);')
IF = re.compile(r'^(else\s+)?if\s*\((.*)\)$')
stack = []            # active block conditions (None for plain blocks)
chain = {}            # depth -> list of prior raw conditions in current if/else chain
pending = None
print('line\tmap\tdevice\tkey\tcommand\tconditions')
for n in range(a, b + 1):
    s = lines[n - 1].strip()
    d = len(stack)
    m = IF.match(s)
    if m:
        prior = chain.get(d, []) if m.group(1) else []
        neg = ' && '.join(f'!({c})' for c in prior)
        pending = (neg + ' && ' if neg else '') + m.group(2)
        chain[d] = prior + [m.group(2)]
        continue
    if s == 'else':
        pending = ' && '.join(f'!({c})' for c in chain.get(d, []))
        chain[d] = []
        continue
    if s == '{':
        stack.append(pending); pending = None; chain[d + 1] = []; continue
    if s == '}':
        if stack: stack.pop()
        continue
    if s and not s.startswith('else'):
        # a normal statement at depth d ends any if/else chain at that depth
        if not BIND.search(s) or pending is None:
            pass
    bm = BIND.search(s)
    if bm:
        conds = [c for c in stack if c] + ([pending] if pending else [])
        pending = None
        print(f'{n}\t{bm.group(1)}\t{bm.group(3).strip()}\t{bm.group(4).strip()}\t{bm.group(5).strip()}\t{" && ".join(conds) or "always"}')
