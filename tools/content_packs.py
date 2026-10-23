"""Which packs a content root loads: its own packages.json when it has one,
otherwise the base game's list (crates/package/base-packages.json), as the
game reads them (bri_package::packages::PackageSet::load_root). Every tool
that reads a content root's packs uses this one reading.
"""
import json
import pathlib
import hashlib

REPO = pathlib.Path(__file__).resolve().parent.parent
BASE_LIST = REPO / 'crates' / 'package' / 'base-packages.json'
CONTENT_LIST = 'packages.json'


def package_list(content, repo=REPO):
    """The package list content loads: (its path, the parsed list, whether it
    is the content root's own). repo is the checkout whose base list applies."""
    own = pathlib.Path(content) / CONTENT_LIST
    path = own if own.is_file() else pathlib.Path(repo) / BASE_LIST.relative_to(REPO)
    return path, json.loads(path.read_text(encoding='utf-8-sig')), own.is_file()


def missing_base_packs(content, repo=REPO):
    """The base game's pack folders (the entries with a role) content lacks:
    empty when it is generated game content."""
    content = pathlib.Path(content)
    _, listing, _ = package_list(content, repo)
    return [p['dir'] for p in listing['packages'] if p.get('role') and not (content / p['dir']).is_dir()]


def verify_menu_images(content, repo=REPO):
    """Verify curated menu artwork in the selected UI pack, not just its folder."""
    content, repo = pathlib.Path(content), pathlib.Path(repo)
    _, listing, _ = package_list(content, repo)
    packs = [p for p in listing['packages'] if p.get('role') == 'ui_pack']
    if not packs:
        return 0
    expected = [r for r in json.loads((repo / 'docs/vanilla-reference-files.json').read_text())['files']
                if r['path'].lower().startswith('screenshots/')]
    if not expected:
        raise ValueError('The reference lists no menu images.')
    for pack in packs:
        root = (content / pack['dir']).resolve()
        if not root.is_relative_to(content.resolve()):
            raise ValueError('UI pack escapes content root.')
        images = json.loads((root / 'ui-pack.json').read_text(encoding='utf-8-sig'))['images']
        for source in expected:
            key = str(pathlib.PurePosixPath(source['path']).with_suffix('')).lower()
            entry = images.get(key)
            if entry is None:
                raise ValueError(f'UI pack is missing menu image {key}; regenerate ui_pack.')
            file = (root / entry['file']).resolve()
            if not file.is_relative_to(root):
                raise ValueError(f'Menu image escapes UI pack: {key}')
            data = file.read_bytes()
            digest = hashlib.sha256(data).hexdigest()
            if len(data) != source['bytes'] or digest != source['sha256'] or digest != entry['sha256']:
                raise ValueError(f'Menu image differs from the curated reference: {key}')
    return len(expected)


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description='Verify packaged reference menu artwork.')
    parser.add_argument('--verify-ui', type=pathlib.Path, required=True)
    parser.add_argument('--repo', type=pathlib.Path, default=REPO)
    args = parser.parse_args()
    print(f'Verified {verify_menu_images(args.verify_ui, args.repo)} reference menu images.')
