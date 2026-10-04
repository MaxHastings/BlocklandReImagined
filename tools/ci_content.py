#!/usr/bin/env python3
"""The generated v20 content the release workflow builds with.

The content is generated from a v20 install (docs/content-regeneration.md) and
is never committed. GitHub Actions gets it from a draft release of this
repository named `ci-content`, holding one asset, `ci-content.zip`. Draft
releases are visible only to people with push access and to the workflow's
own token, so the zip stays private and needs no extra secret.

    python tools/ci_content.py pack      zip the packs the game loads from content/
    python tools/ci_content.py upload    pack, then put the zip on the draft release (needs gh)
    python tools/ci_content.py fetch     (CI) download the zip and unpack it into content/

Run `upload` once, and again whenever the content changes (after a bootstrap
run that rebuilt packs, or when crates/package/base-packages.json names a new
pack). The release workflow stops with a clear error when the zip is missing
or lacks a pack the game needs. The default Add-Ons are either our own,
committed under packages/, or bundled originals, which travel in their own
draft release (tools/addon_bundle.py).
"""
import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import urllib.error
import urllib.request
import zipfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import content_packs  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parent.parent
TAG = 'ci-content'
ASSET = 'ci-content.zip'
# GitHub's limit for one release asset.
ASSET_LIMIT = 2 * 1024 ** 3
SETUP_DOC = 'docs/release-builds.md'


def fail(message):
    # "::error::" makes GitHub Actions show the message on the run's summary.
    prefix = '::error::' if os.environ.get('GITHUB_ACTIONS') else ''
    sys.exit(f'{prefix}{message}')


def package_dirs(content):
    """The pack folders the game loads (content_packs.package_list), and
    whether content has its own list."""
    _, listing, own = content_packs.package_list(content)
    return [p['dir'] for p in listing['packages']], own


