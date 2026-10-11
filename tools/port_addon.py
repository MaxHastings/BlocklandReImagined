#!/usr/bin/env python3
"""One command from a legacy Add-On ZIP to a checked, installable native Add-On.

    python tools/port_addon.py build SOURCE.zip --out DIR [options]
    python tools/port_addon.py install PORTED.zip --content CONTENT_DIR

`build` runs, in order, and stops at the first stage that fails:

  1. inputs    hash every source ZIP (they are only read);
  2. import    run Import Add-On, with the registered recipe for a known
               framework (Rallypack, the Rallypack P90) or plain for anything else;
  3. closure   with --only ITEM..., keep just what those items need and say
               what they need that the Add-On does not define (addon_closure.py);
  4. verify    package identity, companions, declared files and hashes, then
               bri-addon-check on every Add-On the way the game loads them
               (manifest, files, scripts, weapon state timing);
  5. package   one deterministic ZIP, folders at its root;
  6. install   with --content, put the ZIP's folders into the game's content
               folder as the in-game ZIP install does, leaving which Add-Ons are
               on exactly as it was.

It writes BUILD-RECEIPT.json beside the ZIP: the target game, every input hash,
what each stage found and what was installed. Original Add-ons and the game's
own files are never written. Nothing here runs a script or opens a window.

The recipes are tools/build_rallypack.py and tools/build_rallypack_p90.py. A
framework's next weapon needs no new recipe: ask for it with --only, or give its
ZIP and let the plain import carry it.
"""
import argparse
import datetime
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import zipfile

REPO = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / 'tools'))
import addon_closure  # noqa: E402

EXE = '.exe' if os.name == 'nt' else ''
RECEIPT_SCHEMA = 1
# The release the build is made for. A package that uses a newer engine seam
# needs the matching game; downloading it cannot update an older executable.
TARGET_GAME = 'v0.2.9'
RECEIPT = 'BUILD-RECEIPT.json'
MANIFEST = 'package.json'
STAMP = (1980, 1, 1, 0, 0, 0)
MAX_ENTRIES = 20000
MAX_BYTES = 1 << 30


class Failed(Exception):
    """A stage failed; the message says what to repair."""


def fail(stage, message):
    raise Failed(f'[{stage}] {message}')


def read(path):
    return json.loads(pathlib.Path(path).read_text(encoding='utf-8'))


def write(path, value):
    path = pathlib.Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')


