"""Release-side hear-first guard; auditions and offline conversion stay usable.

Only a human decision recorded with its exact input hash can approve a source.
This module never changes approval state or treats a general mix comment as one.
"""
import hashlib
import json
from pathlib import Path


def source_problems(registry, source_hash):
    rows = [row for row in registry['sources'] if row['input_sha256'] == source_hash]
    if len(rows) != 1:
        return [f'Ambience recording {source_hash}: missing or ambiguous source approval identity']
    row = rows[0]
    if row.get('state') != 'approved' or not isinstance(row.get('approval_evidence'), str) or not row['approval_evidence'].strip():
        return [f"{row['id']}: hear-first source approval pending; do not ship this recording"]
    return []


def original_map_problems(repo, directory):
    """Check actual restored map clips, independently of importer freshness.

    Existing packages without converted map audio need no new approval. A
    generated consumer or clip prefix requires its receipt, even if deleted.
    """
    directory = Path(directory)
    receipt_path = directory / 'map-audio-provenance.json'
    pack_path = directory / 'assets/weapons.json'
    try:
        sounds = json.loads(pack_path.read_text(encoding='utf-8')).get('sounds', {}) if pack_path.is_file() else {}
        restored = {key: row for key, row in sounds.items() if row.get('file', '').startswith('map-audio/')}
        if not receipt_path.exists() and not restored and not (directory / 'map-audio.rhai').exists():
            return []
        receipt = json.loads(receipt_path.read_text(encoding='utf-8'))
        if receipt.get('schema_version') != 1:
            return ['Unsupported original-map ambience receipt schema']
        rows = receipt['sources']
        if not rows and not restored and not (directory / 'map-audio.rhai').exists():
            return []  # Unsupported source nodes only: no media adopted.
        expected = {row['profile']: row for row in rows}
        if len(expected) != len(rows) or set(expected) != set(restored):
            return ['Original-map ambience receipt and actual sound inventory differ']
        registry = json.loads((Path(repo) / 'tools/map_ambience/source-approvals.json').read_text(encoding='utf-8'))
        if registry.get('schema_version') != 1:
            return ['Unsupported ambience approval registry schema']
        out = []
        for key, row in expected.items():
            wanted_key='map_audio/'+hashlib.sha256(key.encode()).hexdigest()[:20]
            if row.get('ambient_key')!=wanted_key:
                out.append(f'{key}: missing or stale bounded ambient key; refresh with the current map-audio converter')
            relative = Path(restored[key]['file'])
            assets = (directory / 'assets').resolve()
            clip = (assets / relative).resolve()
            if relative.is_absolute() or not clip.is_relative_to(assets):
                return ['Original-map ambience clip escapes assets']
            actual_hash = hashlib.sha256(clip.read_bytes()).hexdigest()
            if actual_hash != row['sha256']:
                out.append(f'{key}: original recording differs from its source receipt')
            out.extend(source_problems(registry, actual_hash))
        from map_ambience.original_policy import problems as policy_problems
        out.extend(policy_problems(repo,directory))
        return out
    except (OSError, ValueError, KeyError, TypeError) as error:
        return [f'Incomplete original-map ambience approval evidence: {error}']
