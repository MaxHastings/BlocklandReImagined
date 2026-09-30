#!/usr/bin/env python3
"""The default Add-Ons: on in every copy of the game until a player turns them off.

packages/default-addons.json lists them, in load order: today the Duplicator
(two packages) and the Stunt Plane (Kaje and Ephialtes). Each is committed
under packages/<path>. The game installs them into a source checkout's
content/addons/<id> when it starts (crate bri-package, module defaults), and
tools/package_playtest.ps1 copies them into a release's content/addons/<id>
and lists them in its packages.json. One marked "enabled": false (the
Ragdoll) is carried but not listed, so it starts turned off. Nothing else
needs building.

An imported one (it has an "import" entry) is the output of Import Add-On
(bri-import-addon) over the original archive, converted once and committed.
Convert it again only when the importer improves:

    python tools/default_addons.py import  [--archive DIR] [--v20 DIR] [--core FILE]... [--only ID]...
    python tools/default_addons.py check   (each one present and whole)

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
PACKAGES = REPO / 'packages'
LIST = PACKAGES / 'default-addons.json'
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


def problems(addon, root=None):
    """Why packages/<path> is not a whole copy of the default Add-On (empty when it is)."""
    root = root or PACKAGES / addon['path']
    manifest = root / 'package.json'
    if not manifest.is_file():
        return [f'{root} has no package.json']
    info = json.loads(manifest.read_text(encoding='utf-8'))
    out = []
    if info.get('id') != addon['id']:
        out.append(f'{manifest} names {info.get("id")!r}, not {addon["id"]!r}')
    imported = addon.get('import')
    if imported:
        if imported['archive_sha256'] not in json.dumps(info.get('provenance', {})):
            out.append(f'{manifest} was not imported from the listed {imported["archive"]}')
        if info.get('version') != imported['version']:
            out.append(f'{manifest} is version {info.get("version")}, not {imported["version"]}')
        vehicles = root / 'assets' / 'vehicles.json'
        ids = {d.get('id') for d in json.loads(vehicles.read_text(encoding='utf-8')).get('definitions', [])} \
            if vehicles.is_file() else set()
        out += [f'{root} lacks vehicle {v}' for v in imported.get('vehicles', []) if v not in ids]
    return out


def check():
    bad = [p for addon in listed() for p in problems(addon)]
    if bad:
        fail('Default Add-Ons are missing or incomplete:\n  ' + '\n  '.join(bad))
    print(f'Default Add-Ons present: {", ".join(a["id"] for a in listed())}.')


def remembered_v20():
    try:
        return pathlib.Path((REPO / 'content' / '_regeneration' / 'v20-path.txt').read_text(encoding='utf-8').strip())
    except OSError:
        return None


def convert(archive, v20, core, only):
    imported = [a for a in listed() if a.get('import') and (not only or a['id'] in only)]
    if not imported:
        fail('No imported default Add-On matches.')
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
    importer = (target / 'release' / f'bri-import-addon{EXE}').resolve()
    for addon in imported:
        source = archive / addon['import']['archive']
        if not source.is_file():
            fail(f'{source} is missing.')
        digest = hashlib.sha256(source.read_bytes()).hexdigest()
        if digest != addon['import']['archive_sha256']:
            fail(f'{source} is not the listed copy: sha256 {digest}, expected {addon["import"]["archive_sha256"]}.')
        out = PACKAGES / addon['path']
        work = out.parent / f'.{addon["id"]}.import'
        shutil.rmtree(work, ignore_errors=True)
        work.mkdir(parents=True)
        fresh = work / addon['id']
        # Run beside the archive and name it bare, so the report records
        # the archive's name rather than a path on this PC.
        subprocess.run([str(importer), source.name, str(fresh), '--reference', str(v20.resolve()),
                        '--version', addon['import']['version'],
                        *[a for c in core for a in ('--core', str(c.resolve()))]],
                       cwd=archive, check=True, stdout=subprocess.DEVNULL)
        bad = problems(addon, fresh)
        if bad:
            fail('\n'.join(bad))
        shutil.rmtree(out, ignore_errors=True)
        fresh.rename(out)
        shutil.rmtree(work)
        print(f'Imported {addon["import"]["archive"]} into {out.relative_to(REPO)}. Review the diff and commit it.')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('command', choices=['import', 'check'])
    parser.add_argument('--archive', type=pathlib.Path,
                        default=pathlib.Path(os.environ.get('BRI_ADDON_ARCHIVE') or DEFAULT_ARCHIVE))
    parser.add_argument('--v20', type=pathlib.Path)
    parser.add_argument('--core', type=pathlib.Path, action='append')
    parser.add_argument('--only', action='append', help='import just this id (repeatable)')
    args = parser.parse_args()
    if args.command == 'check':
        check()
        return
    v20 = args.v20 or (pathlib.Path(os.environ['BRI_V20']) if os.environ.get('BRI_V20') else None) \
        or remembered_v20()
    convert(args.archive, v20, args.core or CORE, args.only)


if __name__ == '__main__':
    main()