def sha256(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as handle:
        for block in iter(lambda: handle.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def tree_hash(folder):
    """One hash of every file's relative path and bytes under `folder`."""
    digest = hashlib.sha256()
    for path in sorted(p for p in pathlib.Path(folder).rglob('*') if p.is_file()):
        digest.update(path.relative_to(folder).as_posix().encode() + b'\0')
        digest.update(bytes.fromhex(sha256(path)))
    return digest.hexdigest()


# ------------------------------------------------------------------ tools


def find_tool(name, given=None, build=None):
    """A game tool: the path given, $BRI_<NAME>, a build in this checkout, PATH.
    If there is none, build it once from this checkout when `build` names its
    cargo arguments."""
    if given:
        return pathlib.Path(given).resolve()
    env = os.environ.get('BRI_' + name.upper().replace('-', '_'))
    if env:
        return pathlib.Path(env).resolve()
    target = pathlib.Path(os.environ.get('CARGO_TARGET_DIR', REPO / 'target'))
    for profile in ('release', 'debug'):
        found = target / profile / (name + EXE)
        if found.is_file():
            return found.resolve()
    on_path = shutil.which(name)
    if on_path:
        return pathlib.Path(on_path).resolve()
    if build:
        subprocess.run(['cargo', 'build', '--release', '--locked', *build], cwd=REPO, check=True)
        return (target / 'release' / (name + EXE)).resolve()
    fail('tools', f'{name} was not found: build it (cargo build --release -p ...) or pass its path.')


class Tools:
    """The commands the stages run. A command is a list, so a test can stand in
    a script for the real executable."""

    def __init__(self, importer, checker, content=None, vanilla=None):
        self.importer = list(importer)
        self.checker = list(checker)
        self.content = content
        self.vanilla = vanilla


# ------------------------------------------------------------------ recipes


def namespace(stem):
    """The package id the importer gives an Add-On (bri_addon_import::namespace_for)."""
    out = ''
    for c in stem.lower():
        c = c if (c.isascii() and (c.isalnum() or c == '-')) else '_'
        if not (c == '_' and out.endswith('_')):
            out += c
    out = out.strip('_-')
    if not out[:1].isascii() or not out[:1].isalpha():
        out = f'addon_{out}'
    return out.rstrip('_-')


def run(command, log, stage):
    result = subprocess.run(command, capture_output=True, text=True, encoding='utf-8', errors='replace')
    pathlib.Path(log).parent.mkdir(parents=True, exist_ok=True)
    pathlib.Path(log).write_text(result.stdout + result.stderr, encoding='utf-8')
    if result.returncode:
        tail = (result.stdout + result.stderr).strip().splitlines()[-3:]
        fail(stage, f'{pathlib.Path(command[0]).name} exited {result.returncode}: {" | ".join(tail)} (full log {log})')
    return result


def recipe_plain(args, tools, build):
    """Any Add-On: Import Add-On as a player's Import runs it. Needed Add-Ons
    given with --needs are laid beside it as the reference."""
    source = args.sources[0]
    out = build / namespace(source.stem)
    command = [*tools.importer, str(source), str(out)]
    with tempfile.TemporaryDirectory(prefix='bri-port-') as temporary:
        reference = tools.vanilla
        if args.needs:
            refs = pathlib.Path(temporary) / 'reference'
            (refs / 'Add-Ons').mkdir(parents=True)
            for needed in args.needs:
                shutil.copyfile(needed, refs / 'Add-Ons' / needed.name)
            reference = refs
        if reference:
            command += ['--reference', str(reference)]
        if tools.content:
            command += ['--installed', str(tools.content)]
        run(command, build / 'import.log', 'import')
    for name in ('IMPORT-REPORT.md', 'import-report.json'):
        if (out / name).is_file():
            shutil.copyfile(out / name, build / ('source-' + name))
    return {'recipe': 'plain', 'items': None}


def recipe_module(module_name):
    def recipe(args, tools, build):
        if len(tools.importer) != 1:
            fail('import', 'a framework recipe needs the importer as one executable path.')
        module = __import__(module_name)
        needs_vanilla = tools.vanilla is not None and tools.content is not None
        if not needs_vanilla:
            fail('import', f'{module_name} needs --content (the generated game content) and --vanilla '
                           '(the Blockland v20 folder): run python tools/bootstrap.py --v20 ... first.')
        namespace_args = argparse.Namespace(
            source=args.sources[0], importer=pathlib.Path(tools.importer[0]), content=pathlib.Path(tools.content),
            vanilla=pathlib.Path(tools.vanilla), output=build)
        if module_name == 'build_rallypack_p90':
            if not args.needs:
                fail('import', 'the P90 recipe needs Weapon_Rallypack.zip: give it with --needs.')
            namespace_args.p90, namespace_args.rallypack = args.sources[0], args.needs[0]
        # The recipes require a fresh output folder; the build folder is made by them.
        shutil.rmtree(build)
        module.build(namespace_args)
        return {'recipe': module_name, 'items': None}
    return recipe


RECIPES = {
    # Add-On folder name (lower case) -> (recipe name, function, ZIPs it needs besides the source).
    'weapon_rallypack': ('rallypack', recipe_module('build_rallypack'), 0),
    'weapon_rallypack_p90': ('rallypack-p90', recipe_module('build_rallypack_p90'), 1),
}


def choose_recipe(source, forced):
    """(name, function, needed ZIP count) for a source ZIP."""
    if forced == 'plain':
        return 'plain', recipe_plain, 0
    key = source.stem.lower()
    for name, (label, function, needs) in RECIPES.items():
        if forced == label or (forced is None and key == name):
            return label, function, needs
    if forced not in (None, 'auto'):
        fail('import', f'no recipe named {forced}; known: plain, ' + ', '.join(l for l, _, _ in RECIPES.values()))
    return 'plain', recipe_plain, 0


# ------------------------------------------------------------------ verifying


def add_on_folders(build):
    """Folders in `build` that are Add-Ons (hold a package.json), in name order."""
    return sorted(p for p in pathlib.Path(build).iterdir() if (p / MANIFEST).is_file())


def verify_identity(folders, installed_ids):
    """Cross-package checks the single-folder checker cannot make. Returns problems."""
    problems, ids = [], {}
    for folder in folders:
        manifest = read(folder / MANIFEST)
        ident = manifest.get('id')
        if not ident:
            problems.append(f'{folder.name}: package.json has no id')
            continue
        if ident in ids:
            problems.append(f'{folder.name}: id {ident} is also {ids[ident]}; give one of them another id')
        ids[ident] = folder.name
    for folder in folders:
        manifest = read(folder / MANIFEST)
        for companion in manifest.get('companions', []):
            if companion not in ids:
                problems.append(f'{folder.name}: names companion {companion} but no such folder was built')
        for provided in manifest.get('provides', []):
            file = provided.get('file')
            if file and not (folder / file).is_file() and not (folder / 'assets' / file).is_file():
                problems.append(f'{folder.name}: provides {provided.get("id")} from {file}, which is not there')
    for ident, name in ids.items():
        if installed_ids.get(ident, name) != name:
            problems.append(f'{name}: id {ident} is already installed as {installed_ids[ident]}; '
                            'the game would load two Add-Ons with one identity: remove one first')
    return problems


def verify_hashes(folders):
    """The presentation's recorded hashes match the files it points at."""
    problems = []
    for folder in folders:
        assets = folder / 'assets'
        presentation = assets / 'presentation.json'
        if not presentation.is_file():
            continue
        data = read(presentation)
        for field, name in (('weapons_sha256', 'weapons.json'), ('item_physics_sha256', 'item-physics.json')):
            file = assets / name
            if data.get(field) and file.is_file() and sha256(file) != data[field]:
                problems.append(f'{folder.name}: presentation.json {field} does not match {name}; '
                                'a step edited it after hashing')
        for key, model in (data.get('models') or {}).items():
            file = assets / model.get('file', '')
            if model.get('sha256') and file.is_file() and sha256(file) != model['sha256']:
                problems.append(f'{folder.name}: model {key} changed after its hash was recorded')
            elif model.get('file') and not file.is_file():
                problems.append(f'{folder.name}: model {key} names {model["file"]}, which is missing')
        for key, texture in (data.get('textures') or {}).items():
            if texture.get('file') and not (assets / texture['file']).is_file():
                problems.append(f'{folder.name}: texture {key} names {texture["file"]}, which is missing')
    return problems


def unreferenced_assets(folder):
    """Files under assets/ that no JSON in the Add-On names: implementation
    leftovers that ship for nothing. Reported, never deleted."""
    assets = pathlib.Path(folder) / 'assets'
    if not assets.is_dir():
        return []
    text = ''
    for path in assets.rglob('*.json'):
        text += path.read_text(encoding='utf-8', errors='replace')
    for path in pathlib.Path(folder).glob('*.json'):
        text += path.read_text(encoding='utf-8', errors='replace')
    leftovers = []
    for path in sorted(assets.rglob('*')):
        if path.is_file() and path.suffix != '.json':
            rel = path.relative_to(assets).as_posix()
            if rel not in text:
                leftovers.append(rel)
    return leftovers


def run_checker(tools, folder):
    """bri-addon-check on one Add-On. Returns the parsed report."""
    result = subprocess.run([*tools.checker, str(folder), '--json'], capture_output=True, text=True,
                            encoding='utf-8', errors='replace')
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        fail('verify', f'bri-addon-check printed no report for {folder.name} (exit {result.returncode}): '
                       + (result.stderr.strip() or result.stdout.strip())[:300])


def summarize(report):
    errors = [d for d in report.get('diagnostics', []) if str(d.get('severity', '')).lower() == 'error']
    warnings = [d for d in report.get('diagnostics', []) if str(d.get('severity', '')).lower() == 'warning']
    return {'ok': bool(report.get('ok')), 'errors': errors, 'warnings': warnings}


def verify(build, tools, installed_ids):
    folders = add_on_folders(build)
    if not folders:
        fail('verify', f'the import left no Add-On in {build}: see {build / "import.log"}')
    problems = verify_identity(folders, installed_ids) + verify_hashes(folders)
    if problems:
        fail('verify', 'cannot package yet:\n  ' + '\n  '.join(problems))
    results = {}
    for folder in folders:
        summary = summarize(run_checker(tools, folder))
        summary['unreferenced_assets'] = unreferenced_assets(folder)
        results[folder.name] = summary
    failing = {k: v for k, v in results.items() if not v['ok']}
    if failing:
        lines = []
        for name, summary in failing.items():
            for d in summary['errors'][:5] or [{'code': 'check', 'message': 'does not load'}]:
                lines.append(f'{name}: {d.get("code", "")} {d.get("message", "")}'.strip())
        fail('verify', 'bri-addon-check says the game could not load it:\n  ' + '\n  '.join(lines))
    return folders, results


# ------------------------------------------------------------------ packaging


def package(folders, zip_path):
    """Write the Add-On folders into one deterministic ZIP, folders at its root."""
    entries = []
    for folder in folders:
        for path in sorted(p for p in folder.rglob('*') if p.is_file()):
            entries.append((f'{folder.name}/{path.relative_to(folder).as_posix()}', path))
    entries.sort(key=lambda e: e[0])
    zip_path = pathlib.Path(zip_path)
    zip_path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(zip_path, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, path in entries:
            info = zipfile.ZipInfo(name, STAMP)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            archive.writestr(info, path.read_bytes())
    return len(entries)


def installed_packages(content):
    """Installed Add-On id -> folder, from every manifest under content/addons."""
    found = {}
    root = pathlib.Path(content) / 'addons' if content else None
    if not root or not root.is_dir():
        return found
    for manifest in root.glob('*/' + MANIFEST):
        try:
            found[read(manifest).get('id')] = manifest.parent.name
        except (OSError, ValueError):
            continue
    return found


def _safe_name(name):
    parts = name.replace('\\', '/').split('/')
    return not (name.startswith(('/', '\\')) or ':' in parts[0] or any(p in ('', '..') for p in parts[:-1])
                or parts[-1] in ('', '.', '..'))


def read_zip(zip_path, into):
    """Unpack a native Add-On ZIP into `into`, refusing anything that could
    land outside it. Returns the Add-On folder names it holds."""
    total = 0
    with zipfile.ZipFile(zip_path) as archive:
        infos = [i for i in archive.infolist() if not i.is_dir()]
        if len(infos) > MAX_ENTRIES:
            fail('install', f'{zip_path.name} holds {len(infos)} files; the limit is {MAX_ENTRIES}')
        for info in infos:
            if not _safe_name(info.filename):
                fail('install', f'{zip_path.name} holds an unsafe path: {info.filename}')
            total += info.file_size
            if total > MAX_BYTES:
                fail('install', f'{zip_path.name} is over the size limit')
        for info in infos:
            target = pathlib.Path(into) / info.filename
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(archive.read(info))
    tops = sorted({i.filename.replace('\\', '/').split('/')[0] for i in infos})
    folders = [t for t in tops if (pathlib.Path(into) / t / MANIFEST).is_file()]
    if not folders:
        fail('install', f'{zip_path.name} holds no Add-On: each folder at its top must hold a {MANIFEST}')
    if len(folders) != len(tops):
        fail('install', f'{zip_path.name} holds files outside its Add-On folders: '
                        + ', '.join(sorted(set(tops) - set(folders))))
    return folders


def install_zip(zip_path, content):
    """Put the ZIP's Add-On folders into content/addons, each replaced whole
    and all-or-nothing. Which Add-Ons are on (packages.json and
    packages-disabled.json) is not touched. Returns what was installed."""
    content = pathlib.Path(content)
    addons = content / 'addons'
    addons.mkdir(parents=True, exist_ok=True)
    before = {name: sha256(content / name) for name in ('packages.json', 'packages-disabled.json')
              if (content / name).is_file()}
    installed = []
    with tempfile.TemporaryDirectory(prefix='.install-', dir=addons) as temporary:
        stage = pathlib.Path(temporary) / 'new'
        old = pathlib.Path(temporary) / 'old'
        folders = read_zip(pathlib.Path(zip_path), stage)
        known = installed_packages(content)
        for name in folders:
            ident = read(stage / name / MANIFEST).get('id')
            if ident in known and known[ident] != name:
                fail('install', f'{name} has id {ident}, which {known[ident]} already has: '
                                'remove one, or the game would load two Add-Ons with one identity')
        swapped = []
        try:
            for name in folders:
                target = addons / name
                if target.exists():
                    old.mkdir(exist_ok=True)
                    os.replace(target, old / name)
                    swapped.append((target, old / name))
                else:
                    swapped.append((target, None))
                os.replace(stage / name, target)
                installed.append({'folder': name, 'id': read(target / MANIFEST).get('id'),
                                  'replaced': (old / name).exists(), 'tree_sha256': tree_hash(target)})
        except OSError as error:
            for target, backup in reversed(swapped):
                shutil.rmtree(target, ignore_errors=True)
                if backup is not None and backup.exists():
                    os.replace(backup, target)
            fail('install', f'could not install ({error}); nothing was changed')
    after = {name: sha256(content / name) for name in ('packages.json', 'packages-disabled.json')
             if (content / name).is_file()}
    if before != after:
        fail('install', 'the enabled list changed while installing; this must not happen')
    return installed


# ------------------------------------------------------------------ the workflow


def build_command(args, tools):
    for source in [*args.sources, *args.needs]:
        if not source.is_file():
            fail('inputs', f'{source} is not a file')
    out = args.out.resolve()
    if out.exists() and any(out.iterdir()):
        fail('inputs', f'{out} is not empty: give a fresh folder')
    out.mkdir(parents=True, exist_ok=True)
    build = out / 'build'
    build.mkdir()
    sources = {p.name: sha256(p) for p in [*args.sources, *args.needs]}
    label, function, needs = choose_recipe(args.sources[0], args.recipe)
    if needs and len(args.needs) < needs:
        fail('inputs', f'the {label} recipe needs {needs} more ZIP (--needs), for the framework it builds on.')
    receipt = {'schema_version': RECEIPT_SCHEMA, 'target_game': args.target_game, 'recipe': label,
               'created': datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='seconds'),
               'sources': sources, 'stages': {}}
    stage = 'import'
    function(args, tools, build)
    receipt['stages']['import'] = {'ok': True, 'log': 'build/import.log'}
    closure_report = None
    if args.only:
        folders = [f for f in add_on_folders(build) if (f / 'assets' / 'weapons.json').is_file()]
        if not folders:
            fail('closure', '--only needs an Add-On with weapons; the import left none.')
        try:
            closure_report = addon_closure.prune_package(folders[0], args.only, apply=True)
        except KeyError as error:
            fail('closure', str(error.args[0]))
        receipt['stages']['closure'] = {'ok': True, 'package': folders[0].name, **closure_report}
        if closure_report['dangling']:
            receipt['warnings'] = [f'needs {ref}, which the Add-On does not define (named by {", ".join(users)})'
                                   for ref, users in closure_report['dangling'].items()]
    known = installed_packages(args.install) if args.install else {}
    folders, checks = verify(build, tools, known)
    receipt['stages']['verify'] = {'ok': True, 'add_ons': checks}
    names = '+'.join(f.name for f in folders)
    version = read(folders[0] / MANIFEST).get('version', '0.0.0')
    zip_path = out / f'{folders[0].name}-{version}-for-{args.target_game}.zip'
    count = package(folders, zip_path)
    receipt['stages']['package'] = {'ok': True, 'zip': zip_path.name, 'sha256': sha256(zip_path), 'files': count,
                                    'add_ons': [{'folder': f.name, 'id': read(f / MANIFEST).get('id'),
                                                 'tree_sha256': tree_hash(f)} for f in folders]}
    if args.install:
        receipt['stages']['install'] = {'ok': True, 'content': str(args.install), 'add_ons': install_zip(zip_path, args.install),
                                        'enabled_list_changed': False}
    after = {p.name: sha256(p) for p in [*args.sources, *args.needs]}
    if after != sources:
        fail('inputs', 'a source ZIP changed while building; this must not happen')
    write(out / RECEIPT, receipt)
    return out, receipt, names


def describe(receipt, out):
    lines = [f'Built for {receipt["target_game"]} with the {receipt["recipe"]} recipe.']
    package_stage = receipt['stages']['package']
    lines.append(f'  {out / package_stage["zip"]}  ({package_stage["files"]} files)')
    for folder, summary in receipt['stages']['verify']['add_ons'].items():
        lines.append(f'  {folder}: checked, {len(summary["warnings"])} warning(s)'
                     + (f', {len(summary["unreferenced_assets"])} unreferenced file(s)' if summary['unreferenced_assets'] else ''))
        for warning in summary['warnings'][:5]:
            lines.append(f'    warning {warning.get("code", "")}: {warning.get("message", "")}')
    for warning in receipt.get('warnings', []):
        lines.append(f'  note: {warning}')
    if 'install' in receipt['stages']:
        lines.append('  Installed. It is off until you turn it on in Start Game > Add-Ons; '
                     'your enabled list was not changed.')
    else:
        lines.append('  Not installed: add --install CONTENT_DIR, or put the ZIP in the game with Add-Ons > Install.')
    lines.append(f'  Receipt: {out / RECEIPT}')
    return '\n'.join(lines)


def parser():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest='command', required=True)
    b = sub.add_parser('build', help='import, check and package an Add-On')
    b.add_argument('sources', nargs='+', type=pathlib.Path, metavar='SOURCE.zip')
    b.add_argument('--out', type=pathlib.Path, required=True, help='a fresh folder for the build and its receipt')
    b.add_argument('--needs', type=pathlib.Path, action='append', default=[],
                   help='an Add-On ZIP the source builds on (the P90 needs Weapon_Rallypack.zip)')
    b.add_argument('--recipe', help='plain, rallypack or rallypack-p90 (default: by the ZIP\'s name)')
    b.add_argument('--only', nargs='+', metavar='ITEM', help='keep only these items and what they need')
    b.add_argument('--install', type=pathlib.Path, metavar='CONTENT_DIR', help='then install into this content folder')
    b.add_argument('--target-game', default=TARGET_GAME)
    b.add_argument('--importer', type=pathlib.Path)
    b.add_argument('--checker', type=pathlib.Path)
    b.add_argument('--content', type=pathlib.Path, help='generated game content (default: this checkout\'s content/)')
    b.add_argument('--vanilla', type=pathlib.Path, help='the Blockland v20 folder, read only')
    i = sub.add_parser('install', help='install a ported Add-On ZIP into a content folder')
    i.add_argument('zip', type=pathlib.Path)
    i.add_argument('--content', type=pathlib.Path, required=True)
    return p


def default_vanilla():
    import addon_bundle
    return addon_bundle.v20_folder(None)


def main(argv=None):
    args = parser().parse_args(argv)
    try:
        if args.command == 'install':
            for item in install_zip(args.zip, args.content):
                print(f'Installed {item["folder"]} ({item["id"]})' + (', replacing the old copy' if item['replaced'] else ''))
            print('It is off until you turn it on in Start Game > Add-Ons.')
            return 0
        content = args.content or (REPO / 'content')
        vanilla = args.vanilla or default_vanilla()
        importer = find_tool('bri-import-addon', args.importer, ['-p', 'bri-addon-import', '--bin', 'bri-import-addon'])
        checker = find_tool('bri-addon-check', args.checker, ['-p', 'bri-package-runtime', '--bin', 'bri-addon-check'])
        tools = Tools([str(importer)], [str(checker)], content if pathlib.Path(content).is_dir() else None, vanilla)
        out, receipt, _ = build_command(args, tools)
        print(describe(receipt, out))
        return 0
    except Failed as error:
        print(f'port_addon: {error}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
