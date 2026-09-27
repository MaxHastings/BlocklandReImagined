#!/usr/bin/env python3
"""Regenerate every native content pack the client loads from a v20 install.

One ordered, resumable pipeline from a read-only Blockland v20 folder to the
packs named by `ContentConfig::default` (crates/client/src/content.rs). Works
on Windows and Linux. See docs/content-regeneration.md.

    python tools/regenerate_content.py --v20 "/path/to/Blockland v20"

Existing pack directories are kept, so an interrupted run resumes where it
stopped. Delete a pack (and anything built from it) to rebuild it.
"""
import argparse
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import urllib.request
import zipfile

REPO = pathlib.Path(__file__).resolve().parents[1]
EXE = '.exe' if os.name == 'nt' else ''

# Pinned decompiler inputs (docs/research/second-pass-audit.md).
DSO_SHARP_TAG = '2.1.0'
DSO_SHARP_EXE_URL = 'https://github.com/Elletra/dso-sharp/releases/download/2.1.0/dso-sharp.exe'
DSO_SHARP_EXE_SHA256 = 'a17b7a035bc21cc6391c0e9117748b23e141e7aa7f0e7b84651a9b0491bd6c46'
BL_DECOMPILED_URL = 'https://github.com/Elletra/bl-decompiled.git'
BL_DECOMPILED_COMMIT = 'b519133d89ff68768abcfe70ce38c958d6f5bf5b'

# Brick add-ons whose literal datablocks join the stock catalog.
BRICK_ADDONS = ['Brick_Large_Cubes', 'Brick_Arch', 'Brick_V15', 'Brick_Checkpoint',
                'Brick_Treasure_Chest', 'Brick_Halloween', 'Brick_Teledoor']

# Stock files the geometry pass rejects on purpose (docs/vanilla-reference.md):
# a brick fragment that is not a standalone BLB, and a DTS v18 editor marker.
KNOWN_GEOMETRY_FAILURES = {'base/data/bricks/rounds/1x1x5spike.blb',
                           'base/data/shapes/markers/octahedron.dts'}

CORE = REPO / '.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs'
DAMAGE_TYPES = REPO / '.research/v20-dso/server/scripts/DamageTypes.cs'
BL_DECOMPILED = REPO / '.research/bl-decompiled'


def log(message):
    print(f'== {message}', flush=True)


def run(*command, cwd=REPO):
    command = [str(part) for part in command]
    print('  $ ' + ' '.join(f'"{c}"' if ' ' in c else c for c in command), flush=True)
    subprocess.run(command, cwd=cwd, check=True)


def pack_names():
    """The pack directory names the client loads, read from the source."""
    text = (REPO / 'crates/client/src/content.rs').read_text(encoding='utf-8')
    block = re.search(r'impl Default for ContentConfig \{.*?Self \{(.*?)\n\s*\}', text, re.S)
    if not block:
        sys.exit('Could not find ContentConfig::default in crates/client/src/content.rs')
    names = dict(re.findall(r'(\w+): "([^"]+)"\.into\(\)', block.group(1)))
    if not names:
        sys.exit('ContentConfig::default lists no packs')
    return names


