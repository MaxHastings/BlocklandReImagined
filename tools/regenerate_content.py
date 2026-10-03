#!/usr/bin/env python3
"""Regenerate every native content pack the client loads from a v20 install.

One ordered pipeline from a read-only Blockland v20 folder to the packs named
by the base package list (crates/package/base-packages.json). Works on Windows,
Linux and macOS. Most people want tools/bootstrap.py, which also checks the
toolchain first. See docs/content-regeneration.md.

    python tools/regenerate_content.py --v20 "/path/to/Blockland v20"

Each pack this script builds gets a stamp (content/_regeneration/stamps/)
hashing its inputs: the importer sources, the packs it reads, the pinned
decompiler and the v20 file listing. A rerun rebuilds packs that are missing,
unfinished or stale, and everything built from them, and keeps the rest.
Pack folders without a stamp (copied in from a playtest package or an older
checkout) are kept as they are; delete one to rebuild it.
"""
import argparse
import hashlib
import json
import os
import pathlib
import re
import shutil
import struct
import subprocess
import sys
import time
import urllib.request
import zipfile

REPO = pathlib.Path(__file__).resolve().parents[1]
EXE = '.exe' if os.name == 'nt' else ''

# Pinned decompiler inputs (docs/research/second-pass-audit.md).
DSO_SHARP_TAG = '2.1.0'
DSO_SHARP_COMMIT = '965aa93bfb1256688f61faa4f97cdc00583ae775'
DSO_SHARP_URL = 'https://github.com/Elletra/dso-sharp.git'
DSO_SHARP_EXE_URL = 'https://github.com/Elletra/dso-sharp/releases/download/2.1.0/dso-sharp.exe'
DSO_SHARP_EXE_SHA256 = 'a17b7a035bc21cc6391c0e9117748b23e141e7aa7f0e7b84651a9b0491bd6c46'
BL_DECOMPILED_URL = 'https://github.com/Elletra/bl-decompiled.git'
BL_DECOMPILED_COMMIT = 'b519133d89ff68768abcfe70ce38c958d6f5bf5b'
DECOMPILE_PIN = f'dso-sharp {DSO_SHARP_COMMIT}; bl-decompiled {BL_DECOMPILED_COMMIT}; -g blv20 -X'

# Brick add-ons whose literal datablocks join the stock catalog.
BRICK_ADDONS = ['Brick_Large_Cubes', 'Brick_Arch', 'Brick_V15', 'Brick_Checkpoint',
                'Brick_Treasure_Chest', 'Brick_Halloween', 'Brick_Teledoor']

# Stock files the geometry pass rejects on purpose (docs/vanilla-reference.md):
# a brick fragment that is not a standalone BLB, and a DTS v18 editor marker.
KNOWN_GEOMETRY_FAILURES = {'base/data/bricks/rounds/1x1x5spike.blb',
                           'base/data/shapes/markers/octahedron.dts'}

RESEARCH = REPO / '.research'
DECOMPILED = RESEARCH / 'v20-dso'
CORE = DECOMPILED / 'server/scripts/allGameScripts-Vanilla.cs'
DAMAGE_TYPES = DECOMPILED / 'server/scripts/DamageTypes.cs'
BL_DECOMPILED = RESEARCH / 'bl-decompiled'
DECOMPILE_STAMP = RESEARCH / 'v20-dso.pin'

# The designated v20 reference's add-on archives (docs/vanilla-reference.md).
# The geometry pass converts only these, so extra add-ons in someone's v20
# folder never change the shared base packs.
REFERENCE_INVENTORY = 'docs/vanilla-reference-inventory.json'
# Every file of the designated reference an importer may read, with its hash.
REFERENCE_FILES = 'docs/vanilla-reference-files.json'

WEAPON_EFFECTS = 'docs/research/weapon-effects/importer/Cargo.toml'
WEAPON_DEBRIS = 'docs/research/weapon-debris/importer/Cargo.toml'

