#!/usr/bin/env python3
"""Compare effective top-level $pref assignments (last assignment wins) between
the upstream stock v20 defaults.cs and the installed (B4v21-patched) copy.
Usage: defaults_compare.py stock.cs installed.cs out.tsv   (read-only)"""
import re, sys
A = re.compile(r'^(\$[\w:]+(?:\[[^\]]*\])?)\s*=\s*(.*?);\s*$')
def load(p):
    eff, lines = {}, {}
    depth = 0
    for n, l in enumerate(open(p, 'rb').read().decode('utf-8', 'replace').replace('\r', '').split('\n'), 1):
        s = l.strip()
        if depth == 0:
            m = A.match(s)
            if m:
                k = m.group(1).lower()
                eff[k] = (m.group(1), m.group(2)); lines.setdefault(k, []).append(n)
        depth += s.count('{') - s.count('}')
    return eff, lines
s, sl = load(sys.argv[1]); i, il = load(sys.argv[2])
with open(sys.argv[3], 'w') as o:
    o.write('pref\tstock_value\tstock_lines\tinstalled_value\tinstalled_lines\tdiffers\n')
    for k in sorted(set(s) | set(i)):
        sv = s.get(k, ('', ''))[1]; iv = i.get(k, ('', ''))[1]
        name = (s.get(k) or i.get(k))[0]
        norm = lambda v: v.strip('"').rstrip('0').rstrip('.') if re.match(r'^"?-?[\d.]+"?$', v) else v
        o.write(f"{name}\t{sv}\t{','.join(map(str, sl.get(k, [])))}\t{iv}\t{','.join(map(str, il.get(k, [])))}\t{'YES' if norm(sv) != norm(iv) else ''}\n")
