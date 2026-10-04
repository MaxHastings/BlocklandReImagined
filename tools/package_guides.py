#!/usr/bin/env python3
"""Ship the creator guides with working links in every platform package."""
from pathlib import Path
import sys


GUIDES = {
    'PLAYTEST.md': 'RULE-WORKSHOP-PLAYTEST.md',
    'DESIGN.md': 'RULE-WORKSHOP-DESIGN.md',
    'CREATOR-TEST-CARD.md': 'RULE-WORKSHOP-CREATOR-TEST-CARD.md',
    'V0.2.2-PLAYTEST.md': 'V0.2.2-PLAYTEST.md',
    'V0.2.3-PLAYTEST.md': 'V0.2.3-PLAYTEST.md',
    'V0.2.3-RELEASE-NOTES.md': 'V0.2.3-RELEASE-NOTES.md',
}


def copy_guides(repo: Path, destination: Path) -> None:
    for source, filename in GUIDES.items():
        text = (repo / 'docs' / 'rule-workshop' / source).read_text(encoding='utf-8')
        for target, packaged in GUIDES.items():
            text = text.replace(f'({target})', f'({packaged})')
        text = text.replace('(../KNOWN-ISSUES.md)', '(KNOWN-ISSUES.md)')
        (destination / filename).write_text(text, encoding='utf-8')
    # The platform packagers have already copied these player-facing pages.
    # Their repository links also need to point at the flat release layout.
    for filename in ('KNOWN-ISSUES.md', 'FEATURES.md'):
        path = destination / filename
        if not path.is_file():
            continue
        text = path.read_text(encoding='utf-8-sig')
        for source, packaged in GUIDES.items():
            text = text.replace(f'(rule-workshop/{source})', f'({packaged})')
        path.write_text(text, encoding='utf-8')


if __name__ == '__main__':
    copy_guides(Path(__file__).resolve().parent.parent, Path(sys.argv[1]))