# Step -> (packs it reads, importer sources, other repository inputs, recipe).
# Sources are ('cargo', package) including its local path dependencies, or
# ('files', path). Bump a recipe number when a step's command line changes.
CONVERT = [('cargo', 'bri-convert')]
STEP_INPUTS = {
    'geometry': ([], CONVERT, [REFERENCE_INVENTORY], 2),
    'brick_catalog': (['geometry'], CONVERT, [], 1),
    'map_bundle': (['geometry'], CONVERT, [], 1),
    'ui_pack': (['brick_catalog'], [('cargo', 'bri-ui-import')], [], 1),
    'brick_materials': ([], CONVERT, ['docs/vanilla-inventory.json'], 1),
    'avatar': (['geometry', 'ui_pack'], CONVERT, [], 1),
    'effects': ([], CONVERT, ['docs/vanilla-inventory.json'], 1),
    'weapons': ([], [('cargo', 'bri-weapons-import')], [], 1),
    'weapon_debris': (['weapons'], [('manifest', WEAPON_DEBRIS)], [], 1),
    'effects_runtime': (['effects', 'weapons'],
                        [('cargo', 'bri-fx-import'), ('manifest', WEAPON_EFFECTS)], [], 1),
    'item_presentation': (['weapons', 'ui_pack', 'avatar'], [('files', 'docs/research/item-rendering/build_presentation.py')], [], 1),
    'audio': ([], [('cargo', 'bri-audio-import')], [], 1),
    'vehicles': (['geometry'], [('cargo', 'bri-vehicles-import')], [], 1),
    'events': ([], [('files', 'crates/events-import')], [], 1),
    'weather': (['map_bundle'], [('cargo', 'bri-weather-import')], [], 1),
    'foliage': (['map_bundle'], [('cargo', 'bri-foliage-import')], [], 1),
    'worlds': (['brick_catalog', 'effects', 'events', 'audio', 'weapons', 'vehicles'], CONVERT, [], 1),
    'tutorial': (['brick_catalog', 'effects', 'events', 'audio', 'weapons', 'vehicles'], CONVERT, [], 2),
}
PACK_STEPS = list(STEP_INPUTS)
STEPS = ['decompile', 'build_tools', *PACK_STEPS, 'modern_lighting', 'check']

# Workspace packages each pack step runs (bri-client is always built for the check).
STEP_CARGO = {
    'geometry': ['bri-convert'], 'brick_catalog': ['bri-convert'], 'map_bundle': ['bri-convert'],
    'ui_pack': ['bri-ui-import'], 'brick_materials': ['bri-convert'], 'avatar': ['bri-convert'],
    'effects': ['bri-convert'], 'weapons': ['bri-weapons-import'], 'effects_runtime': ['bri-fx-import'],
    'audio': ['bri-audio-import'], 'vehicles': ['bri-vehicles-import'], 'weather': ['bri-weather-import'],
    'foliage': ['bri-foliage-import'], 'worlds': ['bri-convert'], 'tutorial': ['bri-convert'],
}
STEP_MANIFESTS = {'weapon_debris': [WEAPON_DEBRIS], 'effects_runtime': [WEAPON_EFFECTS]}

# Directories and files that never change what an importer produces.
SKIP_DIRS = {'target', 'tests', 'benches', 'examples', '__pycache__', '.git'}


def log(message):
    print(f'== {message}', flush=True)


def fail(message):
    sys.exit(f'\nerror: {message}')


def portable(part):
    """An argument as importers should see it: repository paths relative to the
    repository, so packs that record where their evidence came from read the
    same on every machine (the command itself stays absolute)."""
    if isinstance(part, pathlib.Path):
        try:
            return part.resolve().relative_to(REPO).as_posix()
        except ValueError:
            pass
    return str(part)


def run(*command, cwd=REPO, stdin=None):
    command = [str(command[0])] + [portable(part) for part in command[1:]] if cwd == REPO \
        else [str(part) for part in command]
    print('  $ ' + ' '.join(f'"{c}"' if ' ' in c else c for c in command), flush=True)
    try:
        subprocess.run(command, cwd=cwd, check=True, input=stdin)
    except FileNotFoundError:
        fail(f'{command[0]} was not found. Run tools/bootstrap.py to check the toolchain.')
    except subprocess.CalledProcessError as error:
        fail(f'{pathlib.Path(command[0]).name} exited with status {error.returncode}.')


def pack_names():
    """Role -> pack directory for every base package the game loads."""
    listing = json.loads((REPO / 'crates/package/base-packages.json').read_text(encoding='utf-8'))
    names = {p['role']: p['dir'] for p in listing.get('packages', []) if p.get('role')}
    if not names:
        fail('crates/package/base-packages.json lists no packages')
    return names


def sha256(*parts):
    digest = hashlib.sha256()
    for part in parts:
        digest.update(part if isinstance(part, bytes) else str(part).encode())
        digest.update(b'\0')
    return digest.hexdigest()


def tree_digest(root):
    """Content hash of a source file or directory, line endings normalized so a
    Windows checkout and a Linux one agree."""
    root = pathlib.Path(root)
    files = [root] if root.is_file() else sorted(
        p for p in root.rglob('*')
        if p.is_file() and not SKIP_DIRS.intersection(p.relative_to(root).parts[:-1])
        and p.suffix != '.md' and p.name != 'Cargo.lock')
    digest = hashlib.sha256()
    for path in files:
        digest.update(path.relative_to(root).as_posix().encode() + b'\0')
        digest.update(path.read_bytes().replace(b'\r\n', b'\n') + b'\0')
    return digest.hexdigest()


def v20_identity(v20):
    """The v20 files the importers read, by relative path and size. Lighting
    caches (.ml) and config/ change when someone plays and are not inputs."""
    entries = []
    for top in ('base', 'Add-Ons', 'saves'):
        for path in sorted((v20 / top).rglob('*')):
            if path.is_file() and path.suffix.lower() != '.ml':
                entries.append(f'{path.relative_to(v20).as_posix()}:{path.stat().st_size}')
    return sha256(*entries)


