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
or lacks a pack the game needs. The default Add-Ons (the Duplicator, the
Stunt Plane) are committed under packages/, so they never need uploading.
"""
import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys
import urllib.error
import urllib.request
import zipfile

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
    """The pack folders the game loads: content/packages.json when present,
    otherwise the base game's list (the packager reads the same two)."""
    override = content / 'packages.json'
    listing = override if override.is_file() else REPO / 'crates/package/base-packages.json'
    packages = json.loads(listing.read_text(encoding='utf-8'))['packages']
    return [p['dir'] for p in packages], override.is_file()


def pack(content, out):
    dirs, has_override = package_dirs(content)
    missing = [d for d in dirs if not (content / d).is_dir()]
    if missing:
        fail(f'{content} is missing {", ".join(missing)}. Run python tools/bootstrap.py first.')
    out.parent.mkdir(parents=True, exist_ok=True)
    head = subprocess.run(['git', 'rev-parse', '--short=9', 'HEAD'], cwd=REPO,
                          capture_output=True, text=True).stdout.strip()
    files = 0
    with zipfile.ZipFile(out, 'w', zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        archive.writestr('ci-content.json', json.dumps({'packs': dirs, 'packed_at_commit': head}, indent=2) + '\n')
        if has_override:
            archive.write(content / 'packages.json', 'packages.json')
        for name in dirs:
            for path in sorted((content / name).rglob('*')):
                if path.is_symlink():
                    fail(f'Packs may not contain links: {path}')
                if path.is_file():
                    archive.write(path, path.relative_to(content).as_posix())
                    files += 1
    size = out.stat().st_size
    if size >= ASSET_LIMIT:
        fail(f'{out} is {size / 1024 ** 3:.2f} GiB, over GitHub\'s 2 GiB asset limit.')
    print(f'Packed {len(dirs)} packs, {files} files, into {out} ({size / 1024 ** 2:.0f} MiB).')
    return out


def upload(zip_path):
    gh = shutil.which('gh')
    if gh is None:
        fail(f'gh (GitHub CLI) is not installed. Install it with "winget install GitHub.cli", run "gh auth login", '
             f'and rerun this; or upload {zip_path} by hand as {SETUP_DOC} describes.')
    run = lambda *args, **kw: subprocess.run([gh, *args], cwd=REPO, **kw)  # noqa: E731
    if run('release', 'view', TAG, capture_output=True).returncode != 0:
        # A prerelease as well as a draft: even if someone publishes it by
        # mistake, the game's update check (releases/latest) never sees it.
        made = run('release', 'create', TAG, '--draft', '--prerelease', '--title', 'CI content (private, never publish)',
                   '--notes', 'Generated v20 content for the release workflow. Keep this a draft. '
                              f'Refresh it with python tools/ci_content.py upload ({SETUP_DOC}).')
        if made.returncode != 0:
            fail('Could not create the ci-content draft release (is "gh auth login" done?).')
    if run('release', 'upload', TAG, str(zip_path), '--clobber').returncode != 0:
        fail('Uploading the content zip failed.')
    print(f'Uploaded {zip_path.name} to the {TAG} draft release. Release builds will use it.')


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def _api(url, token, accept='application/vnd.github+json'):
    request = urllib.request.Request(url, headers={
        'Authorization': f'Bearer {token}', 'Accept': accept,
        'X-GitHub-Api-Version': '2022-11-28', 'User-Agent': 'bri-release-workflow'})
    return urllib.request.build_opener(_NoRedirect).open(request, timeout=60)


def fetch(content, repo, token):
    missing_help = (f'No private content for release builds: this repository needs a draft release named "{TAG}" '
                    f'holding {ASSET}. Max sets it up once from his PC with python tools/ci_content.py upload '
                    f'(see {SETUP_DOC}).')
    asset = None
    page = 1
    while asset is None:
        with _api(f'https://api.github.com/repos/{repo}/releases?per_page=100&page={page}', token) as response:
            releases = json.load(response)
        if not releases:
            break
        for release in releases:
            if release.get('draft') and release.get('tag_name') == TAG:
                asset = next((a for a in release.get('assets', []) if a['name'] == ASSET), None)
                if asset is None:
                    fail(missing_help)
                break
        page += 1
    if asset is None:
        fail(missing_help)
    zip_path = content.parent / ASSET
    print(f'Downloading {ASSET} ({asset["size"] / 1024 ** 2:.0f} MiB, updated {asset["updated_at"]}).')
    # The asset URL redirects to storage that must not see the token.
    try:
        response = _api(asset['url'], token, accept='application/octet-stream')
    except urllib.error.HTTPError as redirect:
        if redirect.code not in (301, 302, 307, 308):
            raise
        response = urllib.request.urlopen(redirect.headers['Location'], timeout=600)
    with response, open(zip_path, 'wb') as out:
        shutil.copyfileobj(response, out, 1024 * 1024)
    content.mkdir(parents=True, exist_ok=True)
    root = content.resolve()
    with zipfile.ZipFile(zip_path) as archive:
        for name in archive.namelist():
            target = (root / name).resolve()
            if target != root and root not in target.parents:
                fail(f'{ASSET} holds an unsafe path: {name}')
        archive.extractall(root)
        info = json.loads(archive.read('ci-content.json'))
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
        token = os.environ.get('GITHUB_TOKEN')
        repo = os.environ.get('GITHUB_REPOSITORY')
        if not token or not repo:
            fail('fetch runs in GitHub Actions: it needs GITHUB_TOKEN and GITHUB_REPOSITORY.')
        fetch(args.content, repo, token)
        return
    zip_path = pack(args.content, args.out)
    if args.command == 'upload':
        upload(zip_path)


if __name__ == '__main__':
    main()
