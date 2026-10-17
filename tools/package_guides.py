#!/usr/bin/env python3
"""Ship creator guides, working links and vendored notices on every platform."""
from pathlib import Path
import posixpath
import re
import sys
from urllib.parse import urlsplit


GUIDES = {
    'PLAYTEST.md': 'RULE-WORKSHOP-PLAYTEST.md',
    'DESIGN.md': 'RULE-WORKSHOP-DESIGN.md',
    'CREATOR-TEST-CARD.md': 'RULE-WORKSHOP-CREATOR-TEST-CARD.md',
    'V0.2.2-PLAYTEST.md': 'V0.2.2-PLAYTEST.md',
    'V0.2.3-PLAYTEST.md': 'V0.2.3-PLAYTEST.md',
    'V0.2.3-RELEASE-NOTES.md': 'V0.2.3-RELEASE-NOTES.md',
}

PLAYER_GUIDES = (
    'PLAYTEST.md', 'KNOWN-ISSUES.md', 'TESTER-GUIDE.md', 'FEATURES.md',
    'PLAYTEST-MAC.md', 'DEDICATED-SERVER.md',
)
ONLINE_DOCS = 'https://github.com/MaxHastings/BlocklandReImagined/blob/main/docs/'


def release_links(text: str, source: str, available: set[str]) -> str:
    """Resolve guide links into the flat ZIP; absent platform pages go online."""
    names = {f'rule-workshop/{name}': target for name, target in GUIDES.items()}
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


def copy_guides(repo: Path, destination: Path) -> None:
    available = {p.name for p in destination.glob('*.md')} | set(GUIDES.values())
    # macOS has its own packager; all three invoke this shared document step.
    for source, filename in [('LICENSE', 'WINIT-LICENSE.txt'), ('BRI-PATCH.md', 'WINIT-PATCH.md')]:
        (destination / filename).write_bytes((repo / 'vendor' / 'winit' / source).read_bytes())
    for source, filename in GUIDES.items():
        text = (repo / 'docs' / 'rule-workshop' / source).read_text(encoding='utf-8')
        text = release_links(text, f'rule-workshop/{source}', available)
        (destination / filename).write_text(text, encoding='utf-8')
    # The platform packagers have already copied these player-facing pages.
    # Their repository links also need to point at the flat release layout.
    for filename in PLAYER_GUIDES:
        path = destination / filename
        if not path.is_file():
            continue
        text = path.read_text(encoding='utf-8-sig')
        text = release_links(text, filename, available)
        path.write_text(text, encoding='utf-8')


if __name__ == '__main__':
    copy_guides(Path(__file__).resolve().parent.parent, Path(sys.argv[1]))