def without_lighting_caches(data):
    """A zip without its `.ml` members, every other byte kept, or None when it
    is not a readable zip. v20 adds a mission-lighting cache to a map's zip the
    first time the map is played; removing it restores the shipped archive."""
    end = data.rfind(b'PK\x05\x06')
    if end < 0 or end + 22 > len(data):
        return None
    try:
        disk, cd_disk, _, count, _, cd_offset, comment_len = struct.unpack_from('<HHHHIIH', data, end + 4)
        entries, at = [], cd_offset
        for _ in range(count):
            if data[at:at + 4] != b'PK\x01\x02':
                return None
            name_len, extra_len, note_len = struct.unpack_from('<HHH', data, at + 28)
            offset, = struct.unpack_from('<I', data, at + 42)
            size = 46 + name_len + extra_len + note_len
            entries.append((data[at + 46:at + 46 + name_len], offset, data[at:at + size]))
            at += size
    except struct.error:
        return None
    kept = [e for e in entries if not e[0].lower().endswith(b'.ml')]
    if len(kept) == len(entries):
        return data
    starts = sorted(e[1] for e in entries) + [cd_offset]
    out, central = bytearray(), bytearray()
    for _, offset, record in kept:
        following = starts[starts.index(offset) + 1]
        central += record[:42] + struct.pack('<I', len(out)) + record[46:]
        out += data[offset:following]
    cd_start = len(out)
    out += central
    out += struct.pack('<IHHHHIIH', 0x06054b50, disk, cd_disk, len(kept), len(kept), len(central),
                       cd_start, comment_len)
    out += data[end + 22:end + 22 + comment_len]
    return bytes(out)


def reference_view(v20, view):
    """Copy the designated reference's files from `v20` into `view`, checking
    each against its SHA-256, and return `view`. Every importer reads the view,
    so the base packs are the same on every machine whatever else a v20 folder
    holds: extra add-ons, saves, caches or a changed file never reach them.
    Lookups ignore case, as Windows and Torque do."""
    listing = json.loads((REPO / REFERENCE_FILES).read_text(encoding='utf-8'))['files']
    present = {}
    for top in ('base', 'Add-Ons', 'saves'):
        for path in (v20 / top).rglob('*'):
            if path.is_file():
                present.setdefault(path.relative_to(v20).as_posix().lower(), path)
    missing, changed, contents = [], [], {}
    for entry in listing:
        source = present.get(entry['path'].lower())
        if source is None:
            missing.append(entry['path'])
            continue
        data = source.read_bytes()
        if hashlib.sha256(data).hexdigest() != entry['sha256'] and entry['path'].lower().endswith('.zip'):
            data = without_lighting_caches(data) or data
        if len(data) != entry['bytes'] or hashlib.sha256(data).hexdigest() != entry['sha256']:
            changed.append(entry['path'])
        contents[entry['path']] = data
    if missing or changed:
        lines = [f'  missing: {p}' for p in missing[:10]] + [f'  changed: {p}' for p in changed[:10]]
        more = len(missing) + len(changed) - len(lines)
        fail(f'{v20} is not an unmodified Blockland v20: {len(missing)} file(s) are missing and '
             f'{len(changed)} differ from the reference (docs/vanilla-reference.md). Every player\'s '
             'base content must match, so restore these from an unmodified v20 install:\n'
             + '\n'.join(lines) + (f'\n  ...and {more} more' if more > 0 else ''))
    if view.exists():
        shutil.rmtree(view)
    for entry in listing:
        destination = view / entry['path']
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(contents[entry['path']])
    print(f'  {len(listing)} reference files from {v20}')
    return view


