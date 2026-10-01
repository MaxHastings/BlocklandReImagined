#!/usr/bin/env python3
"""Bundled originals: the classic community Add-Ons our releases carry.

packages/default-addons.json lists them ("original" entries) in load order,
with the authors the credits name and the sha256 of each copy pinned for
bundling. Their files never enter this public repository. This tool finds
Maxwell's own copies, imports each with Import Add-On (bri-import-addon, which
applies the port crates/addon-import/ports/ports.json lists for it), credits
its authors and packs the result into one private zip. The Windows release
workflow downloads that zip from a draft release, the way it gets the
generated v20 content (tools/ci_content.py, docs/release-builds.md); the
Mac and Linux ones take the originals back out of that Windows release,
as they take its base game, so every platform ships the same ones.

    python tools/addon_bundle.py find    [--search DIR]...  where each original is, its sha256 and port
    python tools/addon_bundle.py build   [--search DIR]... [--v20 DIR] [--importer EXE] [--content-root DIR] [--missing-ok]
    python tools/addon_bundle.py upload  [build options]    build, then replace the draft release's zip
    python tools/addon_bundle.py fetch                      (CI) download and unpack it into dist/addon-bundle
    python tools/addon_bundle.py from-release RELEASE_DIR   (CI) take it back out of an unpacked Windows release
    python tools/addon_bundle.py install [--content DIR]    put the bundle into a checkout's content/addons
    python tools/addon_bundle.py sources [--bundle DIR]     (packagers) every default Add-On's folder, as JSON
    python tools/addon_bundle.py verify-release CONTENT_DIR --credits FILE   (packagers) check a release

Searched, in order: every --search folder, $BRI_ADDON_SEARCH (folders split
like PATH), Blockland on Steam's Add-Ons, the v20 install's Add-Ons and
Maxwell's archive; each folder and the folders directly inside it.

Each copy is imported the way the Add-Ons screen's Import does it for a
player: against the game's installed content (--content-root, default the
checkout's content/, which tools/bootstrap.py generates), so base datablocks
an original inherits from or names resolve as they will in a release.

An original with no pinned copy yet is left out of the bundle and of every
release, and `upload` refuses to run until each is pinned (or
--allow-unpinned). To pin one, run `find`, check the copy it names is the
one to credit, and add its sha256 to the entry. To pull one from the next
release (its author asked), add "withdrawn": "<why>" to its entry.
"""
import argparse
import datetime
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile

REPO = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / 'tools'))
import ci_content  # noqa: E402
import content_packs  # noqa: E402

LIST_SCHEMA = 2
BUNDLE_SCHEMA = 1
TAG = 'addon-bundle'
ASSET = 'addon-bundle.zip'
BUNDLE = REPO / 'dist' / 'addon-bundle'
CREDITS = 'CREDITS.md'
ISSUES = 'https://github.com/MaxHastings/BlocklandReImagined/issues'
EXE = '.exe' if os.name == 'nt' else ''
STEAM_ADDONS = pathlib.Path('S:/SteamLibrary/steamapps/common/Blockland/Add-Ons')
ARCHIVE = pathlib.Path('C:/Users/Maxwell/Documents/_Blockland_Maxwell_1588_Archive/Addons')
CORE = [REPO / '.research/v20-dso/server/scripts' / name for name in ('allGameScripts-Vanilla.cs', 'DamageTypes.cs')]
SHA = re.compile(r'sha256 ([0-9a-f]{64})')


def fail(message):
    ci_content.fail(message)


# ---------------------------------------------------------------- the list


def read_list(repo=REPO):
    path = repo / 'packages' / 'default-addons.json'
    data = json.loads(path.read_text(encoding='utf-8'))
    if data.get('schema_version') != LIST_SCHEMA:
        fail(f'Unsupported schema in {path}: expected {LIST_SCHEMA}.')
    for addon in data['addons']:
        if ('path' in addon) == ('original' in addon):
            fail(f'{path}: {addon.get("id")} needs a path (our own) or an original, not both.')
    return data['addons']


