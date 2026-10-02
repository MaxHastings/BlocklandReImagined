#!/usr/bin/env bash
# Linux counterpart of package_playtest.ps1: the same release folder (the
# client, the Add-On importer, the dedicated server, every pack the package list selects, the
# default Add-Ons turned on, the tester docs and a checksummed manifest) plus
# the zip players download. The bundled original Add-Ons come from the
# Add-On bundle (tools/addon_bundle.py; dist/addon-bundle by default), with
# their CREDITS.md. As on Windows and Mac, the zip is the download, and
# launch.sh starts the game from the unzipped folder.
#
#   tools/package_playtest.sh --version 2026-10-02-a19 --sha256 <release-client-sha256> [--stress-lab]
#   tools/package_playtest.sh --validate-only [--stress-lab]
#   tools/package_playtest.sh --verify dist/BlocklandReImagined-<version>-linux
#   [--addon-bundle DIR] [--without-originals]   (the latter only for packaging tests)
#
# Build the client first with BRI_VERSION set to the version, as on Windows:
#   BRI_VERSION=<version> cargo build --release --locked -p bri-client --bin bri-client -p bri-addon-import --bin bri-import-addon -p bri-net --bin bri-server
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
executable="$repo/target/release/bri-client"
importer=""
destination="$repo/dist"
version="" expected="" validate_only=0 verify="" stress_lab=0 skip_version_check=0
addon_bundle="$repo/dist/addon-bundle" without_originals=0

die() { echo "package_playtest: $*" >&2; exit 1; }
while [[ $# -gt 0 ]]; do
    case "$1" in
        --version) version="$2"; shift 2 ;;
        --sha256) expected="$2"; shift 2 ;;
        --executable) executable="$2"; shift 2 ;;
        --importer) importer="$2"; shift 2 ;;
        --destination) destination="$2"; shift 2 ;;
        --stress-lab) stress_lab=1; shift ;;
        --addon-bundle) addon_bundle="$2"; shift 2 ;;
        --without-originals) without_originals=1; shift ;;
        --validate-only) validate_only=1; shift ;;
        --verify) verify="$2"; shift 2 ;;
        # Packaging tests use a stand-in executable that cannot report a version.
        --skip-version-check) skip_version_check=1; shift ;;
        -h|--help) sed -n '2,15p' "$0"; exit 0 ;;
        *) die "unknown argument $1" ;;
    esac
done
command -v python3 >/dev/null || die "python3 is required"

export BRI_REPO="$repo" BRI_EXECUTABLE="$executable" BRI_IMPORTER="${importer:-$(dirname "$executable")/bri-import-addon}" BRI_SERVER="$(dirname "$executable")/bri-server"
export BRI_DESTINATION="$destination" BRI_VERSION_ARG="$version" BRI_EXPECTED="$expected"
export BRI_ADDON_BUNDLE="$addon_bundle" BRI_WITHOUT_ORIGINALS="$without_originals"
export BRI_VALIDATE_ONLY="$validate_only" BRI_VERIFY="$verify" BRI_STRESS_LAB="$stress_lab" BRI_SKIP_VERSION_CHECK="$skip_version_check"

exec python3 - <<'PY'
import hashlib, json, os, pathlib, re, shutil, stat, subprocess, sys, zipfile

env = os.environ
repo = pathlib.Path(env['BRI_REPO'])
sys.path.insert(0, str(repo / 'tools'))
import addon_bundle  # noqa: E402
import content_packs  # noqa: E402
bundle = pathlib.Path(env['BRI_ADDON_BUNDLE']).resolve()
without_originals = env['BRI_WITHOUT_ORIGINALS'] == '1'
FIELDS = ['map_bundle', 'brick_catalog', 'geometry', 'effects', 'worlds', 'ui_pack', 'brick_materials', 'avatar',
          'effects_runtime', 'audio', 'weather', 'foliage', 'weapons', 'item_presentation', 'weapon_debris',
          'vehicles', 'events', 'tutorial']