class Pipeline:
    def __init__(self, v20, content, rebuild_decompiled):
        self.v20 = v20
        self.content = content
        self.scratch = content / '_regeneration'
        self.packs = pack_names()
        self.rebuild_decompiled = rebuild_decompiled
        self.release = REPO / 'target/release'

    def pack(self, key):
        return self.content / self.packs[key]

    def todo(self, key):
        path = self.pack(key)
        if path.exists():
            print(f'  {path.name} exists; kept')
            return False
        return True

    def fresh_scratch(self, name):
        """A path for an intermediate output; it must not exist yet."""
        path = self.scratch / name
        if path.exists():
            shutil.rmtree(path)
        self.scratch.mkdir(parents=True, exist_ok=True)
        return path

    def bin(self, name):
        return self.release / (name + EXE)

    # --- Steps ---------------------------------------------------------------

    def decompile(self):
        log('Recover v20 scripts')
        target = REPO / '.research/v20-dso'
        if CORE.exists() and DAMAGE_TYPES.exists() and not self.rebuild_decompiled:
            print(f'  {target} exists; kept')
        else:
            if target.exists():
                shutil.rmtree(target)
            base = self.v20 / 'base'
            dsos = sorted(base.rglob('*.dso'))
            if not dsos:
                sys.exit(f'No .dso scripts under {base}')
            for source in dsos:
                destination = target / source.relative_to(base)
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, destination)
            run(*self.dso_sharp(), target, '-g', 'blv20', '-X')
            if not CORE.exists():
                sys.exit('dso-sharp did not produce allGameScripts-Vanilla.cs')
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
        """The pinned decompiler: built from source with a .NET 8 SDK, or the
        pinned release executable on Windows."""
        tools = REPO / '.research/tools'
        dotnet = shutil.which('dotnet')
        if dotnet:
            sdks = subprocess.run([dotnet, '--list-sdks'], capture_output=True, text=True).stdout
            if any(int(line.split('.')[0]) >= 8 for line in sdks.splitlines() if line[:1].isdigit()):
                source = tools / f'dso-sharp-{DSO_SHARP_TAG}'
                if not source.exists():
                    run('git', 'clone', '-q', '--depth', '1', '--branch', DSO_SHARP_TAG,
                        'https://github.com/Elletra/dso-sharp.git', source)
                build = tools / f'dso-sharp-{DSO_SHARP_TAG}-build'
                run(dotnet, 'publish', source / 'DSO.csproj', '-c', 'Release', '-o', build)
                binary = next((p for p in build.iterdir() if p.stem == 'dso-sharp' and p.suffix in ('', '.exe')), None)
                if binary:
                    return [binary]
                return [dotnet, build / 'dso-sharp.dll']
        if os.name == 'nt':
            exe = tools / f'dso-sharp-{DSO_SHARP_TAG}.exe'
            if not exe.exists():
                tools.mkdir(parents=True, exist_ok=True)
                print(f'  downloading {DSO_SHARP_EXE_URL}')
                data = urllib.request.urlopen(DSO_SHARP_EXE_URL).read()
                if hashlib.sha256(data).hexdigest() != DSO_SHARP_EXE_SHA256:
                    sys.exit('dso-sharp download does not match the pinned SHA-256')
                exe.write_bytes(data)
            return [exe]
        sys.exit('dso-sharp needs a .NET 8 SDK on this platform (for example '
                 '`pacman -S dotnet-sdk` or `apt install dotnet-sdk-8.0`), then rerun.')

    def build_tools(self):
        log('Build converters')
        run('cargo', 'build', '--release', '--locked',
            '-p', 'bri-convert', '-p', 'bri-ui-import', '-p', 'bri-audio-import',
            '-p', 'bri-fx-import', '-p', 'bri-weapons-import', '-p', 'bri-weather-import',
            '-p', 'bri-foliage-import', '-p', 'bri-vehicles-import', '-p', 'bri-client')
        for manifest in ['docs/research/weapon-effects/importer/Cargo.toml',
                         'docs/research/weapon-debris/importer/Cargo.toml']:
            run('cargo', 'build', '--release', '--manifest-path', REPO / manifest,
                '--target-dir', REPO / 'target')

    def geometry(self):
        log('Geometry: terrain, bricks, shapes, animation, interiors, missions')
        if not self.todo('geometry'):
            return
        out = self.pack('geometry')
        # The converter exits 1 whenever any source fails, after writing its
        # manifest. Two stock files are known, documented non-assets.
        result = subprocess.run([str(self.bin('bri-convert')), str(self.v20), str(out)], cwd=REPO)
        manifest = json.loads((out / 'manifest.json').read_text(encoding='utf-8'))
        failed = {r['virtual_path'].lower() for r in manifest['records'] if not r.get('output')}
        unexpected = failed - KNOWN_GEOMETRY_FAILURES
        if manifest['scan_errors'] or unexpected or (result.returncode and not failed):
            shutil.rmtree(out)
            sys.exit(f'Geometry conversion failed: {sorted(unexpected)} {manifest["scan_errors"]}')

    def brick_catalog(self):
        log('Stock brick catalog and collision')
        if self.todo('brick_catalog'):
            addons = [self.v20 / 'Add-Ons' / f'{name}.zip' for name in BRICK_ADDONS]
            run(self.bin('stock_catalog'), CORE, self.pack('geometry'), self.pack('brick_catalog'), *addons)

    def map_bundle(self):
        log('Maps: architecture, terrain, environment, water and baked lighting')
        if not self.todo('map_bundle'):
            return
        manifest = json.loads((self.pack('geometry') / 'manifest.json').read_text(encoding='utf-8'))
        missions = sorted(r['virtual_path'] for r in manifest['records']
                          if re.fullmatch(r'Add-Ons/Map_[^/]+/[^/]+\.mis', r['virtual_path'], re.I)
                          and r.get('output'))
        if not missions:
            sys.exit('The geometry pass converted no map missions')
        run(self.bin('map_bundle'), self.v20, self.pack('geometry'), self.pack('map_bundle'),
            '--core-script', CORE, *missions)

    def ui_pack(self):
        log('UI')
        if self.todo('ui_pack'):
            run(self.bin('bri-ui-import'), '--v20', self.v20, '--decompiled', REPO / '.research/v20-dso',
                '--stock-defaults', BL_DECOMPILED / 'v20/client/defaults.cs',
                '--brick-catalog', self.pack('brick_catalog') / 'stock-catalog.json', '--out', self.pack('ui_pack'))

    def brick_materials(self):
        log('Brick surfaces and prints')
        if self.todo('brick_materials'):
            run(self.bin('brick_material_bundle'), self.v20, BL_DECOMPILED / 'v20/server/defaultAddOnList.cs',
                REPO / 'docs/vanilla-inventory.json', self.pack('brick_materials'))

    def avatar(self):
        log('Avatar rig and customization')
        if self.todo('avatar'):
            rig = self.fresh_scratch('avatar-rig')
            run(self.bin('avatar_bundle'), self.v20, self.pack('geometry'), CORE, rig)
            run(self.bin('avatar_material_bundle'), self.v20, rig, self.pack('ui_pack'), self.pack('avatar'))

    def effects(self):
        log('Particle and light effects')
        if self.todo('effects'):
            run(self.bin('effect_bundle'), self.v20, CORE, REPO / 'docs/vanilla-inventory.json', self.pack('effects'))

    def weapons(self):
        log('Weapons, tools and items')
        if self.todo('weapons'):
            run(self.bin('bri-weapons-import'), self.v20, CORE, DAMAGE_TYPES, self.pack('weapons'))

    def weapon_debris(self):
        log('Weapon debris')
        if self.todo('weapon_debris'):
            run(self.bin('bri-weapon-debris-import'), self.v20, self.pack('weapons') / 'weapons.json',
                self.pack('weapon_debris'))

    def effects_runtime(self):
        log('Runtime effects, including weapon effects')
        if self.todo('effects_runtime'):
            base = self.fresh_scratch('effects-runtime-base')
            run(self.bin('bri-fx-import'), self.v20, self.pack('effects'), CORE, base)
            run(self.bin('bri-weapon-effects-import'), self.v20, base, self.pack('weapons') / 'weapons.json',
                self.pack('effects_runtime'), CORE)

    def item_presentation(self):
        log('Held and dropped item presentation')
        if self.todo('item_presentation'):
            run(sys.executable, REPO / 'docs/research/item-rendering/build_presentation.py', '--repo', REPO,
                '--original', self.v20, '--weapons', self.pack('weapons').relative_to(REPO),
                '--output', self.pack('item_presentation').relative_to(REPO))

    def audio(self):
        log('Audio')
        if self.todo('audio'):
            run(self.bin('bri-audio-import'), '--v20', self.v20, '--decompiled', REPO / '.research/v20-dso',
                '--out', self.pack('audio'))

    def vehicles(self):
        log('Vehicles')
        if self.todo('vehicles'):
            run(self.bin('bri-vehicles-import'), self.v20, self.pack('geometry'), self.pack('vehicles'))

    def events(self):
        log('Wrench events')
        if self.todo('events'):
            run(sys.executable, REPO / 'crates/events-import/import_events.py', self.v20, CORE, self.pack('events'))

    def weather(self):
        log('Weather')
        if self.todo('weather'):
            run(self.bin('bri-weather-import'), self.v20, CORE, self.pack('map_bundle'), self.pack('weather'))

    def foliage(self):
        log('Foliage')
        if self.todo('foliage'):
            run(self.bin('bri-foliage-import'), self.v20, self.pack('map_bundle'), self.pack('foliage'))

    def bind(self, unbound, out):
        run(self.bin('bind_world_events'), unbound, self.pack('events') / 'catalog.json',
            self.pack('audio') / 'manifest.json', self.pack('weapons') / 'weapons.json',
            self.pack('vehicles') / 'vehicles.json', self.pack('effects') / 'effects.json', out)

    def worlds(self):
        log('Stock saves')
        if self.todo('worlds'):
            unbound = self.fresh_scratch('worlds-unbound')
            run(self.bin('import_saves'), self.v20 / 'saves', self.pack('brick_catalog') / 'stock-catalog.json',
                unbound, self.pack('effects') / 'effects.json')
            self.bind(unbound, self.pack('worlds'))

    def tutorial(self):
        log('Tutorial')
        if not self.todo('tutorial'):
            return
        # import_saves names the map after each save's folder, as in saves/<Map>/.
        root = self.fresh_scratch('tutorial-saves')
        saves = root / 'Tutorial'
        saves.mkdir(parents=True)
        setup = self.scratch / 'targetSetup.txt'
        with zipfile.ZipFile(self.v20 / 'Add-Ons/Map_Tutorial.zip') as archive:
            for name in archive.namelist():
                if '/' in name:
                    continue
                if name.lower().endswith('.bls'):
                    (saves / name).write_bytes(archive.read(name))
                elif name.lower() == 'targetsetup.txt':
                    setup.write_bytes(archive.read(name))
        if not setup.exists():
            sys.exit('Map_Tutorial.zip has no targetSetup.txt')
        unbound = self.fresh_scratch('tutorial-unbound')
        bound = self.fresh_scratch('tutorial-bound')
        run(self.bin('import_saves'), root, self.pack('brick_catalog') / 'stock-catalog.json',
            unbound, self.pack('effects') / 'effects.json')
        self.bind(unbound, bound)
        run(self.bin('tutorial_pack'), bound, setup, self.pack('tutorial'))

    def check(self):
        log('Startup validation')
        run(self.bin('bri-client'), '--check', self.content)


