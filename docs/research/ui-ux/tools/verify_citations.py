#!/usr/bin/env python3
"""Check citations in the UI/UX audit documents.

- `key:N` or `key:N-M` / `key:N–M`: file exists and lines are in range.
- `` `ident`@key:N ``: ident (backtick text) occurs within +-3 lines of N.
Keys: c g s ms d (see README.md). Prints a report; exits 1 on failures.
Usage: verify_citations.py <docs_dir>   (run from the repo root)"""
import re, sys, os, glob
KEYS = {
    'c': '.research/v20-dso/client/scripts/allClientScripts-Vanilla.cs',
    'g': '.research/v20-dso/client/ui/allClientGuis-Vanilla.gui',
    's': '.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs',
    'ms': '.research/v20-dso/server/mainServer.cs',
    'd': '.research/bl-decompiled/v20/client/defaults.cs',
}
LINES = {k: open(v, 'rb').read().decode('utf-8', 'replace').replace('\r', '').split('\n') for k, v in KEYS.items()}
REF = re.compile(r'(?<![\w/.])(c|g|s|ms|d):(\d+)(?:\s*[-–]\s*(\d+))?')
CLAIM = re.compile(r'`([^`]+)`@(c|g|s|ms|d):(\d+)')
fails, n_ref, n_claim = [], 0, 0
for path in sorted(glob.glob(os.path.join(sys.argv[1], '*.md'))):
    text = open(path, encoding='utf-8').read()
    for ln, line in enumerate(text.split('\n'), 1):
        for m in REF.finditer(line):
            k, a, b = m.group(1), int(m.group(2)), m.group(3)
            n_ref += 1
            hi = int(b) if b else a
            if not (1 <= a <= len(LINES[k]) and a <= hi <= len(LINES[k])):
                fails.append(f'{os.path.basename(path)}:{ln}: {m.group(0)} out of range')
        for m in CLAIM.finditer(line):
            ident, k, a = m.group(1), m.group(2), int(m.group(3))
            n_claim += 1
            window = '\n'.join(LINES[k][max(0, a - 4):a + 3])
            probe = ident.split('(')[0].strip() if ident.endswith(')') and '(' in ident and ' ' not in ident.split('(')[0] else ident
            if probe not in window and probe.lower() not in window.lower():
                fails.append(f'{os.path.basename(path)}:{ln}: `{ident}` not found near {k}:{a}')
print(f'citations checked: {n_ref}; identifier claims checked: {n_claim}; failures: {len(fails)}')
for f in fails:
    print('  FAIL', f)
sys.exit(1 if fails else 0)