EXECUTABLES = ('bri-client', 'bri-import-addon', 'bri-server', 'launch.sh')
VERSION = re.compile(r'^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$')


def die(message):
    sys.exit(f'package_playtest: {message}')


def no_links(path):
    for item in [path, *path.rglob('*')]:
        if item.is_symlink():
            die(f'package inputs may not contain symbolic links: {item}')


def files_under(path):
    no_links(path)
    return sorted((p for p in path.rglob('*') if p.is_file()), key=lambda p: p.relative_to(path).as_posix())


def read_list(path):
    if path.is_symlink() or path.stat().st_size > 1024 * 1024:
        die(f'{path} must be a regular file under 1 MiB')
    listing = json.loads(path.read_text(encoding='utf-8'))
    if listing.get('schema_version') != 1:
        die(f'unsupported package list schema version in {path}')
    return listing


def effective_packages():
    """The packs content/ loads (content_packs.package_list)."""
    path, _, _ = content_packs.package_list(repo / 'content', repo)
    listing = read_list(path)
    roles = set()
    for package in listing['packages']:
        name = str(package.get('dir') or '')
        parts = name.split('/')
        if not name or '\\' in name or ':' in name or any(p in ('', '.', '..') or p.endswith((' ', '.')) for p in parts):
            die(f"unsafe or empty package directory for '{package.get('id')}': {name}")
        if package.get('role'):
            roles.add(package['role'])
    for field in FIELDS:
        if field not in roles:
            die(f"{path} has no package for the '{field}' role")
    return listing


SERVER_KINDS = {'behaviour', 'script', 'world', 'entity', 'mode', 'archetype'}
CLIENT_KINDS = {'model', 'hud'}


def mod_package(directory, prefix):
    """The list entry of the Add-On in directory, carried to content/<prefix>/<id>.
    Same side rule as package_playtest.ps1 and bri_package::library."""
    manifest = json.loads((directory / 'package.json').read_text(encoding='utf-8'))
    kinds = [str(p.get('kind')) for p in (manifest.get('provides') or []) if p]
    # Client code follows the host (shared) unless it is personal.
    code = manifest.get('client')
    personal = isinstance(code, dict) and code.get('personal') is True
    if kinds and all(k in SERVER_KINDS for k in kinds):
        side = 'server'
    elif all(k in CLIENT_KINDS for k in kinds) and (personal or (code is None and kinds)):
        side = 'client'
    else:
        side = 'shared'
    return {'id': manifest['id'], 'version': manifest['version'], 'side': side, 'dir': f"{prefix}/{manifest['id']}",
            'path': directory}


def verify(root):
    root = pathlib.Path(root).resolve()
    manifest_path = root / 'MANIFEST.json'
    if not manifest_path.is_file():
        die(f'missing {manifest_path}')
    no_links(root)
    manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
    listed = [e['path'] for e in manifest['files']]
    if manifest.get('schema_version') != 1 or not listed or listed != sorted(set(listed)):
        die('invalid or unsorted package manifest')
    for entry in manifest['files']:
        path = entry['path']
        if '\\' in path or ':' in path or any(part in ('', '.', '..') for part in path.split('/')):
            die(f'unsafe manifest path: {path}')
        data = (root / path).read_bytes()
        if len(data) != entry['bytes'] or hashlib.sha256(data).hexdigest() != entry['sha256']:
            die(f'package checksum mismatch: {path}')
    actual = sorted(p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_file())
    actual = [p for p in actual if p != 'MANIFEST.json' and not p.startswith('logs/')]
    if actual != listed:
        die('package contains unlisted or missing files')
    for name in EXECUTABLES:
        if not os.access(root / name, os.X_OK):
            die(f'{name} is not executable')
    print(f"Verified {len(listed)} files for package version {manifest['version']}.")
    addon_bundle.verify_defaults(repo, root / 'content', root / addon_bundle.CREDITS, without_originals)


