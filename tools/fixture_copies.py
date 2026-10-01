#!/usr/bin/env python3
"""Fail when a test fixture repeats an original Add-On's text.

Our fixtures (crates/**/tests/fixtures) are CC0 stand-ins written for the
port tests: they keep the original Add-Ons' folder, datablock and function
names and the code shapes a port reads, with our own code and numbers. This
check compares every text line of every fixture, with its whitespace
collapsed, against the lines of the originals, and lists each fixture line
over 30 characters that an original has word for word and that carries a
value of its own: a number (not an array index) or text with a space in
it. A line of only names and syntax (`datablock ItemData(GunItem : Base)`,
`image = GunImage;`, `Parent::onFire(%this, %obj, %slot);`) is the
interface a port reads and may match; the numbers and wording must be the
stand-in's own, unless a port's pattern reads that line (its entry.json
`covers`, the script rules of the port and its includes, and the ammo
types its magazine table names): those are the code shapes the port
checks the copy against. 0 and 1 count as syntax, and a line that three or
more different originals share is a common TorqueScript idiom.

The originals are never in the repository: pass the folders or zips that
hold them (Maxwell's copies, the private text zips), and run it locally.

    python tools/fixture_copies.py --originals path/to/originals.zip other/folder

Exit status 1 when any line matches, 0 when none does.
"""
import argparse
import json
import pathlib
import re
import sys
import zipfile

REPO = pathlib.Path(__file__).resolve().parent.parent
# Script and text files an Add-On ships (not our own JSON formats).
TEXT = ('.cs', '.txt', '.gui', '.hfl', '.mis')
MIN = 31


def norm(line):
    return re.sub(r'\s+', ' ', line.strip())


def carries_value(line):
    """Whether `line` holds a number or wording, beyond names and syntax."""
    line = re.sub(r'\[\d+\]', '[]', line)
    if re.search(r'"[^"]*\s[^"]*"', line):
        return True
    names_only = re.sub(r'"[^"\s]*"', '""', line)
    # 0 and 1 (a loop start, an on/off flag) are syntax, not a value.
    numbers = re.findall(r'(?<![\w.%$])\d*\.?\d+(?!\w)', names_only)
    return any(float(n) not in (0, 1) for n in numbers)


def lines(text):
    return {n for n in map(norm, text.splitlines()) if len(n) >= MIN}


# A line this many different originals share is a common TorqueScript idiom
# (`for(%i = 0; %i < %count; %i++)`), not one Add-On's own text.
IDIOM = 3


def read_by_ports():
    """Per fixture file, the line numbers a port's patterns match."""
    ports_dir = REPO / 'crates/addon-import/ports'
    fixtures = REPO / 'crates/addon-import/tests/fixtures/ports'
    read = {}
    for entry in sorted(ports_dir.glob('*/entry.json')):
        port = json.loads(entry.read_text())
        folder = fixtures / port['addon']
        if not folder.is_dir():
            continue
        covers = {k.lower(): list(v.values()) for k, v in port.get('covers', {}).items()}
        rules = []
        data = {}
        manifest = ports_dir / port['port'] / 'port.json'
        if manifest.is_file():
            data = json.loads(manifest.read_text())
            for name in data.get('include', []):
                rules += json.loads((ports_dir / '_shared' / f'{name}.json').read_text()).get('scripts', [])
            rules += data.get('scripts', [])
        # The ammo types a port's magazine table names (`ammotype = "Rifle"`).
        types = list(data.get('magazines', {}).get('types', {})) if manifest.is_file() else []
        for f in folder.rglob('*.cs'):
            text = f.read_text(encoding='latin-1')
            spans = [(m.start(), m.end()) for t in types
                     for m in re.finditer(r'ammotype\s*=\s*"' + re.escape(t) + '"', text, re.I)]
            for p in covers.get(f.relative_to(folder).as_posix().lower(), []):
                spans += [(m.start(), m.end()) for m in re.finditer(p, text, re.I)]
            for m in re.finditer(r'function\s+([\w:]+)\s*\([^)]*\)\s*\{', text):
                name = m.group(1).lower()
                method = name.split('::')[-1]
                pats = covers.get(name, []) + [r['pattern'] for r in rules if 'pattern' in r
                                               and r.get('method', '*').lower() in ('*', method)]
                end = text.find('\n}', m.end())
                body = text[m.end():end if end > 0 else len(text)]
                for p in pats:
                    try:
                        spans += [(m.end() + x.start(), m.end() + x.end()) for x in re.finditer(p, body, re.I)]
                    except re.error:
                        pass
            lines = set()
            for a, b in spans:
                lines.update(range(text.count('\n', 0, a) + 1, text.count('\n', 0, b) + 2))
            read[f.resolve()] = lines
    return read


def addon_of(name):
    """The Add-On folder a file path in the originals sits in."""
    parts = [p for p in re.split(r'[\\/]', name) if p]
    for p in parts:
        if re.match(r'(?i)(weapon|gamemode|tool|support|event|script|server|item|player|vehicle|emote)_', p):
            return p.lower()
    return parts[0].lower() if parts else ''


def originals(paths):
    """Each original line, with the Add-Ons that have it."""
    seen = {}

    def add(name, text):
        for line in lines(text):
            seen.setdefault(line, set()).add(addon_of(name))
    for path in map(pathlib.Path, paths):
        if path.is_file() and path.suffix.lower() == '.zip':
            with zipfile.ZipFile(path) as z:
                for name in z.namelist():
                    if name.lower().endswith(TEXT) and '/imported/' not in f'/{name}':
                        add(name, z.read(name).decode('latin-1'))
        elif path.is_dir():
            for f in path.rglob('*'):
                if f.is_file() and f.suffix.lower() in TEXT and 'imported' not in f.parts:
                    add(f.relative_to(path).as_posix(), f.read_text(encoding='latin-1'))
        else:
            sys.exit(f'{path}: not a folder or zip')
    return {line for line, addons in seen.items() if len(addons) < IDIOM}


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--originals', nargs='+', required=True,
                        help='folders or zips holding the original Add-Ons')
    parser.add_argument('--fixtures', type=pathlib.Path, default=REPO / 'crates',
                        help='where to look for tests/fixtures folders (default: crates)')
    args = parser.parse_args()
    seen = originals(args.originals)
    if not seen:
        sys.exit('No original text found: check --originals.')
    read = read_by_ports()
    found = 0
    for f in sorted(args.fixtures.rglob('*')):
        if not f.is_file() or 'fixtures' not in f.parts or f.suffix.lower() not in TEXT:
            continue
        for number, line in enumerate(f.read_text(encoding='latin-1').splitlines(), 1):
            if (norm(line) in seen and len(norm(line)) >= MIN and carries_value(norm(line))
                    and number not in read.get(f.resolve(), ())):
                found += 1
                print(f'{f.relative_to(REPO)}:{number}: {norm(line)}')
    print(f'{found} fixture line(s) repeat an original.' if found else 'No fixture line repeats an original.')
    return 1 if found else 0


if __name__ == '__main__':
    sys.exit(main())