class Sources:
    """Fingerprints of importer sources, including local path dependencies."""

    def __init__(self):
        self.graphs = {}
        self.cache = {}

    def graph(self, manifest):
        if manifest not in self.graphs:
            result = subprocess.run(['cargo', 'metadata', '--format-version', '1', '--locked',
                                     '--manifest-path', str(REPO / manifest)],
                                    cwd=REPO, capture_output=True, text=True)
            if result.returncode:
                fail(f'cargo metadata failed for {manifest}:\n{result.stderr.strip()}\n'
                     'If the lock file is out of date, run '
                     f'`cargo metadata --format-version 1 --manifest-path {manifest} > {os.devnull}` '
                     'and commit the updated Cargo.lock.')
            meta = json.loads(result.stdout)
            local = {p['id']: p for p in meta['packages'] if p['source'] is None}
            deps = {node['id']: [d['pkg'] for d in node['deps']
                                 if any(k['kind'] != 'dev' for k in d['dep_kinds'])]
                    for node in meta['resolve']['nodes']}
            self.graphs[manifest] = (local, deps, meta['resolve']['root'])
        return self.graphs[manifest]

    def cargo(self, name, manifest='Cargo.toml'):
        local, deps, root = self.graph(manifest)
        start = root if name is None else next(
            (i for i, p in local.items() if p['name'] == name), None)
        if start is None:
            fail(f'No workspace package named {name}')
        seen, todo = set(), [start]
        while todo:
            current = todo.pop()
            if current in seen or current not in local:
                continue
            seen.add(current)
            todo.extend(deps.get(current, []))
        return sha256(*sorted(f'{local[i]["name"]}:{self.files(pathlib.Path(local[i]["manifest_path"]).parent)}'
                              for i in seen))

    def files(self, path):
        path = pathlib.Path(path).resolve()
        if path not in self.cache:
            self.cache[path] = tree_digest(path)
        return self.cache[path]

    def fingerprint(self, source):
        kind, value = source
        if kind == 'cargo':
            return self.cargo(value)
        if kind == 'manifest':
            return self.cargo(None, value)
        return self.files(REPO / value)


