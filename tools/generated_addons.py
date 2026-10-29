"""Lightweight identity and verification for generated native release Add-Ons.

Media stays outside Git. Release fetch/packaging verifies the pinned recipe,
every recording and the companion policy without importing audio libraries.
"""
import hashlib
import json
from pathlib import Path

RECIPES = {
    'map-ambience': {
        'id': 'map_ambience',
        'builder': 'tools/build_map_ambience.py',
        'receipt': 'map-ambience-generated.json',
        'needs_content': True,
        'inputs': ['tools/build_map_ambience.py', 'tools/generated_addons.py',
                   'tools/prepare_map_ambience_audition.py', 'tools/prepare_map_ambience_details.py',
                   'tools/map_ambience/approvals.py', 'tools/map_ambience/source-approvals.json',
                   'tools/map_ambience/candidates.json', 'tools/map_ambience/masters.json',
                   'tools/map_ambience/maps.json', 'tools/map_ambience/palette.json',
                   'tools/map_ambience/package.json', 'tools/map_ambience/rules-package.json', 'tools/map_ambience/behaviour.json',
                   'tools/map_ambience/ambience.rhai'],
    },
    'npc-reactions': {
        'id': 'npc_popculture',
        'builder': 'tools/build_npc_reactions.py',
        'inputs': ['tools/build_npc_reactions.py', 'tools/generated_addons.py',
                   'tools/npc_reactions/catalog.json', 'tools/npc_reactions/package.json',
                   'tools/npc_reactions/rules-package.json', 'tools/npc_reactions/behaviour.json',
                   'tools/npc_reactions/reactions.rhai'],
    },
}


def recipe_hash(repo, recipe):
    digest = hashlib.sha256()
    for name in RECIPES[recipe]['inputs']:
        digest.update(name.encode() + b'\0')
        digest.update((Path(repo) / name).read_bytes().replace(b'\r\n', b'\n'))
    return digest.hexdigest()


def problems(repo, directory, recipe):
    if recipe == 'map-ambience':
        from build_map_ambience import problems as ambience_problems
        return ambience_problems(repo, directory)
    if recipe not in RECIPES:
        return [f'Unknown generated Add-On recipe {recipe!r}']
    directory = Path(directory)
    try:
        receipt = json.loads((directory / 'npc-reactions-generated.json').read_text(encoding='utf-8'))
        if receipt.get('recipe_sha256') != recipe_hash(repo, recipe):
            return ['Generated Add-On predates its current recipe; rebuild and upload the bundle']
        catalog = json.loads((Path(repo) / 'tools/npc_reactions/catalog.json').read_text(encoding='utf-8'))
        expected = {clip['id']: clip for clip in catalog['clips']}
        clips = receipt['clips']
        if len(clips) != len(expected) or {clip['id'] for clip in clips} != set(expected):
            return ['Generated recording inventory differs from the pinned catalog']
        out = []
        for clip in clips:
            slug = clip['id']
            # Only catalog-owned names may select a file; never trust receipt paths.
            if not slug or any(c not in 'abcdefghijklmnopqrstuvwxyz0123456789_-' for c in slug):
                return ['Invalid recording identity']
            file = directory / 'assets/clips' / (slug + '.wav')
            if clip['sha256'] != expected[slug]['sha256']:
                out.append(f'{slug}: source hash differs from pinned recording')
            if hashlib.sha256(file.read_bytes()).hexdigest() != clip['processed_sha256']:
                out.append(f'{slug}: processed recording missing or changed')
        companion = directory.parent / 'npc_popculture_rules'
        for source, target in [('package.json', directory / 'package.json'),
                               ('rules-package.json', companion / 'package.json'),
                               ('behaviour.json', companion / 'behaviour.json'),
                               ('reactions.rhai', companion / 'reactions.rhai')]:
            wanted = (Path(repo) / 'tools/npc_reactions' / source).read_bytes().replace(b'\r\n', b'\n')
            if target.read_bytes().replace(b'\r\n', b'\n') != wanted:
                out.append(f'{target.name}: generated policy differs from current recipe')
        return out
    except (OSError, ValueError, KeyError, TypeError) as error:
        return [f'Incomplete generated Add-On: {error}']
