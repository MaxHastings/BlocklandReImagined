#!/usr/bin/env python3
"""Compare v20's client pref defaults with the defaults our code assumes.

Reads `client/defaults.cs` of the recovered v20 client scripts (the last
assignment of each `$pref::...` wins, as when Torque executes it) and, for
every pref, the `"<pref>"` literals in `crates/` together with the default
passed to the nearest `bool_or` / `i64_or` / `f64_or` / `str_or` call on the
same line. Prints prefs whose default differs from v20's and prefs our code
never names.

    python tools/audit_v20_prefs.py [--defaults .research/bl-decompiled/v20/client/defaults.cs]
"""
import argparse
import pathlib
import re

REPO = pathlib.Path(__file__).resolve().parents[1]
ASSIGN = re.compile(r'(?im)^\s*(\$pref::[\w:]+)\s*=\s*("[^"]*"|[^;]+);')
USE = re.compile(r'(?i)(bool|i64|f64|str|f32)_or\(\s*"(\$pref::[\w:]+)"\s*,\s*([^)]*)\)')


def number(v):
    v = v.strip().strip('"')
    if v.lower() in ('true', 'false'):
        return 1.0 if v.lower() == 'true' else 0.0
    try:
        return float(v.rstrip('f').replace('_', ''))
    except ValueError:
        return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--defaults', type=pathlib.Path,
                    default=REPO / '.research/bl-decompiled/v20/client/defaults.cs')
    args = ap.parse_args()
    v20 = {}
    for name, value in ASSIGN.findall(args.defaults.read_text(encoding='latin-1')):
        v20[name.lower()] = (name, value.strip())
    ours = {}
    named = set()
    for p in (REPO / 'crates').rglob('*.rs'):
        if 'target' in p.parts:
            continue
        text = p.read_text(encoding='utf-8', errors='replace')
        low = text.lower()
        for key in v20:
            if f'"{key}"' in low:
                named.add(key)
        for m in USE.finditer(text):
            ours.setdefault(m.group(2).lower(), set()).add(
                (m.group(3).strip(), str(p.relative_to(REPO)).replace('\\', '/')))
    differ, missing = [], []
    for key, (name, value) in sorted(v20.items()):
        if key not in named:
            missing.append(f'{name} = {value}')
            continue
        for default, where in sorted(ours.get(key, ())):
            a, b = number(value), number(default)
            same = (a == b) if a is not None and b is not None else (
                value.strip('"') == default.strip('"').removesuffix('.into()').strip('"'))
            if not same:
                differ.append(f'{name}: v20 {value}, ours {default} ({where})')
    print('## Defaults that differ from v20')
    print('\n'.join(differ) or '(none)')
    print('\n## v20 prefs our code never names')
    print('\n'.join(missing) or '(none)')
    print(f'\n{len(v20)} v20 prefs, {len(differ)} differing defaults, {len(missing)} unnamed')


if __name__ == '__main__':
    main()
