#!/usr/bin/env python3
"""Publish a GitHub Release from zips a release.yml run already built.

    python tools/publish_release.py --version v0.2.11 --sha <commit> [--run <id>]
        [--notes docs/releases/v0.2.11.md] [--draft] [--wait]

The three platform zips come from one release.yml run (Windows, macOS and
Linux build side by side there). Without --run it finds the newest
successful run built at exactly --sha that still holds all three for
--version (with --wait, after any run still building there finishes); when
there is none it exits 3, and the caller builds instead.

Before anything is created it checks that every zip's MANIFEST.json names
--version and that the base content packs are byte-identical on all three
platforms. Then it creates the release as a draft at --sha (which creates the
tag when it does not exist), attaches the zips and SHA256SUMS, and publishes
it as the latest release unless --draft. A draft left by an earlier attempt
is reused; a published release of that version is never touched.

Runs in GitHub Actions (GITHUB_TOKEN/GH_TOKEN, GITHUB_REPOSITORY) or anywhere
`gh` is signed in.
"""
import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import zipfile

REPO = pathlib.Path(__file__).resolve().parents[1]
WORKFLOW = 'release.yml'
NO_RUN = 3
# Artifact name -> the zip inside it that players download.
PLATFORMS = {
    'windows': ('BlocklandReImagined-{version}-windows', 'BlocklandReImagined-windows.zip'),
    'macos': ('mac-release', 'BlocklandReImagined-macos.zip'),
    'linux': ('linux-release', 'BlocklandReImagined-linux.zip'),
}
BOILERPLATE = """**Windows:** download `BlocklandReImagined-windows.zip`, unzip it anywhere and run `Launch.cmd`. Windows may say "Windows protected your PC": click More info, then Run anyway. Settings and saves live in `%LOCALAPPDATA%\\BlocklandReImagined`, so a new version's folder picks them up.

**macOS** (Apple silicon): `BlocklandReImagined-macos.zip`. **Linux** (x86-64, glibc 2.35 or newer): `BlocklandReImagined-linux.zip`.

To play together, one player hosts and the others join by IP address. Hosting over the internet needs UDP port 28000 forwarded to the host when the router doesn't open it automatically.
"""


def fail(message):
    print(f'::error::{message}' if os.environ.get('GITHUB_ACTIONS') else f'error: {message}', file=sys.stderr)
    sys.exit(1)


def gh(*args, capture=True):
    result = subprocess.run(['gh', *args], capture_output=capture, text=True)
    if result.returncode:
        fail(f"gh {' '.join(args)} failed: {(result.stderr or '').strip()}")
    return result.stdout if capture else ''


def gh_json(*args):
    return json.loads(gh(*args) or 'null')


def repository():
    return os.environ.get('GITHUB_REPOSITORY') or gh('repo', 'view', '--json', 'nameWithOwner', '-q', '.nameWithOwner').strip()


def artifact_names(version):
    return {platform: name.format(version=version) for platform, (name, _) in PLATFORMS.items()}


def pick_run(runs, artifacts_of, version, sha):
    """The newest run in `runs` (API objects, any order) built at sha that
    succeeded and still holds every platform's artifact for version."""
    wanted = set(artifact_names(version).values())
    for run in sorted(runs, key=lambda r: r.get('created_at', ''), reverse=True):
        if run.get('head_sha') != sha or run.get('conclusion') != 'success':
            continue
        have = {a['name'] for a in artifacts_of(run['id']) if not a.get('expired')}
        if wanted <= have:
            return run['id']
    return None


def find_run(repo, version, sha):
    runs = gh_json('api', f'repos/{repo}/actions/workflows/{WORKFLOW}/runs?head_sha={sha}&status=success&per_page=50')
    return pick_run(runs.get('workflow_runs', []),
                    lambda run_id: gh_json('api', f'repos/{repo}/actions/runs/{run_id}/artifacts?per_page=100')['artifacts'],
                    version, sha)


def wait_for_builds(repo, sha):
    """Wait out any release.yml run still building at sha, so a tag pushed
    while the build-only run is finishing publishes its zips instead of
    starting a second build."""
    for status in ('in_progress', 'queued', 'waiting', 'pending'):
        runs = gh_json('api', f'repos/{repo}/actions/workflows/{WORKFLOW}/runs?head_sha={sha}&status={status}')
        for run in runs.get('workflow_runs', []):
            print(f"Waiting for run {run['id']} ({run.get('display_title', '')}) to finish.")
            subprocess.run(['gh', 'run', 'watch', str(run['id']), '--repo', repo, '--interval', '30'],
                           stdout=subprocess.DEVNULL)


def read_manifest(zip_path):
    """(release folder name, MANIFEST.json) of a release zip holding one folder."""
    with zipfile.ZipFile(zip_path) as archive:
        tops = {name.split('/', 1)[0] for name in archive.namelist() if name.strip('/')}
        if len(tops) != 1:
            fail(f'{zip_path.name} must hold exactly one release folder, not {sorted(tops)}')
        top = tops.pop()
        try:
            data = archive.read(f'{top}/MANIFEST.json')
        except KeyError:
            fail(f'{zip_path.name} has no {top}/MANIFEST.json')
    return top, json.loads(data.decode('utf-8-sig'))