def write_zip(folder, zip_path):
    """The folder as one zip under its own name, sorted, keeping the executable bits."""
    top = folder.name
    with zipfile.ZipFile(zip_path, 'x', zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        entry = zipfile.ZipInfo(f'{top}/')
        entry.create_system = 3
        entry.external_attr = (stat.S_IFDIR | 0o755) << 16 | 0x10
        archive.writestr(entry, b'')
        for path in files_under(folder):
            relative = path.relative_to(folder).as_posix()
            entry = zipfile.ZipInfo.from_file(path, f'{top}/{relative}')
            entry.create_system = 3
            mode = 0o755 if relative in EXECUTABLES else 0o644
            entry.external_attr = (stat.S_IFREG | mode) << 16
            entry.compress_type = zipfile.ZIP_DEFLATED
            with open(path, 'rb') as source, archive.open(entry, 'w') as out:
                shutil.copyfileobj(source, out, 1024 * 1024)


if env['BRI_VERIFY']:
    verify(env['BRI_VERIFY'])
    sys.exit(0)

stress_lab = env['BRI_STRESS_LAB'] == '1'
executable = pathlib.Path(env['BRI_EXECUTABLE'])
importer = pathlib.Path(env['BRI_IMPORTER'])
server = pathlib.Path(env['BRI_SERVER'])
for path, what in ((executable, 'release client missing; build it first'),
                   (importer, 'Add-On importer missing; build it with the client (cargo build --release --locked -p bri-addon-import)'),
                   (server, 'dedicated server missing; build it with the client (cargo build --release --locked -p bri-net --bin bri-server)')):
    if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
        die(f'{what}: {path}')
sha = hashlib.sha256(executable.read_bytes()).hexdigest()

listing = effective_packages()
content = repo / 'content'
selected, content_files, content_bytes = [], 0, 0
for package in listing['packages']:
    directory = content / package['dir']
    if not directory.is_dir() or directory.is_symlink():
        die(f"selected {package['id']} package is missing or a link: {directory}")
    if content.resolve() not in directory.resolve().parents:
        die(f"selected package escapes the content root: {package['dir']}")
    files = files_under(directory)
    if not files:
        die(f"selected {package['id']} package is empty: {directory}")
    content_files += len(files)
    content_bytes += sum(f.stat().st_size for f in files)
    selected.append(package)

# The default Add-Ons every build ships (content/addons/<id>): our own from
# packages/, the bundled originals from the Add-On bundle, turned on unless
# the list carries one turned off; the Stress Lab ones join them with
# --stress-lab.
defaults, credits = addon_bundle.default_sources(repo, bundle, without_originals)
mods = [dict(mod_package(pathlib.Path(a['path']), 'addons'), enabled=a['enabled']) for a in defaults]
if stress_lab:
    found = sorted(d for d in (repo / 'packages/stresslab').iterdir() if (d / 'package.json').is_file())
    if not found:
        die('no Add-Ons found in packages/stresslab')
    mods += [mod_package(d, 'stresslab') for d in found]

docs = [('docs/PLAYTEST.md', 'PLAYTEST.md'), ('docs/KNOWN-ISSUES.md', 'KNOWN-ISSUES.md'),
        ('docs/TESTER-GUIDE.md', 'TESTER-GUIDE.md'), ('docs/FEATURES.md', 'FEATURES.md'),
        ('docs/DEDICATED-SERVER.md', 'DEDICATED-SERVER.md'),
        ('tools/launch_playtest.sh', 'launch.sh')]
if stress_lab:
    docs.append(('docs/stress-lab/PLAYTEST-STRESS-LAB.md', 'PLAYTEST-STRESS-LAB.md'))
for source, _ in docs:
    if not (repo / source).is_file():
        die(f'required package file missing: {source}')

if env['BRI_VALIDATE_ONLY'] == '1':
    print(json.dumps({'package_count': len(selected), 'content_files': content_files, 'content_bytes': content_bytes,
                      'client_sha256': sha, 'mod_packages': [m['dir'] for m in mods]}, indent=2))
    sys.exit(0)

version = env['BRI_VERSION_ARG']
if not VERSION.match(version):
    die('supply --version with 1-64 letters, digits, dot, underscore or dash')
if not re.fullmatch(r'[0-9A-Fa-f]{64}', env['BRI_EXPECTED']):
    die("supply --sha256 with the release client's expected SHA-256")
if sha != env['BRI_EXPECTED'].lower():
    die(f'release client hash differs from the expected build: {sha}')
if env['BRI_SKIP_VERSION_CHECK'] != '1':
    run = subprocess.run([str(executable), '--version'], capture_output=True, text=True)
    if run.returncode != 0:
        die(f'{executable} --version failed: {run.stderr.strip()}')
    reported = run.stdout.split()
    if not reported or reported[0] != version:
        die(f"the client reports version '{reported[0] if reported else ''}', not '{version}'. "
            f'Rebuild with BRI_VERSION={version} cargo build --release.')

destination = pathlib.Path(env['BRI_DESTINATION']).resolve()
destination.mkdir(parents=True, exist_ok=True)
release = destination / f"BlocklandReImagined-{version}-linux"
# One download name across versions (releases/latest/download/...);
# the folder inside carries the version.
zip_path = destination / 'BlocklandReImagined-linux.zip'
for existing in (release, zip_path):
    if existing.exists():
        die(f'refusing to overwrite {existing}')

try:
    (release / 'content').mkdir(parents=True)
    shutil.copyfile(executable, release / 'bri-client')
    shutil.copyfile(importer, release / 'bri-import-addon')
    # A dedicated server for a VPS, run from this folder (docs/dedicated-server.md).
    shutil.copyfile(server, release / 'bri-server')
    # Line tables stay in target/release for crash reports (the Windows .pdb
    # is kept apart the same way); players get the binaries without them.
    if shutil.which('strip'):
        subprocess.run(['strip', '--strip-debug', str(release / 'bri-client'), str(release / 'bri-import-addon'), str(release / 'bri-server')], check=True)
    for source, name in docs:
        shutil.copyfile(repo / source, release / name)
    import package_guides
    package_guides.copy_guides(repo, release)
    if credits:
        shutil.copyfile(credits, release / addon_bundle.CREDITS)
    for name in EXECUTABLES:
        (release / name).chmod(0o755)
    for package in selected:
        shutil.copytree(content / package['dir'], release / 'content' / package['dir'])
    packages = list(listing['packages'])
    for mod in mods:
        shutil.copytree(mod['path'], release / 'content' / mod['dir'])
        if not mod.get('enabled', True):
            continue
        packages.append({'id': mod['id'], 'version': mod['version'], 'side': mod['side'], 'dir': mod['dir']})
    (release / 'content/packages.json').write_text(
        json.dumps({'schema_version': listing['schema_version'], 'packages': packages}, indent=2) + '\n', encoding='utf-8')
    entries = []
    for path in files_under(release):
        data = path.read_bytes()
        entries.append({'path': path.relative_to(release).as_posix(), 'bytes': len(data),
                        'sha256': hashlib.sha256(data).hexdigest()})
    entries.sort(key=lambda e: e['path'])
    manifest = {'schema_version': 1, 'version': version, 'executable': 'bri-client',
                'content_config': 'content/packages.json', 'files': entries}
    (release / 'MANIFEST.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
    print(f'Created {release} ({content_files} content files, {content_bytes} bytes; client {executable.stat().st_size} bytes).')
    verify(release)
    write_zip(release, zip_path)
    print(f'Created {zip_path} ({zip_path.stat().st_size} bytes).')
except BaseException:
    if zip_path.exists():
        zip_path.unlink()
    if release.is_dir() and not release.is_symlink():
        shutil.rmtree(release)
    raise
PY
