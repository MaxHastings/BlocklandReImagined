#!/usr/bin/env python3
"""Imported Blockland Add-Ons every release ships, turned on.

tools/shipped-addons.json lists them: today the Stunt Plane (Kaje and
Ephialtes). Like the base packs they are generated, never committed: `build`
runs Import Add-On (bri-import-addon) over each original archive into
content/shipped-addons/<id>. tools/ci_content.py carries them to release
builds, and tools/package_playtest.ps1 copies them into the release's
content/addons/<id> and turns them on in its packages.json, as it does the
Duplicator. A host offers them to joining players like any other Add-On.

    python tools/shipped_addons.py build   [--archive DIR] [--v20 DIR] [--core FILE]...
    python tools/shipped_addons.py check   (each one present and whole)

The archive folder defaults to $BRI_ADDON_ARCHIVE, then Maxwell's archive;
the v20 folder to $BRI_V20, then the one bootstrap last used; the core
scripts to bootstrap's recovered ones in .research/v20-dso (they resolve the
base datablocks an Add-On names, such as the vehicle splash emitters).
"""
import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
LIST = REPO / 'tools' / 'shipped-addons.json'
DIR = 'shipped-addons'
DEFAULT_ARCHIVE = pathlib.Path('C:/Users/Maxwell/Documents/_Blockland_Maxwell_1588_Archive/Addons')
EXE = '.exe' if os.name == 'nt' else ''
CORE = [REPO / '.research/v20-dso/server/scripts' / name for name in ('allGameScripts-Vanilla.cs', 'DamageTypes.cs')]


def fail(message):
    prefix = '::error::' if os.environ.get('GITHUB_ACTIONS') else ''
    sys.exit(f'{prefix}{message}')


def listed():
    data = json.loads(LIST.read_text(encoding='utf-8'))
    if data.get('schema_version') != 1:
        fail(f'Unsupported schema in {LIST}.')
    return data['addons']


def problems(content, addon):
    """Why content/shipped-addons/<id> is not a whole import of it (empty when it is)."""
    root = content / DIR / addon['id']
    manifest = root / 'package.json'
    if not manifest.is_file():
        return [f'{root} has no package.json']
    info = json.loads(manifest.read_text(encoding='utf-8'))
    out = []
    if info.get('id') != addon['id']:
        out.append(f'{manifest} names {info.get("id")!r}, not {addon["id"]!r}')
    source = json.dumps(info.get('provenance', {}))
    if addon['archive_sha256'] not in source:
        out.append(f'{manifest} was not imported from the listed {addon["archive"]}')
    vehicles = root / 'assets' / 'vehicles.json'
    ids = {d.get('id') for d in json.loads(vehicles.read_text(encoding='utf-8')).get('definitions', [])} \
        if vehicles.is_file() else set()
    out += [f'{root} lacks vehicle {v}' for v in addon.get('vehicles', []) if v not in ids]
    return out


def check(content):
    bad = [p for addon in listed() for p in problems(content, addon)]
    if bad:
        fail('Shipped Add-Ons are missing or incomplete. Run python tools/shipped_addons.py build on the PC '
             '(and python tools/ci_content.py upload for release builds):\n  '
             + '\n  '.join(bad))
    print(f'Shipped Add-Ons present: {", ".join(a["id"] for a in listed())}.')


def remembered_v20(content):
    try:
        return pathlib.Path((content / '_regeneration' / 'v20-path.txt').read_text(encoding='utf-8').strip())
    except OSError:
        return None


def build(content, archive, v20, core):
    if not archive.is_dir():
        fail(f'No Add-On archive folder at {archive}; pass --archive or set BRI_ADDON_ARCHIVE.')
    if v20 is None or not (v20 / 'base').is_dir():
        fail('Pass the Blockland v20 folder with --v20 (or set BRI_V20).')
    missing = [c for c in core if not c.is_file()]
    if missing:
        fail(f'Missing core scripts {", ".join(map(str, missing))}; run python tools/bootstrap.py first or pass --core.')
    subprocess.run(['cargo', 'build', '--release', '--locked', '-p', 'bri-addon-import', '--bin', 'bri-import-addon'],
                   cwd=REPO, check=True)
    target = pathlib.Path(os.environ.get('CARGO_TARGET_DIR', REPO / 'target'))
    importer = target / 'release' / f'bri-import-addon{EXE}'
    for addon in listed():
        source = archive / addon['archive']
        if not source.is_file():
            fail(f'{source} is missing.')
        digest = hashlib.sha256(source.read_bytes()).hexdigest()
        if digest != addon['archive_sha256']:
            fail(f'{source} is not the listed copy: sha256 {digest}, expected {addon["archive_sha256"]}.')
        out = content / DIR / addon['id']
        fresh = out.with_name(out.name + '.new')
        shutil.rmtree(fresh, ignore_errors=True)
        fresh.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run([str(importer), str(source), str(fresh), '--reference', str(v20),
                        '--version', addon['version'], *[a for c in core for a in ('--core', str(c))]],
                       cwd=REPO, check=True, stdout=subprocess.DEVNULL)
        shutil.rmtree(out, ignore_errors=True)
        fresh.rename(out)
        bad = problems(content, addon)
        if bad:
            fail('\n'.join(bad))
        print(f'Imported {addon["archive"]} into {out}.')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('command', choices=['build', 'check'])
    parser.add_argument('--content', type=pathlib.Path, default=REPO / 'content')
    parser.add_argument('--archive', type=pathlib.Path,
                        default=pathlib.Path(os.environ.get('BRI_ADDON_ARCHIVE') or DEFAULT_ARCHIVE))
    parser.add_argument('--v20', type=pathlib.Path)
    parser.add_argument('--core', type=pathlib.Path, action='append')
    args = parser.parse_args()
    content = args.content.resolve()
    if args.command == 'check':
        check(content)
        return
    v20 = args.v20 or (pathlib.Path(os.environ['BRI_V20']) if os.environ.get('BRI_V20') else None) \
        or remembered_v20(content)
    build(content, args.archive, v20, args.core or CORE)


if __name__ == '__main__':
    main()
