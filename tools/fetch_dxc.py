#!/usr/bin/env python3
"""Put Microsoft's DirectX Shader Compiler beside a built game, from the release pinned in tools/shader-compiler.json.

    python tools/fetch_dxc.py [--into target/release] [--archive dxc.zip]

On Windows wgpu compiles the world's shaders with the system's FXC unless dxcompiler.dll sits beside bri-client.exe; DXC
takes seconds where FXC takes minutes on a busy CPU (docs/release-builds.md). The release packager does this for a release;
this does it for a loose build (`target/release`). The archive is downloaded once into dist/shader-compiler/, refused unless its
sha256 is the pinned one, and only the pinned files (each checked against its own sha256) are copied: nothing else in the archive
is read. Nothing it fetches is committed. Pass --archive for a copy fetched some other way.
"""
import argparse
import hashlib
import json
import pathlib
import shutil
import sys
import urllib.request
import zipfile

REPO = pathlib.Path(__file__).resolve().parent.parent
PIN = REPO / 'tools' / 'shader-compiler.json'


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def load_pin():
    pin = json.loads(PIN.read_text(encoding='utf-8'))
    assert pin['schema_version'] == 1 and len(pin['sha256']) == 64 and pin['ships'], 'invalid shader compiler pin'
    for f in pin['ships']:
        to = f['to']
        assert '\\' not in to and ':' not in to and not any(p in ('', '.', '..') for p in to.split('/')), f'invalid file in the pin: {to}'
    return pin


def archive_for(pin, given):
    if given:
        path = pathlib.Path(given)
    else:
        path = REPO / 'dist' / 'shader-compiler' / pathlib.PurePosixPath(pin['url']).name
        if not path.is_file():
            path.parent.mkdir(parents=True, exist_ok=True)
            print(f"Downloading {pin['name']} {pin['version']} from {pin['url']}")
            partial = path.with_suffix(path.suffix + '.part')
            with urllib.request.urlopen(pin['url']) as response, open(partial, 'wb') as out:
                shutil.copyfileobj(response, out)
            partial.replace(path)
    got = sha256(path.read_bytes())
    if got != pin['sha256']:
        sys.exit(f"{path} has sha256 {got}, not the pinned {pin['sha256']}; refusing it.")
    return path


def install(into, archive_path=None):
    pin = load_pin()
    archive = archive_for(pin, archive_path)
    into = pathlib.Path(into)
    with zipfile.ZipFile(archive) as z:
        for f in pin['ships']:
            data = z.read(f['from'])
            got = sha256(data)
            if got != f['sha256']:
                sys.exit(f"{f['from']} in the archive has sha256 {got}, not the pinned {f['sha256']}; refusing it.")
            target = into / f['to']
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
    print(f"{pin['name']} {pin['version']} (sha256 {pin['sha256']}) is beside the game in {into}")


def main():
    p = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    p.add_argument('--into', default=str(REPO / 'target' / 'release'))
    p.add_argument('--archive')
    a = p.parse_args()
    install(a.into, a.archive)


if __name__ == '__main__':
    main()