class Pipeline:
    def __init__(self, v20, content, rebuild_decompiled=False, keep_stale=False, force=()):
        self.v20 = v20
        self.content = content
        self.scratch = content / '_regeneration'
        self.stamps = self.scratch / 'stamps'
        self.packs = pack_names()
        self.rebuild_decompiled = rebuild_decompiled
        self.keep_stale = keep_stale
        self.force = set(force)
        self.release = REPO / 'target/release'
        self.plan = {}

    def pack(self, key):
        return self.content / self.packs[key]

    def bin(self, name):
        return self.release / (name + EXE)

    # --- Stamps and planning -------------------------------------------------

    def stamp_path(self, key):
        return self.stamps / f'{self.packs[key]}.json'

    def read_stamp(self, key):
        try:
            return json.loads(self.stamp_path(key).read_text(encoding='utf-8'))
        except (OSError, ValueError):
            return None

    def write_stamp(self, key, complete):
        self.stamps.mkdir(parents=True, exist_ok=True)
        self.stamp_path(key).write_text(json.dumps({
            'pack': self.packs[key], 'inputs': self.plan[key]['inputs'], 'complete': complete,
            'written': time.strftime('%Y-%m-%dT%H:%M:%S%z')}, indent=1) + '\n', encoding='utf-8')

    def make_plan(self, allowed):
        """Decide per pack: build, rebuild (stale or unfinished) or keep. A pack's
        inputs include the stamps of the packs it reads, so rebuilding one makes
        everything built from it stale too."""
        sources = Sources()
        v20 = sha256(v20_identity(self.v20), sources.files(REPO / REFERENCE_FILES))
        tokens = {}
        for key in PACK_STEPS:
            upstream, tools, files, recipe = STEP_INPUTS[key]
            inputs = sha256(key, recipe, v20, DECOMPILE_PIN,
                            *(sources.fingerprint(t) for t in tools),
                            *(sources.files(REPO / f) for f in files),
                            *(tokens[u] for u in upstream))
            path, stamp = self.pack(key), self.read_stamp(key)
            if not path.exists():
                state = 'missing'
            elif key in self.force:
                state = 'forced'
            elif stamp is None:
                state = 'unstamped'
            elif not stamp.get('complete'):
                state = 'unfinished'
            elif stamp.get('inputs') != inputs:
                state = 'stale'
            else:
                state = 'current'
            build = state in ('missing', 'forced', 'unfinished') or (state == 'stale' and not self.keep_stale)
            if key not in allowed:
                build = False
            if build or state == 'current':
                tokens[key] = inputs
            elif stamp and stamp.get('inputs'):
                tokens[key] = stamp['inputs']
            else:
                tokens[key] = f'unstamped:{self.packs[key]}'
            self.plan[key] = {'state': state, 'build': build, 'inputs': inputs}
        return self.plan

    def print_plan(self):
        notes = {'missing': 'missing, will build', 'forced': 'will rebuild (--rebuild)',
                 'unfinished': 'unfinished, will rebuild', 'stale': 'inputs changed, will rebuild',
                 'current': 'up to date', 'unstamped': 'kept (not built by this script)'}
        log(f'Content plan for {self.content}')
        for key in PACK_STEPS:
            entry = self.plan[key]
            note = notes[entry['state']]
            if entry['state'] == 'stale' and not entry['build']:
                note = 'inputs changed, kept (--keep-stale)'
            elif entry['state'] != 'current' and not entry['build'] and entry['state'] != 'unstamped':
                note = f'{entry["state"]}, not selected'
            print(f'  {self.packs[key]:<28} {note}')

    def begin(self, key):
        if not self.plan[key]['build']:
            return False
        path = self.pack(key)
        if path.exists():
            shutil.rmtree(path)
        self.write_stamp(key, complete=False)
        return True

    def finish(self, key):
        self.write_stamp(key, complete=True)

    def fresh_scratch(self, name):
        """A path for an intermediate output; it must not exist yet."""
        path = self.scratch / name
        if path.exists():
            shutil.rmtree(path)
        self.scratch.mkdir(parents=True, exist_ok=True)
        return path

    # --- Steps ---------------------------------------------------------------

    def decompile(self):
        pinned = DECOMPILE_STAMP.exists() and DECOMPILE_STAMP.read_text(encoding='utf-8').strip() == DECOMPILE_PIN
        unstamped = not DECOMPILE_STAMP.exists() and CORE.exists() and DAMAGE_TYPES.exists()
        if CORE.exists() and DAMAGE_TYPES.exists() and (pinned or unstamped) and not self.rebuild_decompiled:
            print(f'  {DECOMPILED} exists; kept')
        else:
            if DECOMPILED.exists():
                shutil.rmtree(DECOMPILED)
            base = self.v20 / 'base'
            dsos = sorted(base.rglob('*.dso'))
            if not dsos:
                fail(f'No .dso scripts under {base}')
            for source in dsos:
                destination = DECOMPILED / source.relative_to(base)
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, destination)
            # -X is command-line mode. Without it dso-sharp waits for a key
            # press, so a newline goes to stdin as well. -q only quiets logging.
            print(f'  decompiling {len(dsos)} scripts')
            run(*self.dso_sharp(), DECOMPILED, '-g', 'blv20', '-X', '-q', stdin=b'\n')
            if not CORE.exists():
                fail('dso-sharp did not produce allGameScripts-Vanilla.cs')
            DECOMPILE_STAMP.write_text(DECOMPILE_PIN + '\n', encoding='utf-8')
        if (BL_DECOMPILED / '.git').exists():
            current = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=BL_DECOMPILED,
                                     capture_output=True, text=True).stdout.strip()
            if current == BL_DECOMPILED_COMMIT:
                print(f'  {BL_DECOMPILED} at {BL_DECOMPILED_COMMIT[:7]}; kept')
                return
        if BL_DECOMPILED.exists():
            shutil.rmtree(BL_DECOMPILED)
        BL_DECOMPILED.mkdir(parents=True)
        run('git', 'init', '-q', cwd=BL_DECOMPILED)
        run('git', 'remote', 'add', 'origin', BL_DECOMPILED_URL, cwd=BL_DECOMPILED)
        run('git', 'fetch', '-q', '--depth', '1', 'origin', BL_DECOMPILED_COMMIT, cwd=BL_DECOMPILED)
        run('git', 'checkout', '-q', 'FETCH_HEAD', cwd=BL_DECOMPILED)

    def dso_sharp(self):
        """The pinned decompiler: the pinned release executable on Windows, or
        built from the pinned source with any .NET 8+ SDK elsewhere."""
        tools = RESEARCH / 'tools'
        if os.name == 'nt':
            exe = tools / f'dso-sharp-{DSO_SHARP_TAG}.exe'
            if not exe.exists():
                tools.mkdir(parents=True, exist_ok=True)
                print(f'  downloading {DSO_SHARP_EXE_URL}')
                data = urllib.request.urlopen(DSO_SHARP_EXE_URL).read()
                if hashlib.sha256(data).hexdigest() != DSO_SHARP_EXE_SHA256:
                    fail('dso-sharp download does not match the pinned SHA-256')
                exe.write_bytes(data)
            return [exe]
        dotnet = shutil.which('dotnet')
        if not dotnet or dotnet_sdk_major(dotnet) < 8:
            fail('Decompiling the v20 scripts needs a .NET 8 or newer SDK. ' + DOTNET_HINT)
        source = tools / f'dso-sharp-{DSO_SHARP_TAG}'
        if not (source / '.git').exists():
            if source.exists():
                shutil.rmtree(source)
            source.mkdir(parents=True)
            run('git', 'init', '-q', cwd=source)
            run('git', 'remote', 'add', 'origin', DSO_SHARP_URL, cwd=source)
            run('git', 'fetch', '-q', '--depth', '1', 'origin', DSO_SHARP_COMMIT, cwd=source)
            run('git', 'checkout', '-q', 'FETCH_HEAD', cwd=source)
        build = tools / f'dso-sharp-{DSO_SHARP_TAG}-build'
        dll = build / 'dso-sharp.dll'
        if not dll.exists():
            # A plain framework-dependent build: the project's native AOT publish
            # would also need clang/Xcode. RollForward lets a newer SDK run it.
            run(dotnet, 'build', source / 'DSO.csproj', '-c', 'Release', '-o', build,
                '-p:PublishAot=false', '-p:RollForward=Major', '-nologo')
        return [dotnet, dll]

    def build_tools(self):
        # Always the same package set: a smaller one changes cargo's feature
        # unification and recompiles much of the client even when nothing changed.
        packages = sorted({p for names in STEP_CARGO.values() for p in names})
        args = []
        for package in packages + ['bri-client', 'bri-render']:
            args += ['-p', package]
        run('cargo', 'build', '--release', '--locked', *args)
        manifests = sorted({m for key in PACK_STEPS if self.plan[key]['build'] for m in STEP_MANIFESTS.get(key, [])})
        for manifest in manifests:
            run('cargo', 'build', '--release', '--locked', '--manifest-path', REPO / manifest,
                '--target-dir', REPO / 'target')

    def geometry(self):
        out = self.pack('geometry')
        # The converter exits 1 whenever any source fails, after writing its
        # manifest. Two stock files are known, documented non-assets; failures
        # in add-ons that v20 does not ship are reported and skipped.
        result = subprocess.run([str(self.bin('bri-convert')), str(self.v20), str(out),
                                 '--reference', str(REPO / REFERENCE_INVENTORY)], cwd=REPO)
        manifest_path = out / 'manifest.json'
        if not manifest_path.exists():
            fail(f'bri-convert exited with status {result.returncode} and wrote no manifest')
        manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
        failed = {r['virtual_path'] for r in manifest['records'] if not r.get('output')}
        unexpected = sorted(p for p in failed if p.lower() not in KNOWN_GEOMETRY_FAILURES)
        stock = [p for p in unexpected if self.stock_path(p)]
        extra = [p for p in unexpected if not self.stock_path(p)]
        if extra:
            print(f'  note: {len(extra)} file(s) from non-stock add-ons did not convert and are skipped:')
            for path in extra[:20]:
                print(f'    {path}')
        if manifest['scan_errors'] or stock or (result.returncode and not failed):
            fail(f'Geometry conversion failed: {stock} {manifest["scan_errors"]}')

    def stock_path(self, path):
        """Whether a v20 path belongs to the stock game rather than an extra add-on."""
        parts = path.split('/')
        if parts[0].lower() != 'add-ons' or len(parts) < 2:
            return True
        name = parts[1].lower()
        if not hasattr(self, '_stock_addons'):
            listing = BL_DECOMPILED / 'v20/server/defaultAddOnList.cs'
            text = listing.read_text(encoding='utf-8', errors='replace') if listing.exists() else ''
            self._stock_addons = {n.lower() for n in re.findall(r'\$AddOn__(\w+)', text)}
            self._stock_addons |= {n.lower() for n in BRICK_ADDONS}
        return name.startswith('map_') or name in self._stock_addons

    def brick_catalog(self):
        addons = [self.v20 / 'Add-Ons' / f'{name}.zip' for name in BRICK_ADDONS]
        run(self.bin('stock_catalog'), CORE, self.pack('geometry'), self.pack('brick_catalog'), *addons)

    def map_bundle(self):
        manifest = json.loads((self.pack('geometry') / 'manifest.json').read_text(encoding='utf-8'))
        missions = sorted(r['virtual_path'] for r in manifest['records']
                          if re.fullmatch(r'Add-Ons/Map_[^/]+/[^/]+\.mis', r['virtual_path'], re.I)
                          and r.get('output'))
        if not missions:
            fail('The geometry pass converted no map missions')
        run(self.bin('map_bundle'), self.v20, self.pack('geometry'), self.pack('map_bundle'),
            '--core-script', CORE, *missions)

    def ui_pack(self):
        run(self.bin('bri-ui-import'), '--v20', self.v20, '--decompiled', DECOMPILED,
            '--stock-defaults', BL_DECOMPILED / 'v20/client/defaults.cs',
            '--brick-catalog', self.pack('brick_catalog') / 'stock-catalog.json', '--out', self.pack('ui_pack'))

    def brick_materials(self):
        run(self.bin('brick_material_bundle'), self.v20, BL_DECOMPILED / 'v20/server/defaultAddOnList.cs',
            REPO / 'docs/vanilla-inventory.json', self.pack('brick_materials'))

    def avatar(self):
        rig = self.fresh_scratch('avatar-rig')
        run(self.bin('avatar_bundle'), self.v20, self.pack('geometry'), CORE, rig)
        run(self.bin('avatar_material_bundle'), self.v20, rig, self.pack('ui_pack'), self.pack('avatar'))

    def effects(self):
        run(self.bin('effect_bundle'), self.v20, CORE, REPO / 'docs/vanilla-inventory.json', self.pack('effects'))

    def weapons(self):
        run(self.bin('bri-weapons-import'), self.v20, CORE, DAMAGE_TYPES, self.pack('weapons'))

    def weapon_debris(self):
        run(self.bin('bri-weapon-debris-import'), self.v20, self.pack('weapons') / 'weapons.json',
            self.pack('weapon_debris'))

    def effects_runtime(self):
        base = self.fresh_scratch('effects-runtime-base')
        run(self.bin('bri-fx-import'), self.v20, self.pack('effects'), CORE, base)
        run(self.bin('bri-weapon-effects-import'), self.v20, base, self.pack('weapons') / 'weapons.json',
            self.pack('effects_runtime'), CORE)

    def item_presentation(self):
        run(sys.executable, REPO / 'docs/research/item-rendering/build_presentation.py', '--repo', REPO,
            '--original', self.v20, '--weapons', self.pack('weapons'), '--ui', self.pack('ui_pack'),
            '--avatar', self.pack('avatar'), '--output', self.pack('item_presentation'))

    def audio(self):
        run(self.bin('bri-audio-import'), '--v20', self.v20, '--decompiled', DECOMPILED,
            '--out', self.pack('audio'))

    def vehicles(self):
        run(self.bin('bri-vehicles-import'), self.v20, self.pack('geometry'), self.pack('vehicles'))

    def events(self):
        run(sys.executable, REPO / 'crates/events-import/import_events.py', self.v20, CORE, self.pack('events'))

    def weather(self):
        run(self.bin('bri-weather-import'), self.v20, CORE, self.pack('map_bundle'), self.pack('weather'))

    def foliage(self):
        run(self.bin('bri-foliage-import'), self.v20, self.pack('map_bundle'), self.pack('foliage'))

    def bind(self, unbound, out):
        run(self.bin('bind_world_events'), unbound, self.pack('events') / 'catalog.json',
            self.pack('audio') / 'manifest.json', self.pack('weapons') / 'weapons.json',
            self.pack('vehicles') / 'vehicles.json', self.pack('effects') / 'effects.json', out)

    def worlds(self):
        unbound = self.fresh_scratch('worlds-unbound')
        run(self.bin('import_saves'), self.v20 / 'saves', self.pack('brick_catalog') / 'stock-catalog.json',
            unbound, self.pack('effects') / 'effects.json')
        self.bind(unbound, self.pack('worlds'))

    def tutorial(self):
        # import_saves names the map after each save's folder, as in saves/<Map>/.
        root = self.fresh_scratch('tutorial-saves')
        saves = root / 'Tutorial'
        saves.mkdir(parents=True)
        setup = self.scratch / 'targetSetup.txt'
        zips = {p.name.lower(): p for p in (self.v20 / 'Add-Ons').iterdir()}
        if 'map_tutorial.zip' not in zips:
            fail(f'{self.v20 / "Add-Ons"} has no Map_Tutorial.zip')
        with zipfile.ZipFile(zips['map_tutorial.zip']) as archive:
            for name in archive.namelist():
                if '/' in name:
                    continue
                if name.lower().endswith('.bls'):
                    (saves / name).write_bytes(archive.read(name))
                elif name.lower() == 'targetsetup.txt':
                    setup.write_bytes(archive.read(name))
        if not setup.exists():
            fail('Map_Tutorial.zip has no targetSetup.txt')
        unbound = self.fresh_scratch('tutorial-unbound')
        bound = self.fresh_scratch('tutorial-bound')
        run(self.bin('import_saves'), root, self.pack('brick_catalog') / 'stock-catalog.json',
            unbound, self.pack('effects') / 'effects.json')
        self.bind(unbound, bound)
        # The target models and their textures come from the archive itself.
        run(self.bin('tutorial_pack'), bound, setup, zips['map_tutorial.zip'], self.pack('tutorial'))

    def modern_lighting(self):
        # Copied/unstamped packs also need the derived modern-light descriptor.
        # The Rust preparer validates source identity and skips a fresh sidecar;
        # stale/missing data is recovered offline, never during Dynamic play.
        run(self.bin('prepare_lighting'), self.pack('map_bundle'))
        run(self.bin('prepare_lighting'), self.pack('map_bundle'), '--check')

    def check(self):
        run(self.bin('bri-client'), '--check', self.content)


