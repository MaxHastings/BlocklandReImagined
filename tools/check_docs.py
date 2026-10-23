#!/usr/bin/env python3
"""Check maintained reader guides and generated release layouts offline.

Run: python tools/check_docs.py
Checks local Markdown links/heading anchors, not external URLs or factual claims.
No game content, compiler or third-party modules are needed. Dated progress,
research, audits and release plans remain history.
"""
import re
import shutil
import sys
import tempfile
import unicodedata
from pathlib import Path
from urllib.parse import unquote, urlsplit

import package_guides

ROOT = Path(__file__).resolve().parents[1]
HISTORY = {'progress.md', 'playtest-contract.md', 'v022-delivery-contract.md',
           'v023-work-plan.md'}
LINK = re.compile(r'\]\((<[^>\n]+>|[^\s)]+)(?:\s+"[^"\n]*")?\)')
REFERENCE = re.compile(r'^\s{0,3}\[[^\]]+\]:\s*(<[^>\n]+>|\S+)', re.M)
REPO_BLOB = 'https://github.com/MaxHastings/BlocklandReImagined/blob/main/'


def prose(text, keep_inline=False):
    """Leave line numbers intact while excluding fenced and inline code."""
    result = []
    fence = None
    for line in text.splitlines(keepends=True):
        marker = re.match(r'^\s{0,3}(`{3,}|~{3,})', line)
        if marker:
            token = marker.group(1)
            if fence is None:
                fence = token
            elif token[0] == fence[0] and len(token) >= len(fence):
                fence = None
            result.append('\n')
        elif fence:
            result.append('\n')
        else:
            result.append(line.replace('`', '') if keep_inline else
                          re.sub(r'(`+).*?\1', '', line))
    return ''.join(result)


def anchors(text):
    """GitHub-style heading slugs, including duplicate headings and Unicode."""
    found = set()
    # Code labels in headings contribute to their anchor, unlike code examples.
    for line in prose(text, keep_inline=True).splitlines():
        match = re.match(r'^\s{0,3}#{1,6}\s+(.+?)\s*#*\s*$', line)
        if not match:
            continue
        title = re.sub(r'\[([^\]]+)\]\([^)]*\)', r'\1', match.group(1))
        title = re.sub(r'<[^>]+>', '', title).lower()
        slug = ''.join(c for c in title if c in ' -_' or
                       unicodedata.category(c)[0] in 'LN').replace(' ', '-')
        unique = slug
        number = 0
        while unique in found:
            number += 1
            unique = f'{slug}-{number}'
        found.add(unique)
    return found


def check_links(path, root, anchor_cache=None, online_root=None):
    cache = anchor_cache if anchor_cache is not None else {}
    try:
        text = prose(path.read_text(encoding='utf-8-sig'))
    except UnicodeError as error:
        return [f'{path.relative_to(root)}: invalid UTF-8: {error}']
    errors = []
    for match in [*LINK.finditer(text), *REFERENCE.finditer(text)]:
        target = match.group(1).strip('<>')
        url = urlsplit(target)
        online_source = online_root is not None and target.startswith(REPO_BLOB)
        if (url.scheme or url.netloc) and not online_source:
            continue
        line = text[:match.start()].count('\n') + 1
        label = f'{path.relative_to(root)}:{line}: {target}'
        if online_source:
            relative = urlsplit(target[len(REPO_BLOB):]).path
            destination = (online_root / unquote(relative)).resolve()
            allowed_root = online_root.resolve()
        else:
            destination = (path.parent / unquote(url.path)).resolve() if url.path else path.resolve()
            allowed_root = root.resolve()
        if not destination.is_relative_to(allowed_root):
            errors.append(label + ' (escapes documentation root)')
        elif not destination.exists():
            errors.append(label + ' (missing path)')
        elif url.fragment and destination.suffix.lower() == '.md':
            if destination not in cache:
                cache[destination] = anchors(destination.read_text(encoding='utf-8-sig'))
            if unquote(url.fragment) not in cache[destination]:
                errors.append(label + ' (missing heading)')
    return errors


def maintained_docs(root):
    files = [root / 'README.md', root / 'CONTRIBUTING.md']
    files += [p for p in (root / 'docs').glob('*.md') if p.name not in HISTORY]
    files += list((root / 'docs/modding').glob('*.md'))
    files += [root / 'docs/rule-workshop' / name for name in package_guides.GUIDES]
    files += list((root / 'packages').glob('*/README.md'))
    files += list((root / 'crates').glob('*/README.md'))
    return sorted(set(files))


def check_release_guides(root, platform, stress_lab=False):
    """Use the actual copier on the docs each packager ships, without content."""
    with tempfile.TemporaryDirectory(prefix='bri-docs-') as temporary:
        destination = Path(temporary)
        players = set(package_guides.PLAYER_GUIDES)
        if platform == 'macos':
            players.remove('DEDICATED-SERVER.md')
        else:
            players.remove('PLAYTEST-MAC.md')
        for filename in players:
            shutil.copyfile(root / 'docs' / filename, destination / filename)
        if stress_lab:
            shutil.copyfile(root / 'docs/stress-lab/PLAYTEST-STRESS-LAB.md',
                            destination / 'PLAYTEST-STRESS-LAB.md')
        package_guides.copy_guides(root, destination)
        for source, filename in (('LICENSE', 'WINIT-LICENSE.txt'),
                                 ('BRI-PATCH.md', 'WINIT-PATCH.md')):
            if (destination / 'licenses' / filename).read_bytes() != (root / 'vendor/winit' / source).read_bytes():
                return [f'{platform}/{filename}: vendored notice changed']
        cache = {}
        return [f'{platform}{"+stress" if stress_lab else ""}/{error}'
                for path in sorted((destination / 'docs').glob('*.md'))
                for error in check_links(path, destination, cache, online_root=root)]


def main():
    files = maintained_docs(ROOT)
    cache = {}
    errors = [error for path in files
              for error in check_links(path, ROOT, cache, online_root=ROOT)]
    for platform in ('windows', 'linux', 'macos'):
        for stress_lab in (False, True):
            errors += check_release_guides(ROOT, platform, stress_lab)
    if errors:
        print('\n'.join(errors))
        print(f'{len(errors)} documentation link error(s).')
        return 1
    print(f'Checked {len(files)} maintained documents and six generated release-guide layouts.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