def ships(addon):
    """Our own always; an original once a copy is pinned, until withdrawn
    (bri_package::defaults::DefaultAddOn::ships)."""
    original = addon.get('original')
    return original is None or (bool(original['sha256']) and not original.get('withdrawn'))


def originals(addons):
    return [a for a in addons if 'original' in a]


def namespace(addon):
    """The package id the importer gives an Add-On (bri_addon_import::namespace_for)."""
    out = ''
    for c in addon.lower():
        c = c if (c.isascii() and (c.isalnum() or c == '-')) else '_'
        if not (c == '_' and out.endswith('_')):
            out += c
    out = out.strip('_-')
    if not out[:1].isascii() or not out[:1].isalpha():
        out = f'addon_{out}'
    return out.rstrip('_-')


def ports_by_addon(repo=REPO):
    path = repo / 'crates/addon-import/ports/ports.json'
    return {p['addon'].lower(): p for p in json.loads(path.read_text(encoding='utf-8'))['ports']}


def pinned_sha(manifest):
    """The source sha256 an imported package.json records (provenance.source)."""
    found = SHA.search(json.dumps((manifest.get('provenance') or {}).get('source', '')))
    return found.group(1) if found else None


# ---------------------------------------------------------------- finding copies


def remembered_v20():
    try:
        return pathlib.Path((REPO / 'content' / '_regeneration' / 'v20-path.txt').read_text(encoding='utf-8').strip())
    except OSError:
        return None


def v20_folder(arg):
    v20 = arg or (pathlib.Path(os.environ['BRI_V20']) if os.environ.get('BRI_V20') else None) or remembered_v20()
    return v20 if v20 and (v20 / 'base').is_dir() else None


def search_roots(extra, v20):
    roots = list(extra)
    roots += [pathlib.Path(p) for p in os.environ.get('BRI_ADDON_SEARCH', '').split(os.pathsep) if p]
    roots += [STEAM_ADDONS]
    if v20:
        roots.append(v20 / 'Add-Ons')
    roots.append(ARCHIVE)
    seen, out = set(), []
    for root in roots:
        key = str(root).lower()
        if key not in seen and root.is_dir():
            seen.add(key)
            out.append(root)
    return out


def classic_addons(roots):
    """Every Add-On zip or folder in the roots and the folders directly inside them, by lower-case name."""
    found = {}

    def scan(folder):
        try:
            entries = sorted(folder.iterdir())
        except OSError:
            return []
        subfolders = []
        for entry in entries:
            if entry.is_symlink():
                continue
            if entry.suffix.lower() == '.zip' and entry.is_file():
                found.setdefault(entry.stem.lower(), []).append(entry)
            elif entry.is_dir():
                if (entry / 'server.cs').is_file() or (entry / 'description.txt').is_file():
                    found.setdefault(entry.name.lower(), []).append(entry)
                else:
                    subfolders.append(entry)
        return subfolders

    for root in roots:
        for sub in scan(root):
            scan(sub)
    return found


def importer_path(arg):
    if arg:
        return arg.resolve()
    target = pathlib.Path(os.environ.get('CARGO_TARGET_DIR', REPO / 'target'))
    importer = target / 'release' / f'bri-import-addon{EXE}'
    subprocess.run(['cargo', 'build', '--release', '--locked', '-p', 'bri-addon-import', '--bin', 'bri-import-addon'],
                   cwd=REPO, check=True)
    return importer.resolve()


def installed_content(args):
    """The game content the originals are imported against, as a player's
    Import passes it (--installed): a release's base datablocks."""
    content = args.content_root.resolve()
    missing = content_packs.missing_base_packs(content, args.repo)
    if missing:
        fail(f"No generated game content at {content} (it lacks {', '.join(missing)}): "
             'run python tools/bootstrap.py first or pass --content-root.')
    return content