TITLES = {
    'decompile': 'Recover v20 scripts', 'build_tools': 'Build converters and the client',
    'geometry': 'Geometry: terrain, bricks, shapes, animation, interiors, missions',
    'brick_catalog': 'Stock brick catalog and collision',
    'map_bundle': 'Maps: architecture, terrain, environment, water and baked lighting',
    'ui_pack': 'UI', 'brick_materials': 'Brick surfaces and prints', 'avatar': 'Avatar rig and customization',
    'effects': 'Particle and light effects', 'weapons': 'Weapons, tools and items',
    'weapon_debris': 'Weapon debris', 'effects_runtime': 'Runtime effects, including weapon effects',
    'item_presentation': 'Held and dropped item presentation', 'audio': 'Audio', 'vehicles': 'Vehicles',
    'events': 'Wrench events', 'weather': 'Weather', 'foliage': 'Foliage', 'worlds': 'Stock saves',
    'tutorial': 'Tutorial', 'modern_lighting': 'Modern live-light parameters (offline recovery)',
    'check': 'Startup validation (bri-client --check)',
}

DOTNET_HINT = {
    'darwin': 'Install it with `brew install --cask dotnet-sdk` (or from https://dot.net), then rerun.',
    'linux': 'Install it with `sudo apt install dotnet-sdk-8.0` (Debian/Ubuntu), '
             '`sudo pacman -S dotnet-sdk` (Arch) or `sudo dnf install dotnet-sdk-8.0` (Fedora), then rerun.',
}.get(sys.platform, 'Install it from https://dot.net, then rerun.')


