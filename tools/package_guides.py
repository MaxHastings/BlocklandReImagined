#!/usr/bin/env python3
"""Ship the creator guides with working links in every platform package."""
from pathlib import Path
import sys


GUIDES = {
    'PLAYTEST.md': 'RULE-WORKSHOP-PLAYTEST.md',
    'DESIGN.md': 'RULE-WORKSHOP-DESIGN.md',
    'CREATOR-TEST-CARD.md': 'RULE-WORKSHOP-CREATOR-TEST-CARD.md',
}


def copy_guides(repo: Path, destination: Path) -> None:
    for source, filename in GUIDES.items():
        text = (repo / 'docs' / 'rule-workshop' / source).read_text(encoding='utf-8')
        for target, packaged in GUIDES.items():
            text = text.replace(f'({target})', f'({packaged})')
        (destination / filename).write_text(text, encoding='utf-8')


if __name__ == '__main__':
    copy_guides(Path(__file__).resolve().parent.parent, Path(sys.argv[1]))
