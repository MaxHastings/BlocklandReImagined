#!/usr/bin/env python3
"""Index TorqueScript function definitions (name, line span) in decompiled files.
Usage: function_index.py out.tsv file...   Read-only research tool."""
import re, sys, os
FN = re.compile(r'^\s*function\s+([A-Za-z_][\w:]*)\s*\(([^)]*)\)')
out = open(sys.argv[1], 'w')
out.write('file\tline\tend\tfunction\targs\n')
for p in sys.argv[2:]:
    lines = open(p, 'rb').read().decode('utf-8', 'replace').replace('\r', '').split('\n')
    i = 0
    while i < len(lines):
        m = FN.match(lines[i])
        if m:
            depth, j, opened = 0, i, False
            while j < len(lines):
                for ch in lines[j]:
                    if ch == '{': depth += 1; opened = True
                    elif ch == '}': depth -= 1
                if opened and depth <= 0: break
                j += 1
            out.write(f'{p}\t{i+1}\t{j+1}\t{m.group(1)}\t{m.group(2).strip()}\n')
        i += 1
