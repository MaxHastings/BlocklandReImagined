#!/usr/bin/env python3
"""Use the verified v0.1.15 Windows content for this disposable playtest.

The private CI draft is older than main and the addon-bundle draft is absent.
Reuse the same credited originals that the published build already carries;
never publish or upload a refreshed draft as part of this experiment.
"""
import hashlib
import pathlib
import shutil
import subprocess
import sys

import ci_content

ROOT = pathlib.Path(__file__).resolve().parent.parent
TAG = 'v0.1.15-alpha'
SHA256 = 'fdccaf6af617e8ae48e860c3851310f9b5045234f1248fa4fdc43d9edf3b5dad'


def main():
    cache = ROOT / 'artifacts' / 'rule-workshop-upstream'
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / 'BlocklandReImagined-windows.zip'
    if not archive.is_file():
        subprocess.run(['gh', 'release', 'download', TAG, '--repo',
                        'MaxHastings/BlocklandReImagined', '--pattern', archive.name,
                        '--dir', str(cache)], check=True)
    with archive.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    if digest != SHA256:
        sys.exit('Pinned upstream content checksum differs. Refusing to package it.')
    unpacked = cache / 'unpacked'
    if not unpacked.exists():
        ci_content.extract(archive, unpacked)
    release = unpacked / 'BlocklandReImagined-v0.1.15-windows'
    content = ROOT / 'content'
    # Select only base packs. Packaging copies the branch's own default Add-Ons.
    dirs, _ = ci_content.package_dirs(release / 'content')
    for name in dirs:
        if name.startswith('addons/'):
            continue
        shutil.copytree(release / 'content' / name, content / name, dirs_exist_ok=True)
    subprocess.run([sys.executable, 'tools/addon_bundle.py', 'from-release',
                    str(release)], cwd=ROOT, check=True)
    subprocess.run([sys.executable, 'tools/addon_bundle.py', 'install',
                    str(content)], cwd=ROOT, check=True)
    print('Workshop content recovered from the checksum-pinned Windows release.')


if __name__ == '__main__':
    main()
