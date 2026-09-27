#!/usr/bin/env python3
"""Extract static object literals (GUI controls, profiles, audio profiles...)
from decompiled Blockland v20 TorqueScript/GUI files.

Read-only research tool. Never executes script. Objects created inside function
bodies are ignored (they are dynamic and documented by hand with line refs).

Usage: gui_extract.py <file> [<file>...] --out tree.json
Each object: {class, name, line, end_line, fields{}, children[], file}
"""
import json, re, sys, argparse, os

NEW_RE = re.compile(r'^\s*(?:%[\w]+\s*=\s*)?new\s+([A-Za-z_]\w*)\s*\(\s*"?([^")]*)"?\s*\)\s*(\{)?\s*;?\s*$')
FIELD_RE = re.compile(r'^\s*([A-Za-z_]\w*(?:\[[^\]]*\])?)\s*=\s*(.*?);\s*$')
FUNC_RE = re.compile(r'^\s*(function|package|if|else|for|while|switch|datablock)\b')

def unquote(v):
    v = v.strip()
    if len(v) >= 2 and v[0] == '"' and v[-1] == '"':
        s = v[1:-1]
        out, i = [], 0
        while i < len(s):
            c = s[i]
            if c == '\\' and i + 1 < len(s):
                n = s[i + 1]
                m = {'n': '\n', 't': '\t', '"': '"', '\\': '\\', "'": "'"}
                if n in m:
                    out.append(m[n]); i += 2; continue
                if n == 'c' and i + 2 < len(s):
                    out.append('<c%s>' % s[i + 2]); i += 3; continue
                if n == 'x' and i + 3 < len(s):
                    out.append(chr(int(s[i+2:i+4], 16))); i += 4; continue
            out.append(c); i += 1
        return ''.join(out)
    try:
        return int(v)
    except ValueError:
        try:
            return float(v)
        except ValueError:
            return {'expr': v}

def count_braces(line):
    # ignore braces inside strings
    depth, instr, esc = 0, False, False
    for ch in line:
        if esc: esc = False; continue
        if ch == '\\': esc = True; continue
        if ch == '"': instr = not instr; continue
        if instr: continue
        if ch == '{': depth += 1
        elif ch == '}': depth -= 1
    return depth

def parse(path):
    raw = open(path, 'rb').read()
    try:
        text = raw.decode('utf-8')
    except UnicodeDecodeError:
        text = raw.decode('latin-1')
    lines = text.replace('\r', '').split('\n')
    roots, stack = [], []
    code_depth = 0          # >0 while inside a function/package body
    pending = None
    awaiting_code_open = False
    guard_open = 0
    for ln, raw in enumerate(lines, 1):
        line = raw.strip()
        if code_depth > 0:
            code_depth += count_braces(line)
            continue
        if awaiting_code_open:
            d = count_braces(line)
            if d > 0:
                code_depth = d; awaiting_code_open = False
            continue
        if pending is not None:
            if line == '{':
                stack.append(pending); pending = None
                continue
            # object literal with no body: `new X(Y);`
            (stack[-1]['children'] if stack else roots).append(pending); pending = None
        m = NEW_RE.match(raw)
        if m and (not line.endswith(';') or line.endswith('{')):
            nm, parent = (m.group(2) or ''), None
            if ':' in nm:                      # `new X(Name : Parent)` inheritance
                nm, parent = [t.strip() for t in nm.split(':', 1)]
            obj = {'class': m.group(1), 'name': nm or None, 'line': ln,
                   'fields': {}, 'children': [], 'file': os.path.basename(path)}
            if parent:
                obj['parent'] = parent
            if m.group(3):
                stack.append(obj)
            else:
                pending = obj
            continue
        if stack:
            if line in ('};', '}'):
                obj = stack.pop(); obj['end_line'] = ln
                (stack[-1]['children'] if stack else roots).append(obj)
                continue
            fm = FIELD_RE.match(raw)
            if fm:
                stack[-1]['fields'][fm.group(1)] = unquote(fm.group(2))
            continue
        # top level, not in an object literal
        if line.startswith(('function', 'package', 'datablock')):
            d = count_braces(line)
            if d > 0: code_depth = d
            else: awaiting_code_open = True
            continue
        # Top-level if/else guards (e.g. `if (!isObject(X)) { new GuiControlProfile(X) {...}; }`)
        # are transparent: object literals inside them are still static definitions.
        if FUNC_RE.match(line):
            guard_open += count_braces(line)
            continue
        if line == '{':
            guard_open += 1
            continue
        if line in ('}', '};') and guard_open > 0:
            guard_open -= 1
            continue
    return roots

if __name__ == '__main__':
    ap = argparse.ArgumentParser()
    ap.add_argument('files', nargs='+')
    ap.add_argument('--out', required=True)
    a = ap.parse_args()
    result = {}
    for p in a.files:
        result[os.path.basename(p)] = parse(p)
    with open(a.out, 'w') as f:
        json.dump(result, f, indent=1)
    for k, v in result.items():
        def cnt(o): return 1 + sum(cnt(c) for c in o['children'])
        print(k, 'top-level objects:', len(v), 'total:', sum(cnt(o) for o in v))