def dotnet_sdk_major(dotnet):
    sdks = subprocess.run([dotnet, '--list-sdks'], capture_output=True, text=True).stdout
    majors = [int(line.split('.')[0]) for line in sdks.splitlines() if line[:1].isdigit()]
    return max(majors, default=0)


def valid_v20(path):
    return (path / 'base').is_dir() and (path / 'Add-Ons').is_dir() and (path / 'saves').is_dir()


def regenerate(v20, content, steps, rebuild_decompiled=False, keep_stale=False, force=()):
    """Plan and run the selected steps. Returns the pipeline."""
    content.mkdir(parents=True, exist_ok=True)
    override = content / 'packages.json'
    if override.exists():
        # It pins older pack names; the client would load those instead of the
        # packs built here. Keep it beside the content, out of the way.
        aside = content / 'packages.json.disabled'
        override.replace(aside)
        print(f'  moved {override.name} (an older pack selection) to {aside.name}; '
              'the client now loads the current default packs')
    log('Check the v20 folder against the reference')
    v20 = reference_view(v20, content / '_regeneration' / 'v20-reference')
    pipeline = Pipeline(v20, content, rebuild_decompiled, keep_stale, force)
    missing = set(PACK_STEPS) - set(pipeline.packs)
    if missing or set(pipeline.packs) - set(PACK_STEPS):
        fail(f'Pipeline steps and base packages disagree: {sorted(missing)} '
             f'{sorted(set(pipeline.packs) - set(PACK_STEPS))}')
    pipeline.make_plan({s for s in steps if s in PACK_STEPS})
    pipeline.print_plan()
    building = [s for s in PACK_STEPS if pipeline.plan[s]['build']]
    if building and 'item_presentation' in building:
        try:
            import PIL  # noqa: F401
        except ImportError:
            fail(f'The item presentation step needs Pillow: `{sys.executable} -m pip install pillow`')
    prepare_lighting = bool({'map_bundle', 'modern_lighting', 'check'} & set(steps))
    selected = [s for s in STEPS
                if (s in PACK_STEPS and s in building)
                or (s == 'decompile' and 'decompile' in steps and (building or rebuild_decompiled))
                or (s == 'build_tools' and (building or prepare_lighting))
                or (s == 'modern_lighting' and prepare_lighting)
                or (s == 'check' and 'check' in steps)]
    started = time.monotonic()
    for index, step in enumerate(selected, 1):
        log(f'[{index}/{len(selected)}] {TITLES[step]}')
        begun = time.monotonic()
        if step in PACK_STEPS:
            pipeline.begin(step)
            getattr(pipeline, step)()
            pipeline.finish(step)
        else:
            getattr(pipeline, step)()
        print(f'  ({time.monotonic() - begun:.0f}s)', flush=True)
    log(f'Done in {time.monotonic() - started:.0f}s')
    return pipeline


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--v20', type=pathlib.Path, required=True, help='Blockland v20 install (read only)')
    parser.add_argument('--content', type=pathlib.Path, default=REPO / 'content',
                        help='content directory to fill (default: <repo>/content)')
    parser.add_argument('--from', dest='start', choices=STEPS, help='start at this step')
    parser.add_argument('--only', choices=STEPS, action='append', help='run only these steps')
    parser.add_argument('--rebuild', choices=PACK_STEPS, action='append', default=[],
                        help='rebuild this pack even if it is up to date')
    parser.add_argument('--keep-stale', action='store_true', help='keep packs whose inputs changed')
    parser.add_argument('--plan', action='store_true', help='print what would be rebuilt and stop')
    parser.add_argument('--redecompile', action='store_true', help='decompile the scripts again')
    args = parser.parse_args()
    v20 = args.v20.resolve()
    if not valid_v20(v20):
        fail(f'{v20} is not a Blockland v20 install (needs base/, Add-Ons/ and saves/)')
    if args.only:
        steps = [s for s in STEPS if s in args.only]
    elif args.start:
        steps = STEPS[STEPS.index(args.start):]
    else:
        steps = STEPS
    if args.plan:
        view = reference_view(v20, args.content.resolve() / '_regeneration' / 'v20-reference')
        pipeline = Pipeline(view, args.content.resolve(), keep_stale=args.keep_stale, force=args.rebuild)
        pipeline.make_plan(set(steps))
        pipeline.print_plan()
        return
    regenerate(v20, args.content.resolve(), steps, args.redecompile, args.keep_stale, args.rebuild)


if __name__ == '__main__':
    main()