def import_copy(importer, source, out, version, v20, core, installed):
    """Import one copy into out; its import report. Run beside the copy and
    name it bare, so the report records its name rather than a path on this PC."""
    command = [str(importer), source.name, str(out), '--version', version, '--installed', str(installed)]
    if v20:
        command += ['--reference', str(v20.resolve())]
    for script in core:
        command += ['--core', str(script.resolve())]
    run = subprocess.run(command, cwd=source.parent, capture_output=True, text=True)
    if run.returncode != 0:
        fail(f'Importing {source} failed:\n{run.stderr.strip() or run.stdout.strip()}')
    return json.loads((out / 'import-report.json').read_text(encoding='utf-8'))


def port_applied(report, port):
    return any(p.get('port') == port['port'] and p.get('applied') for p in report.get('ports', []))


# ---------------------------------------------------------------- commands


def find(args):
    v20 = v20_folder(args.v20)
    roots = search_roots(args.search, v20)
    print('Searching:\n  ' + '\n  '.join(map(str, roots)))
    available = classic_addons(roots)
    importer = importer_path(args.importer)
    ports = ports_by_addon(args.repo)
    core = [c for c in (args.core or CORE) if c.is_file()]
    installed = installed_content(args)
    with tempfile.TemporaryDirectory(prefix='bri-addon-find-') as work:
        for n, addon in enumerate(originals(read_list(args.repo))):
            original = addon['original']
            print(f"\n{addon['id']}: {original['title']} by {', '.join(original['authors'])} ({original['addon']})")
            if original.get('withdrawn'):
                print(f"  withdrawn: {original['withdrawn']}")
            copies = available.get(original['addon'].lower(), [])
            if not copies:
                close = [name for name in available if original['addon'].lower().split('_')[-1] in name]
                print('  no copy found' + (f"; similar names: {', '.join(sorted(close)[:8])}" if close else ''))
            for m, copy in enumerate(copies):
                report = import_copy(importer, copy, pathlib.Path(work) / f'{n}-{m}', original['version'], v20, core, installed)
                source = report['source']
                pinned = 'pinned' if source['sha256'] in original['sha256'] else 'NOT pinned'
                print(f"  {copy}\n    sha256 {source['sha256']} ({pinned})")
                print(f"    title {source.get('title')!r}, authors {source.get('authors')}, verdict {report['summary']['verdict']}")
                port = ports.get(original['addon'].lower())
                if port:
                    state = 'applied' if port_applied(report, port) else 'NOT applied'
                    print(f"    port {port['port']} ({port['status']}): {state}")
                else:
                    print(f"    no port listed; {report['summary']['needs_behaviour']} script functions need one")


def credits_text(entries):
    lines = [
        '# Bundled Add-On credits',
        '',
        'These classic Blockland Add-Ons come with Blockland ReImagined, credited to the people who made them. '
        'Each is the author\'s original, run through our Add-On importer; where a port is named, our own code '
        'adds the behaviour its scripts had.',
        '',
        f'If you made one of them and want it out, open an issue at {ISSUES} and it will be left out of the '
        'next release.',
        '',
    ]
    for entry in entries:
        port = f", port {entry['port']}" if entry.get('port') else ''
        state = 'on at start' if entry['enabled'] else 'installed, off at start'
        lines.append(f"- **{entry['title']}** by {', '.join(entry['authors'])} "
                     f"({entry['addon']}, sha256 {entry['sha256']}{port}; {state})")
    if not entries:
        lines.append('None in this release.')
    return '\n'.join(lines) + '\n'


