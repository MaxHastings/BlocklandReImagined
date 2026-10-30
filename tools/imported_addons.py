"""Generation identity for imported release Add-Ons and their host rules."""
import functools
import hashlib
import json
from pathlib import Path

import regenerate_content

REPO = Path(__file__).resolve().parents[1]
RECEIPT = 'import-generation.json'


@functools.cache
def importer_hash():
    # Reuse the base-content dependency walk: normal/build path dependencies,
    # importer sources and embedded ports, excluding tests and build output.
    sources = regenerate_content.Sources()
    return regenerate_content.sha256(
        sources.cargo('bri-addon-import'),
        (REPO / 'Cargo.toml').read_bytes().replace(b'\r\n', b'\n'),
        (REPO / 'Cargo.lock').read_bytes().replace(b'\r\n', b'\n'),
        (REPO / 'tools/addon_bundle.py').read_bytes().replace(b'\r\n', b'\n'),
        (REPO / 'tools/map_ambience/original_policy.py').read_bytes().replace(b'\r\n', b'\n'),
        (REPO / 'tools/map_ambience/original-emitter-policy.json').read_bytes().replace(b'\r\n', b'\n'),
        (REPO / 'tools/map_ambience/original-overlay.rhai').read_bytes().replace(b'\r\n', b'\n'),
        Path(__file__).read_bytes().replace(b'\r\n', b'\n'))


def recipe_hash(repo):
    # --repo can supply a selected port registry (including integration fixtures).
    return regenerate_content.sha256(importer_hash(), regenerate_content.tree_digest(
        Path(repo) / 'crates/addon-import/ports'))


def inventory(directory):
    directory = Path(directory)
    manifest = json.loads((directory / 'package.json').read_text(encoding='utf-8'))
    folders = [directory]
    for name in manifest.get('companions', []):
        if not name or any(c not in 'abcdefghijklmnopqrstuvwxyz0123456789_-' for c in name):
            raise ValueError('Invalid imported companion identity')
        folders.append(directory.parent / name)
    files = {}
    for folder in folders:
        if not (folder / 'package.json').is_file():
            raise ValueError(f'Missing companion {folder.name}')
        for path in sorted(folder.rglob('*')):
            if path.is_symlink() or not path.resolve().is_relative_to(folder.resolve()):
                raise ValueError(f'Imported content contains a link: {path.name}')
            if path.is_file() and path != directory / RECEIPT:
                with path.open('rb') as stream:
                    files[path.relative_to(directory.parent).as_posix()] = hashlib.file_digest(stream, 'sha256').hexdigest()
    return files


def write_receipt(directory, recipe):
    directory = Path(directory)
    data = {'schema_version': 1, 'recipe_sha256': recipe, 'files': inventory(directory)}
    (directory / RECEIPT).write_text(json.dumps(data, indent=2, sort_keys=True) + '\n', encoding='utf-8')


def problems(directory, recipe):
    try:
        receipt = json.loads((Path(directory) / RECEIPT).read_text(encoding='utf-8'))
        if receipt.get('schema_version') != 1 or receipt.get('recipe_sha256') != recipe:
            return ['Imported Add-On predates the current importer/ports; rebuild and upload the bundle']
        actual = inventory(directory)
        expected = receipt['files']
        if not isinstance(expected, dict):
            raise ValueError('Invalid generated inventory')
        if actual != expected:
            changed = sorted(name for name in actual.keys() | expected.keys() if actual.get(name) != expected.get(name))
            return ['Imported Add-On or host rules changed since generation: ' + ', '.join(changed[:5])]
        return []
    except (OSError, ValueError, KeyError, TypeError) as error:
        return [f'Imported Add-On lacks a valid generation receipt; rebuild the bundle: {error}']
