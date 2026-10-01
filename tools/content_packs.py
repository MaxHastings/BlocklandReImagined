"""Which packs a content root loads: its own packages.json when it has one,
otherwise the base game's list (crates/package/base-packages.json), as the
game reads them (bri_package::packages::PackageSet::load_root). Every tool
that reads a content root's packs uses this one reading.
"""
import json
import pathlib

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