def build(args):
    v20 = v20_folder(args.v20)
    if v20 is None:
        fail('Pass the Blockland v20 folder with --v20 (or set BRI_V20); the originals build on its base game.')
    core = args.core or CORE
    missing = [c for c in core if not c.is_file()]
    if missing:
        fail(f'Missing core scripts {", ".join(map(str, missing))}; run python tools/bootstrap.py first or pass --core.')
    installed = installed_content(args)
    roots = search_roots(args.search, v20)
    available = classic_addons(roots)
    importer = importer_path(args.importer)
    ports = ports_by_addon(args.repo)
    addons = read_list(args.repo)
    out = args.out.resolve()
    work = out.with_name(out.name + '.building')
    shutil.rmtree(work, ignore_errors=True)
    (work / 'addons').mkdir(parents=True)
    entries, unpinned = [], []
    for addon in originals(addons):
        original = addon['original']
        if original.get('withdrawn'):
            print(f"Left out {original['addon']}: withdrawn ({original['withdrawn']}).")
            continue
        if not original['sha256']:
            print(f"Left out {original['addon']}: no copy pinned yet (python tools/addon_bundle.py find).")
            unpinned.append(original['addon'])
            continue
        if namespace(original['addon']) != addon['id']:
            fail(f"{original['addon']} imports as {namespace(original['addon'])}, but the list names it {addon['id']}.")
        copies = available.get(original['addon'].lower(), [])
        chosen = None
        tried = []
        for n, copy in enumerate(copies):
            fresh = work / f".{addon['id']}-{n}"
            report = import_copy(importer, copy, fresh, original['version'], v20, core, installed)
            sha = report['source']['sha256']
            if sha in original['sha256']:
                chosen = (copy, fresh, report)
                break
            tried.append(f'{copy} (sha256 {sha})')
            shutil.rmtree(fresh)
        if chosen is None and not tried and args.missing_ok:
            print(f"Left out {original['addon']}: no copy on this machine.")
            unpinned.append(original['addon'])
            continue
        if chosen is None:
            found = '; '.join(tried) or 'no copy at all'
            fail(f"No pinned copy of {original['addon']} found in {', '.join(map(str, roots)) or 'any folder'}: {found}.")
        copy, fresh, report = chosen
        if report['package']['id'] != addon['id']:
            fail(f"{copy} imported as {report['package']['id']}, not {addon['id']}.")
        port = ports.get(original['addon'].lower())
        if port and not port_applied(report, port):
            reasons = [p.get('reason') for p in report.get('ports', []) if p.get('port') == port['port']]
            fail(f"The port {port['port']} did not apply to {copy}: {reasons}.")
        manifest_path = fresh / 'package.json'
        manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
        manifest['name'] = original['title']
        manifest['authors'] = original['authors']
        manifest.setdefault('provenance', {})['bundled'] = (
            f"The authors' original Add-On, bundled with Blockland ReImagined with credit to them. "
            f"To have it left out, open an issue at {ISSUES}.")
        manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + '\n', encoding='utf-8')
        fresh.rename(work / 'addons' / addon['id'])
        entries.append({'id': addon['id'], 'addon': original['addon'], 'title': original['title'],
                        'authors': original['authors'], 'version': original['version'],
                        'sha256': report['source']['sha256'], 'port': port['port'] if port else None,
                        'enabled': addon.get('enabled', True)})
        print(f"Bundled {original['addon']} from {copy}" + (f" with port {port['port']}." if port else ', no port.'))
    commit = subprocess.run(['git', 'rev-parse', '--short=9', 'HEAD'], cwd=args.repo,
                            capture_output=True, text=True).stdout.strip()
    (work / 'bundle.json').write_text(json.dumps({
        'schema_version': BUNDLE_SCHEMA, 'built_at_commit': commit or None,
        'built_at': datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'),
        'addons': entries}, indent=2) + '\n', encoding='utf-8')
    (work / CREDITS).write_text(credits_text(entries), encoding='utf-8')
    shutil.rmtree(out, ignore_errors=True)
    work.rename(out)
    zip_path = out.with_suffix('.zip')
    zip_path.unlink(missing_ok=True)
    with zipfile.ZipFile(zip_path, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in sorted(out.rglob('*')):
            if path.is_file():
                archive.write(path, path.relative_to(out).as_posix())
    print(f'Built {out} and {zip_path}: {len(entries)} originals.')
    return zip_path, unpinned


def read_bundle(bundle):
    path = bundle / 'bundle.json'
    if not path.is_file():
        return None
    data = json.loads(path.read_text(encoding='utf-8'))
    if data.get('schema_version') != BUNDLE_SCHEMA:
        fail(f'Unsupported bundle schema in {path}.')
    return data


def problems(directory, addon):
    """Why directory is not a whole copy of the default Add-On (empty when it is)."""
    manifest_path = directory / 'package.json'
    if not manifest_path.is_file():
        return [f'{directory} has no package.json']
    manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
    out = []
    if manifest.get('id') != addon['id']:
        out.append(f"{manifest_path} names {manifest.get('id')!r}, not {addon['id']!r}")
    original = addon.get('original')
    if original:
        sha = pinned_sha(manifest)
        if sha not in original['sha256']:
            out.append(f"{manifest_path} was imported from a copy of {original['addon']} the list does not pin "
                       f"(sha256 {sha})")
        if manifest.get('version') != original['version']:
            out.append(f"{manifest_path} is version {manifest.get('version')}, not {original['version']}")
        if manifest.get('authors') != original['authors']:
            out.append(f"{manifest_path} does not credit {', '.join(original['authors'])}")
    return out


def default_sources(repo, bundle, without_originals=False):
    """Every default Add-On that ships, in load order, with the folder it is
    copied from: packages/<path> for our own, the bundle's addons/<id> for an
    original; and the bundle's credits file when any original ships. Fails
    on one missing or incomplete. The release packagers all use this."""
    out = []
    for addon in read_list(repo):
        if not ships(addon):
            continue
        if 'original' in addon:
            if without_originals:
                continue
            directory = bundle / 'addons' / addon['id']
            if not directory.is_dir():
                fail(f"The Add-On bundle at {bundle} lacks {addon['original']['addon']} ({addon['id']}). "
                     f'Fetch it (python tools/addon_bundle.py fetch) or build it on the PC '
                     f'(python tools/addon_bundle.py build); see docs/release-builds.md.')
        else:
            directory = repo / 'packages' / addon['path']
        bad = problems(directory, addon)
        if bad:
            fail(f"Default Add-On {addon['id']} is missing or incomplete: {'; '.join(bad)}")
        out.append({'id': addon['id'], 'path': str(directory), 'enabled': addon.get('enabled', True),
                    'original': 'original' in addon})
    credits = bundle / CREDITS if any(a['original'] for a in out) else None
    if credits and not credits.is_file():
        fail(f'The Add-On bundle at {bundle} has no {CREDITS}.')
    return out, credits


def sources(args):
    addons, credits = default_sources(args.repo, args.bundle.resolve(), args.without_originals)
    print(json.dumps({'addons': addons, 'credits': str(credits) if credits else None}, indent=2))


def verify_defaults(repo, content, credits, without_originals=False):
    """A release's content turns on every default Add-On that starts on (at
    addons/<id>), carries the rest installed but off, each whole, and its
    credits name every original it carries."""
    enabled = json.loads((content / 'packages.json').read_text(encoding='utf-8-sig'))['packages']
    text = credits.read_text(encoding='utf-8') if credits and credits.is_file() else ''
    shipped = []
    for addon in read_list(repo):
        if not ships(addon) or ('original' in addon and without_originals):
            continue
        entry = [p for p in enabled if p.get('id') == addon['id']]
        if not addon.get('enabled', True):
            if entry:
                fail(f"The release turns on {addon['id']}, which ships turned off.")
        elif len(entry) != 1 or entry[0].get('dir') != f"addons/{addon['id']}":
            fail(f"The release does not turn on the default Add-On {addon['id']} at addons/{addon['id']}.")
        bad = problems(content / 'addons' / addon['id'], addon)
        if bad:
            fail(f"Default Add-On {addon['id']} is incomplete: {'; '.join(bad)}")
        if 'original' in addon:
            original = addon['original']
            if original['title'] not in text or not all(a in text for a in original['authors']):
                fail(f"The release's {CREDITS} does not credit {original['title']} to "
                     f"{', '.join(original['authors'])}.")
        shipped.append(addon['id'])
    print(f"Verified default Add-Ons: {', '.join(shipped)}.")


def verify_release(args):
    verify_defaults(args.repo, args.content.resolve(), args.credits, args.without_originals)


def install(args):
    """Copy the bundle's originals into a checkout's content/addons, where the
    game finds them like a release's; remove ones no longer bundled."""
    bundle = args.bundle.resolve()
    content = args.content.resolve()
    info = read_bundle(bundle)
    if info is None:
        fail(f'No Add-On bundle at {bundle}: build it first (python tools/addon_bundle.py build).')
    shipping = {a['id']: a for a in read_list(args.repo) if 'original' in a and ships(a)}
    installed = []
    for entry in info['addons']:
        addon = shipping.get(entry['id'])
        if addon is None:
            continue
        source = bundle / 'addons' / entry['id']
        bad = problems(source, addon)
        if bad:
            fail('; '.join(bad))
        target = content / 'addons' / entry['id']
        shutil.rmtree(target, ignore_errors=True)
        shutil.copytree(source, target)
        installed.append(entry['id'])
    # An original the list no longer ships (withdrawn) leaves the checkout too.
    for addon in originals(read_list(args.repo)):
        target = content / 'addons' / addon['id']
        if addon['id'] not in shipping and (target / 'package.json').is_file():
            manifest = json.loads((target / 'package.json').read_text(encoding='utf-8'))
            if 'bundled' in (manifest.get('provenance') or {}):
                shutil.rmtree(target)
                print(f"Removed {addon['id']}, which is no longer bundled.")
    missing = [a['original']['addon'] for i, a in shipping.items() if i not in installed]
    print(f"Installed {', '.join(installed) or 'no originals'} into {content / 'addons'}."
          + (f" Not in the bundle: {', '.join(missing)}; rebuild it." if missing else ''))


def fetch(args):
    repo, token = ci_content.github_env()
    help_text = (f'No Add-On bundle for release builds: this repository needs a draft release named "{TAG}" '
                 f'holding {ASSET}. The Gate builds and uploads it on the PC with python tools/addon_bundle.py '
                 'upload (see docs/release-builds.md).')
    out = args.bundle.resolve()
    zip_path = ci_content.download(repo, token, out.with_suffix('.zip'), TAG, ASSET, help_text)
    shutil.rmtree(out, ignore_errors=True)
    ci_content.extract(zip_path, out)
    zip_path.unlink()
    info = read_bundle(out)
    if info is None:
        fail(f'{ASSET} has no bundle.json.')
    bundled = {a['id'] for a in info['addons']}
    stale = [a['original']['addon'] for a in read_list(args.repo) if 'original' in a and ships(a) and a['id'] not in bundled]
    if stale:
        fail(f"The uploaded Add-On bundle predates this commit: it lacks {', '.join(stale)}. "
             'Rebuild and upload it on the PC with python tools/addon_bundle.py upload.')
    print(f"Unpacked {len(bundled)} originals (built at commit {info.get('built_at_commit') or 'unknown'}).")


def from_release(args):
    """The bundle a published release carries, taken back out of it: each
    shipping original from its content/addons/<id> and its CREDITS.md. The
    Mac and Linux release builds take it from the Windows zip they already
    take the base game's packs from, so every platform ships the same
    credited originals."""
    root = args.content.resolve()
    content = root / 'content'
    if not content.is_dir():
        fail(f'{root} is not an unpacked release: it has no content folder.')
    out = args.bundle.resolve()
    work = out.with_name(out.name + '.taking')
    shutil.rmtree(work, ignore_errors=True)
    (work / 'addons').mkdir(parents=True)
    ports = ports_by_addon(args.repo)
    entries = []
    for addon in read_list(args.repo):
        if 'original' not in addon or not ships(addon):
            continue
        original = addon['original']
        source = content / 'addons' / addon['id']
        bad = problems(source, addon)
        if bad:
            fail(f"The release at {root} lacks a whole {original['addon']}: {'; '.join(bad)}")
        shutil.copytree(source, work / 'addons' / addon['id'])
        manifest = json.loads((source / 'package.json').read_text(encoding='utf-8'))
        port = ports.get(original['addon'].lower())
        entries.append({'id': addon['id'], 'addon': original['addon'], 'title': original['title'],
                        'authors': original['authors'], 'version': original['version'],
                        'sha256': pinned_sha(manifest), 'port': port['port'] if port else None,
                        'enabled': addon.get('enabled', True)})
    if entries:
        credits = root / CREDITS
        if not credits.is_file():
            fail(f'The release at {root} carries originals but no {CREDITS}.')
        shutil.copyfile(credits, work / CREDITS)
    (work / 'bundle.json').write_text(json.dumps({
        'schema_version': BUNDLE_SCHEMA, 'taken_from_release': root.name, 'addons': entries}, indent=2) + '\n',
        encoding='utf-8')
    shutil.rmtree(out, ignore_errors=True)
    work.rename(out)
    print(f'Took {len(entries)} originals and their credits from {root} into {out}.')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('command', choices=['find', 'build', 'upload', 'fetch', 'from-release', 'install', 'sources',
                                            'verify-release'])
    parser.add_argument('content', nargs='?', type=pathlib.Path,
                        help='verify-release: the release content folder; from-release: the unpacked release')
    parser.add_argument('--search', type=pathlib.Path, action='append', default=[], help='a folder of Add-Ons')
    parser.add_argument('--v20', type=pathlib.Path)
    parser.add_argument('--core', type=pathlib.Path, action='append')
    parser.add_argument('--importer', type=pathlib.Path, help='a built bri-import-addon (default: build it)')
    parser.add_argument('--out', type=pathlib.Path, default=BUNDLE, help='build: the bundle folder')
    parser.add_argument('--bundle', type=pathlib.Path, default=BUNDLE)
    parser.add_argument('--repo', type=pathlib.Path, default=REPO, help='the checkout whose packages/default-addons.json to use')
    parser.add_argument('--content-root', dest='content_root', type=pathlib.Path, default=REPO / 'content',
                        help='the game content: find and build import against it, install fills it')
    parser.add_argument('--credits', type=pathlib.Path, help='verify-release: the release credits file')
    parser.add_argument('--without-originals', action='store_true',
                        help='sources, verify-release: a build without the bundle (packaging tests)')
    parser.add_argument('--allow-unpinned', action='store_true', help='upload: even if some original is unpinned')
    parser.add_argument('--missing-ok', action='store_true',
                        help='build: leave out an original with no copy here (bootstrap on a machine without them)')
    args = parser.parse_args()
    args.repo = args.repo.resolve()
    if args.command == 'find':
        find(args)
    elif args.command == 'build':
        build(args)
    elif args.command == 'upload':
        zip_path, unpinned = build(args)
        if unpinned and not args.allow_unpinned:
            fail(f"Not uploaded: no copy is pinned for {', '.join(unpinned)}, so the release would lack them. "
                 'Pin them (find), or pass --allow-unpinned to release without them.')
        ci_content.upload(zip_path, TAG, 'Add-On bundle (private, never publish)',
                          'The original classic Add-Ons the releases bundle, with credit. Keep this a draft. '
                          'Refresh it with python tools/addon_bundle.py upload (docs/release-builds.md).')
    elif args.command == 'fetch':
        fetch(args)
    elif args.command == 'from-release':
        if args.content is None:
            fail('from-release needs the unpacked release folder.')
        from_release(args)
    elif args.command == 'install':
        args.content = args.content_root
        install(args)
    elif args.command == 'sources':
        sources(args)
    else:
        if args.content is None:
            fail('verify-release needs the release content folder.')
        verify_release(args)


if __name__ == '__main__':
    main()
