#!/usr/bin/env python3
"""List every field v20's stock datablocks set and which importer reads it.

Reads the recovered core script and every stock add-on ZIP of a read-only v20
install, collects the literal `datablock Class(Name) { field = value; }`
blocks of the gameplay classes (weapons, projectiles, explosions, items,
vehicles, players, emitters, sounds), and reports, per class and field, how
many datablocks set it, a sample value, and the importer source files that
name the field. A field no importer names never reaches the game.

    python tools/audit_v20_fields.py --v20 "E:/.../Blockland v20" [--json out.json] [--md out.md]

docs/audits/v20-fidelity.md is the reviewed result.

Read only: nothing is written inside the v20 folder. Field references are
found by a case-insensitive search for the field name as a quoted string (or
`name[` for arrays) in the importer sources listed in IMPORTERS, so a field
read under a different spelling is reported as unread; the audit document
records such cases by hand.
"""
import argparse
import collections
import json
import pathlib
import re
import zipfile

REPO = pathlib.Path(__file__).resolve().parents[1]
CORE = REPO / '.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs'

CLASSES = {
    'itemdata', 'shapebaseimagedata', 'projectiledata', 'explosiondata',
    'debrisdata', 'particleemitterdata', 'particledata', 'audioprofile',
    'audiodescription', 'wheeledvehicledata', 'wheeledvehicletire',
    'wheeledvehiclespring', 'flyingvehicledata', 'hovervehicledata',
    'playerdata', 'splashdata', 'shapebasedata', 'staticshapedata',
}

# Importers and the data files they write through (anything that turns a v20
# datablock field into native content).
IMPORTERS = ['crates', 'docs/research/weapon-effects/importer',
             'docs/research/weapon-debris/importer', 'docs/research/item-rendering']

BLOCK = re.compile(r'(?is)\bdatablock\s+(\w+)\s*\(\s*(\w+)\s*(?::\s*(\w+)\s*)?\)\s*\{([^{}]*)\}\s*;')
FIELD = re.compile(r'(?m)([A-Za-z_]\w*)\s*(?:\[\s*[^\]]*\])?\s*=\s*([^;]*);')


def strip_comments(text):
    out, i, quote = [], 0, None
    while i < len(text):
        c = text[i]
        if quote:
            out.append(c)
            if c == '\\' and i + 1 < len(text):
                out.append(text[i + 1])
                i += 2
                continue
            if c == quote:
                quote = None
        elif c in '"\'':
            quote = c
            out.append(c)
        elif text.startswith('//', i):
            j = text.find('\n', i)
            i = len(text) if j < 0 else j
            continue
        elif text.startswith('/*', i):
            j = text.find('*/', i + 2)
            i = len(text) if j < 0 else j + 2
            continue
        else:
            out.append(c)
        i += 1
    return ''.join(out)


def sources(v20, core):
    yield 'core', core.read_text(encoding='latin-1')
    for z in sorted((v20 / 'Add-Ons').glob('*.zip')):
        with zipfile.ZipFile(z) as archive:
            for name in archive.namelist():
                if name.lower().endswith('.cs'):
                    yield f'{z.stem}/{name}', archive.read(name).decode('latin-1')


def collect(v20, core):
    blocks = {}
    for origin, text in sources(v20, core):
        for m in BLOCK.finditer(strip_comments(text)):
            cls, name, parent, body = m.group(1).lower(), m.group(2), m.group(3), m.group(4)
            if cls not in CLASSES:
                continue
            fields = collections.OrderedDict()
            for f in FIELD.finditer(body):
                fields.setdefault(f.group(1).lower(), f.group(2).strip())
            blocks[name.lower()] = dict(cls=cls, name=name, parent=parent, origin=origin, fields=fields)
    return blocks


def importer_text():
    text = {}
    for root in IMPORTERS:
        base = REPO / root
        if not base.exists():
            continue
        for p in base.rglob('*'):
            if p.suffix in {'.rs', '.py', '.toml'} and 'target' not in p.parts and 'tests' not in p.parts and p.stat().st_size < 4 << 20:
                text[str(p.relative_to(REPO)).replace('\\', '/')] = p.read_text(encoding='utf-8', errors='replace').lower()
    return text


def readers(field, text):
    """Files naming the field as a quoted key (read from data), else as a word
    (a hand-ported constant whose comment cites the datablock field)."""
    needles = [f'"{field}"', f'"{field}[', f"'{field}'", f"'{field}["]
    quoted = sorted(p for p, body in text.items() if any(n in body for n in needles))
    if quoted:
        return quoted
    # `air_control` ports `airControl`: compare with underscores removed.
    word = re.compile(rf'(?<![a-z0-9]){re.escape(field)}(?![a-z0-9])')
    return sorted(p + ' (cited)' for p, body in text.items() if word.search(body.replace('_', '')))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--v20', required=True, type=pathlib.Path)
    ap.add_argument('--core', type=pathlib.Path, default=CORE,
                    help='recovered allGameScripts-Vanilla.cs (tools/regenerate_content.py makes it)')
    ap.add_argument('--json', type=pathlib.Path)
    ap.add_argument('--md', type=pathlib.Path)
    args = ap.parse_args()
    blocks = collect(args.v20, args.core)
    text = importer_text()
    table = collections.defaultdict(lambda: collections.defaultdict(list))
    for b in blocks.values():
        for field, value in b['fields'].items():
            table[b['cls']][field].append((b['name'], value, b['origin']))
    rows = []
    for cls in sorted(table):
        for field in sorted(table[cls]):
            users = table[cls][field]
            rows.append(dict(cls=cls, field=field, count=len(users),
                             sample=f'{users[0][0]} = {users[0][1]}'[:70],
                             readers=readers(field, text)))
    if args.json:
        args.json.write_text(json.dumps(dict(blocks=blocks, rows=rows), indent=1))
    lines = ['| Class | Field | Blocks | Sample | Read by |', '|---|---|---|---|---|']
    for r in rows:
        who = ', '.join(p.split('/')[1] if p.startswith('crates/') else p.split('/')[2] for p in r['readers']) or '**none**'
        lines.append(f"| {r['cls']} | {r['field']} | {r['count']} | `{r['sample'].replace('|', '/')}` | {who} |")
    out = '\n'.join(lines) + '\n'
    if args.md:
        args.md.write_text(out, encoding='utf-8')
    else:
        print(out)
    unread = sum(1 for r in rows if not r['readers'])
    print(f'{len(blocks)} datablocks, {len(rows)} class fields, {unread} read by no importer', flush=True)


if __name__ == '__main__':
    main()
