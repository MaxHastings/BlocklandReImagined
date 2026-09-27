"""Independently compare effect-bound worlds to the previously verified imports."""
import copy
import hashlib
import json
import sys
from pathlib import Path


def verify(before_dir, after_dir, effect_dir):
    library = json.loads((effect_dir / 'effects.json').read_text())
    names = {}
    for prefix, entries in [('light', library['lights']), ('emitter', library['emitters'])]:
        for entry in entries:
            names[(prefix + '_datablock', entry['id'].rsplit('/', 1)[1])] = entry['id']
            if entry['name'].strip():
                key = (prefix + '_ui', entry['name'].strip().lower())
                assert key not in names, key
                names[key] = entry['id']
    report = json.loads((after_dir / 'report.json').read_text())
    resolved = 0

    def resolve(value):
        nonlocal resolved
        if isinstance(value, dict):
            if value.get('kind') == 'unresolved' and isinstance(value.get('value'), dict):
                ref = value['value']
                key = (ref.get('namespace'), ref.get('name', '').strip().lower())
                if key in names:
                    resolved += 1
                    return {'kind': 'resolved', 'value': names[key]}
            return {k: resolve(v) for k, v in value.items()}
        if isinstance(value, list):
            return [resolve(v) for v in value]
        return value

    total = 0
    for entry in report['saves']:
        before = json.loads((before_dir / entry['file']).read_text())
        after = json.loads((after_dir / entry['file']).read_text())
        expected = resolve(copy.deepcopy(before))
        assert after == expected, entry['source']
        assert (before_dir / 'provenance' / (entry['sha256'] + '.source.bls')).read_bytes() == (
            after_dir / 'provenance' / (entry['sha256'] + '.source.bls')).read_bytes()
        total += len(after['bricks'])
    textures = json.loads((effect_dir / 'report.json').read_text())['texture_bindings']
    for binding in textures:
        file = effect_dir / library['textures'][binding['id']]
        assert hashlib.sha256(file.read_bytes()).hexdigest() == binding['sha256']
    result = dict(status='passed', saves=len(report['saves']), bricks=total,
                  resolved_effect_references=resolved, textures=len(textures),
                  scope='Only known light/emitter references changed; every other native field and original BLS byte remains equal to previously independently verified imports. Packaged texture hashes match the conversion manifest. No rendered behavior claim.')
    (after_dir / 'effect-binding-verification.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    verify(*(Path(p) for p in sys.argv[1:]))