def base_pack_files(manifest, pack_dirs):
    """{path under content/: sha256} of the base content packs a manifest lists."""
    root = manifest['content_config'].rsplit('/', 1)[0] + '/'
    files = {}
    for entry in manifest['files']:
        if not entry['path'].startswith(root):
            continue
        relative = entry['path'][len(root):]
        if relative.split('/', 1)[0] in pack_dirs:
            files[relative] = entry['sha256']
    return files


def compare_platforms(manifests, version, pack_dirs):
    """Problems (strings) across {platform: MANIFEST.json}: a wrong version, or
    base content that differs from Windows'."""
    problems = [f'{platform} zip is version {m.get("version")!r}, not {version!r}'
                for platform, m in manifests.items() if m.get('version') != version]
    reference = base_pack_files(manifests['windows'], pack_dirs)
    if not reference:
        problems.append('the Windows zip lists no base content packs')
    for platform, manifest in manifests.items():
        if platform == 'windows':
            continue
        files = base_pack_files(manifest, pack_dirs)
        missing = sorted(set(reference) - set(files))
        extra = sorted(set(files) - set(reference))
        changed = sorted(p for p in set(reference) & set(files) if reference[p] != files[p])
        for label, paths in (('lacks', missing), ('adds', extra), ('differs in', changed)):
            if paths:
                problems.append(f'{platform} {label} {len(paths)} base content file(s) against Windows, '
                                f'first {paths[0]}')
    return problems


def sha256_of(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as handle:
        for block in iter(lambda: handle.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def notes_text(version, notes):
    body = notes.read_text(encoding='utf-8').strip() + '\n\n' if notes and notes.is_file() else ''
    return body + BOILERPLATE


def existing_release(version):
    result = subprocess.run(['gh', 'release', 'view', version, '--json', 'isDraft,url'],
                            capture_output=True, text=True)
    return json.loads(result.stdout) if result.returncode == 0 else None


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--version', required=True)
    parser.add_argument('--sha', required=True, help='the commit the release tag names (full sha)')
    parser.add_argument('--run', help='release.yml run id holding the zips (default: find one built at --sha)')
    parser.add_argument('--notes', type=pathlib.Path, help='Markdown put above the download instructions')
    parser.add_argument('--draft', action='store_true', help='leave the release as a draft')
    parser.add_argument('--wait', action='store_true', help='first wait for release.yml runs still building at --sha')
    parser.add_argument('--out', type=pathlib.Path, default=REPO / 'dist' / 'publish')
    args = parser.parse_args()
    repo = repository()

    if args.wait and not args.run:
        wait_for_builds(repo, args.sha)
    run = args.run or find_run(repo, args.version, args.sha)
    if not run:
        print(f'No successful {WORKFLOW} run at {args.sha[:9]} holds the {args.version} zips.')
        sys.exit(NO_RUN)
    print(f'Taking the {args.version} zips from run {run}.')

    released = existing_release(args.version)
    if released and not released['isDraft']:
        fail(f'{args.version} is already published ({released["url"]}). Pick a new version.')

    shutil.rmtree(args.out, ignore_errors=True)
    zips, manifests = {}, {}
    for platform, artifact in artifact_names(args.version).items():
        folder = args.out / platform
        gh('run', 'download', str(run), '--repo', repo, '--name', artifact, '--dir', str(folder), capture=False)
        zip_path = folder / PLATFORMS[platform][1]
        if not zip_path.is_file():
            fail(f'artifact {artifact} of run {run} has no {zip_path.name}')
        zips[platform] = zip_path
        _, manifests[platform] = read_manifest(zip_path)
    packs = json.loads((REPO / 'crates/package/base-packages.json').read_text(encoding='utf-8'))['packages']
    problems = compare_platforms(manifests, args.version, {p['dir'] for p in packs})
    if problems:
        fail('; '.join(problems))
    print(f'All three zips are {args.version} and carry the same base content.')

    sums = args.out / 'SHA256SUMS'
    sums.write_text(''.join(f'{sha256_of(z)}  {z.name}\n' for z in zips.values()), encoding='utf-8')
    notes = args.out / 'notes.md'
    notes.write_text(notes_text(args.version, args.notes), encoding='utf-8')
    title = f'Blockland ReImagined {args.version} Alpha'
    assets = [str(z) for z in zips.values()] + [str(sums)]
    if released:
        print(f'Reusing the existing draft of {args.version}.')
        gh('release', 'upload', args.version, *assets, '--clobber', capture=False)
        gh('release', 'edit', args.version, '--title', title, '--notes-file', str(notes), capture=False)
    else:
        gh('release', 'create', args.version, *assets, '--draft', '--target', args.sha,
           '--title', title, '--notes-file', str(notes), capture=False)
    if not args.draft:
        gh('release', 'edit', args.version, '--draft=false', '--latest', capture=False)
    url = gh('release', 'view', args.version, '--json', 'url', '-q', '.url').strip()
    print(sums.read_text(encoding='utf-8'), end='')
    print(f"{'Drafted' if args.draft else 'Published'} {url}")


if __name__ == '__main__':
    main()