def pack(content, out):
    dirs, has_override = package_dirs(content)
    missing = [d for d in dirs if not (content / d).is_dir()]
    if missing:
        fail(f'{content} is missing {", ".join(missing)}. Run python tools/bootstrap.py first.')
    out.parent.mkdir(parents=True, exist_ok=True)
    head = subprocess.run(['git', 'rev-parse', '--short=9', 'HEAD'], cwd=REPO,
                          capture_output=True, text=True).stdout.strip()
    members = {}
    if has_override:
        members['packages.json'] = content / 'packages.json'
    for name in dirs:
        directory = content / name
        if directory.resolve() != content.resolve() and content.resolve() not in directory.resolve().parents:
            fail(f'Pack directory escapes content root: {name}')
        for path in sorted(directory.rglob('*')):
            if path.is_symlink():
                fail(f'Packs may not contain links: {path}')
            if path.is_file():
                members[path.relative_to(content).as_posix()] = path
    info = {'schema_version': 1, 'packs': dirs, 'packed_at_commit': head, 'files': {}}
    with zipfile.ZipFile(out, 'w', zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        for name, path in sorted(members.items()):
            # Hash exactly the bytes archived, even if a generator changes a file.
            data = path.read_bytes()
            archive.writestr(name, data)
            info['files'][name] = hashlib.sha256(data).hexdigest()
        archive.writestr('ci-content.json', json.dumps(info, indent=2) + '\n')
    files = len(members)
    verify(out)
    size = out.stat().st_size
    if size >= ASSET_LIMIT:
        fail(f'{out} is {size / 1024 ** 3:.2f} GiB, over GitHub\'s 2 GiB asset limit.')
    print(f'Packed {len(dirs)} packs, {files} files, into {out} ({size / 1024 ** 2:.0f} MiB).')
    return out


def upload(zip_path, tag=TAG, title='CI content (private, never publish)',
           notes='Generated v20 content for the release workflow. Keep this a draft. '
                 f'Refresh it with python tools/ci_content.py upload ({SETUP_DOC}).'):
    """Put zip_path on the draft release `tag`, replacing the asset of that name."""
    gh = shutil.which('gh')
    if gh is None:
        fail(f'gh (GitHub CLI) is not installed. Install it with "winget install GitHub.cli", run "gh auth login", '
             f'and rerun this; or upload {zip_path} by hand as {SETUP_DOC} describes.')
    run = lambda *args, **kw: subprocess.run([gh, *args], cwd=REPO, **kw)  # noqa: E731
    # ci-content and the bundled originals must remain private draft assets.
    release = run('release', 'view', tag, '--json', 'isDraft', capture_output=True, text=True)
    if release.returncode != 0:
        made = run('release', 'create', tag, '--draft', '--prerelease', '--title', title, '--notes', notes)
        if made.returncode != 0:
            fail(f'Could not create the {tag} draft release (is "gh auth login" done?).')
    elif not json.loads(release.stdout).get('isDraft'):
        fail(f'Refusing to upload private content: {tag} is published, not a draft.')
    if run('release', 'upload', tag, str(zip_path), '--clobber').returncode != 0:
        fail(f'Uploading {zip_path.name} failed.')
    print(f'Uploaded {zip_path.name} to the {tag} draft release. Release builds will use it.')


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def _api(url, token, accept='application/vnd.github+json'):
    request = urllib.request.Request(url, headers={
        'Authorization': f'Bearer {token}', 'Accept': accept,
        'X-GitHub-Api-Version': '2022-11-28', 'User-Agent': 'bri-release-workflow'})
    return urllib.request.build_opener(_NoRedirect).open(request, timeout=60)


def download(repo, token, zip_path, tag, asset_name, missing_help):
    """Download the asset `asset_name` of the draft release `tag` to zip_path."""
    asset = None
    page = 1
    while asset is None:
        with _api(f'https://api.github.com/repos/{repo}/releases?per_page=100&page={page}', token) as response:
            releases = json.load(response)
        if not releases:
            break
        for release in releases:
            if release.get('draft') and release.get('tag_name') == tag:
                asset = next((a for a in release.get('assets', []) if a['name'] == asset_name), None)
                if asset is None:
                    fail(missing_help)
                break
        page += 1
    if asset is None:
        fail(missing_help)
    print(f'Downloading {asset_name} ({asset["size"] / 1024 ** 2:.0f} MiB, updated {asset["updated_at"]}).')
    # The asset URL redirects to storage that must not see the token.
    try:
        response = _api(asset['url'], token, accept='application/octet-stream')
    except urllib.error.HTTPError as redirect:
        if redirect.code not in (301, 302, 307, 308):
            raise
        response = urllib.request.urlopen(redirect.headers['Location'], timeout=600)
    zip_path.parent.mkdir(parents=True, exist_ok=True)
    with response, open(zip_path, 'wb') as out:
        shutil.copyfileobj(response, out, 1024 * 1024)
    return zip_path


def verify(zip_path):
    """Validate every snapshot byte before installing it. Folder existence
    cannot detect an obsolete, incomplete pack with the same revision name."""
    with zipfile.ZipFile(zip_path) as archive:
        names = archive.namelist()
        for name in names:
            parts = pathlib.PurePosixPath(name)
            if parts.is_absolute() or '..' in parts.parts or '\\' in name or ':' in name:
                fail(f'{zip_path.name} holds an unsafe path: {name}')
        if len(set(names)) != len(names):
            fail(f'{zip_path.name} holds duplicate files.')
        info = json.loads(archive.read('ci-content.json'))
        if info.get('schema_version') != 1 or not isinstance(info.get('files'), dict):
            fail('The content snapshot has no file integrity manifest. Refresh it with ci_content.py upload.')
        expected = set(info['files']) | {'ci-content.json'}
        if set(names) != expected:
            fail(f'{zip_path.name} does not match its file manifest.')
        for name, digest in info['files'].items():
            if hashlib.sha256(archive.read(name)).hexdigest() != digest:
                fail(f'{zip_path.name} has changed or damaged content: {name}')
        for directory in info.get('packs', []):
            if not any(name.startswith(directory + '/') for name in info['files']):
                fail(f'{zip_path.name} has no files for pack {directory}.')
        return info


def extract(zip_path, root):
    """Unpack zip_path into root, refusing any member that would land outside it."""
    root.mkdir(parents=True, exist_ok=True)
    root = root.resolve()
    with zipfile.ZipFile(zip_path) as archive:
        for name in archive.namelist():
            target = (root / name).resolve()
            if target != root and root not in target.parents:
                fail(f'{zip_path.name} holds an unsafe path: {name}')
        archive.extractall(root)


def github_env():
    token = os.environ.get('GITHUB_TOKEN')
    repo = os.environ.get('GITHUB_REPOSITORY')
    if not token or not repo:
        fail('fetch runs in GitHub Actions: it needs GITHUB_TOKEN and GITHUB_REPOSITORY.')
    return repo, token


def fetch(content, repo, token):
    missing_help = (f'No private content for release builds: this repository needs a draft release named "{TAG}" '
                    f'holding {ASSET}. Max sets it up once from his PC with python tools/ci_content.py upload '
                    f'(see {SETUP_DOC}).')
    zip_path = download(repo, token, content.parent / ASSET, TAG, ASSET, missing_help)
    info = verify(zip_path)
    extract(zip_path, content)
    zip_path.unlink()
    dirs, _ = package_dirs(content)
    missing = [d for d in dirs if not (content / d).is_dir()]
    if missing:
        fail(f'The uploaded content predates this commit: it has no {", ".join(missing)}. '
             f'Rerun bootstrap on the PC, then python tools/ci_content.py upload ({SETUP_DOC}).')
    print(f'Unpacked {len(info["packs"])} packs (packed at commit {info.get("packed_at_commit") or "unknown"}).')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('command', choices=['pack', 'upload', 'fetch'])
    parser.add_argument('--content', type=pathlib.Path, default=REPO / 'content')
    parser.add_argument('--out', type=pathlib.Path, default=REPO / 'dist' / ASSET)
    args = parser.parse_args()
    if args.command == 'fetch':
        fetch(args.content, *github_env())
        return
    zip_path = pack(args.content, args.out)
    if args.command == 'upload':
        upload(zip_path)


if __name__ == '__main__':
    main()