STEPS = ['decompile', 'build_tools', 'geometry', 'brick_catalog', 'map_bundle', 'ui_pack',
         'brick_materials', 'avatar', 'effects', 'weapons', 'weapon_debris', 'effects_runtime',
         'item_presentation', 'audio', 'vehicles', 'events', 'weather', 'foliage', 'worlds',
         'tutorial', 'check']


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--v20', type=pathlib.Path, required=True, help='Blockland v20 install (read only)')
    parser.add_argument('--from', dest='start', choices=STEPS, help='start at this step')
    parser.add_argument('--only', choices=STEPS, action='append', help='run only these steps')
    parser.add_argument('--redecompile', action='store_true', help='decompile the scripts again')
    args = parser.parse_args()
    v20 = args.v20.resolve()
    if not (v20 / 'base').is_dir() or not (v20 / 'Add-Ons').is_dir():
        sys.exit(f'{v20} is not a Blockland v20 install (needs base/ and Add-Ons/)')
    content = REPO / 'content'
    content.mkdir(exist_ok=True)
    pipeline = Pipeline(v20, content, args.redecompile)
    missing = set(STEPS) - {'decompile', 'build_tools', 'check'} - set(pipeline.packs)
    if missing or set(pipeline.packs) - set(STEPS):
        sys.exit(f'Pipeline steps and ContentConfig packs disagree: {sorted(missing)} '
                 f'{sorted(set(pipeline.packs) - set(STEPS))}')
    if args.only:
        steps = [s for s in STEPS if s in args.only]
    elif args.start:
        steps = STEPS[STEPS.index(args.start):]
    else:
        steps = STEPS
    # Every step after decompiling runs the release converters.
    if any(s != 'decompile' for s in steps) and 'build_tools' not in steps:
        steps.insert(1 if steps[0] == 'decompile' else 0, 'build_tools')
    for step in steps:
        getattr(pipeline, step)()
    log('Done')


if __name__ == '__main__':
    main()
