#!/usr/bin/env python3
"""Ship current reader guides and notices without historical release artifacts."""
from pathlib import Path
import posixpath
import re
import sys
from urllib.parse import urlsplit


GUIDES = {
    'rule-workshop/PLAYTEST.md': 'RULE-WORKSHOP-PLAYTEST.md',
    'modding/bot-spawners.md': 'BOT-SPAWNERS.md',
}

PLAYER_GUIDES = (
    'PLAYTEST.md', 'KNOWN-ISSUES.md', 'TESTER-GUIDE.md', 'FEATURES.md',
    'PLAYTEST-MAC.md', 'DEDICATED-SERVER.md',
)
ONLINE_DOCS = 'https://github.com/MaxHastings/BlocklandReImagined/blob/main/docs/'


def release_links(text: str, source: str, available: set[str]) -> str:
    """Resolve guide links into the flat ZIP; absent platform pages go online."""
    names = dict(GUIDES)
    names.update({name: name for name in PLAYER_GUIDES})

    def replace(match):
        url = urlsplit(match.group(1))
        if url.scheme or url.netloc or not url.path:
            return match.group(0)
        resolved = posixpath.normpath(posixpath.join(posixpath.dirname(source), url.path))
        filename = names.get(resolved)
        if filename is None:
            if url.path in available:
                return match.group(0)
            destination = ONLINE_DOCS + resolved
        else:
            destination = filename if filename in available else ONLINE_DOCS + resolved
        if url.query:
            destination += '?' + url.query
        if url.fragment:
            destination += '#' + url.fragment
        return '](' + destination + ')'

    return re.sub(r'\]\(([^\s)]+)\)', replace, text)


def copy_guides(repo: Path, destination: Path, version: str = 'development') -> None:
    docs = destination / 'docs'
    docs.mkdir(exist_ok=True)
    licenses = destination / 'licenses'
    licenses.mkdir(exist_ok=True)
    available = {p.name for p in destination.glob('*.md')} | set(GUIDES.values())
    # macOS has its own packager; all three invoke this shared document step.
    for source, filename in [('LICENSE', 'WINIT-LICENSE.txt'), ('BRI-PATCH.md', 'WINIT-PATCH.md')]:
        (licenses / filename).write_bytes((repo / 'vendor' / 'winit' / source).read_bytes())
        # Platform packagers may already have copied the notice at the root.
        (destination / filename).unlink(missing_ok=True)
    navigation_notices = licenses / 'recastnavigation-rs'
    navigation_notices.mkdir(exist_ok=True)
    for name in ('NOTICE.md', 'LICENSE.txt', 'RECAST-LICENSE.txt'):
        (navigation_notices / name).write_bytes((repo / 'third-party/recastnavigation-rs' / name).read_bytes())
    for source, filename in GUIDES.items():
        text = (repo / 'docs' / source).read_text(encoding='utf-8')
        text = release_links(text, source, available)
        (docs / filename).write_text(text, encoding='utf-8')
        (destination / filename).unlink(missing_ok=True)
    # The platform packagers have already copied these player-facing pages.
    # Their repository links also need to point at the flat release layout.
    for filename in (*PLAYER_GUIDES, 'PLAYTEST-STRESS-LAB.md'):
        path = destination / filename
        if not path.is_file():
            continue
        text = path.read_text(encoding='utf-8-sig')
        text = release_links(text, filename, available)
        (docs / filename).write_text(text, encoding='utf-8')
        path.unlink()
    (destination / 'README.txt').write_text(
        f'Blockland ReImagined {version}\n\n'
        'Windows: run Launch.cmd. Linux: run launch.sh. macOS: open BlocklandReImagined.app.\n'
        'Keep the content folder and bundled libraries beside the game.\n\n'
        'Instructions and controls: docs/TESTER-GUIDE.md\n'
        'Known limitations: docs/KNOWN-ISSUES.md\n'
        'Detailed playtest guide: docs/PLAYTEST.md\n'
        'Creator recipes: docs/RULE-WORKSHOP-PLAYTEST.md\n'
        'Bot encounters: docs/BOT-SPAWNERS.md\n'
        'Third-party credits: CREDITS.md and licenses/\n\n'
        'To report a problem, reproduce it, close the game and send the latest\n'
        'session log from logs/ with the version, map and a screenshot.\n'
        'MANIFEST.json records the packaged files for integrity verification.\n',
        encoding='utf-8')


if __name__ == '__main__':
    copy_guides(Path(__file__).resolve().parent.parent, Path(sys.argv[1]),
                sys.argv[2] if len(sys.argv) > 2 else 'development')
