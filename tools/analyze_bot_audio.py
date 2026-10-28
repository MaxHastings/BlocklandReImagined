#!/usr/bin/env python3
"""Validate actual reaction presentation and summarize a native match report."""
import argparse
from collections import Counter
import json
import math
from pathlib import Path


def analyze(report, provenance):
    clips = {f'npc_popculture:sound/{r["id"]}': r for r in provenance['clips']}
    cues = report.get('reaction_cues', [])
    assert cues, 'no reaction presentation recorded'
    active = []
    last = {}
    profiles = Counter()
    violations = []
    previous = -100000
    max_active = 0
    for cue in cues:
        shape = cue['kind']['SoundShaped']
        clip = clips[shape['profile']]
        tick = cue['tick']
        pos = cue['position']
        if tick - previous < 360:
            violations.append(f'global start gap below 3 seconds at {tick}')
        previous = tick
        active = [a for a in active if a[0] > tick]
        if len(active) >= 2:
            violations.append(f'more than two active reactions at {tick}')
        if any((pos[0]-p[0])**2+(pos[2]-p[2])**2 < 576 for _, p in active):
            violations.append(f'nearby reaction overlap at {tick}')
        key = clip['id']
        interval = 14400 if clip['iconic'] else 5400
        if key in last and tick-last[key] < interval:
            violations.append(f'{key} global repeat interval too short at {tick}')
        last[key] = tick
        profiles[key] += 1
        assert shape['radius'] == 38.0 and 0.8 <= shape['pitch'] <= 1.4
        assert all(math.isfinite(p) for p in pos)
        active.append((tick + math.ceil(clip['seconds']/shape['pitch']*120) + 18, pos))
        max_active = max(max_active, len(active))
    policy = report['rules_state']['namespaces']['npc_popculture_rules']['global']
    assert policy['played'] == len(cues), 'policy plays did not reach presentation'
    return {'seconds': report['seconds'], 'played': len(cues), 'per_minute': len(cues)*60/report['seconds'],
            'unique_recordings': len(profiles), 'max_active': max_active,
            'by_event': policy['by_event'], 'by_tier': policy['by_tier'],
            'eligible': policy['eligible'], 'violations': violations,
            'most_used': profiles.most_common(10)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    parser.add_argument('provenance', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    result = analyze(json.loads(args.report.read_text(encoding='utf-8')),
                     json.loads(args.provenance.read_text(encoding='utf-8')))
    rendered = json.dumps(result, indent=2)
    print(rendered)
    if args.output:
        args.output.write_text(rendered+'\n', encoding='utf-8')
    if result['violations']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
