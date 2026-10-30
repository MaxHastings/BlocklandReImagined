"""Authored playback selection over original conversion, with a separate receipt.

Original WAVs, profiles, scene/geometry and conversion receipts are untouched.
Fresh bundle generation applies this policy before its honest inventory receipt.
"""
import hashlib
import json
from pathlib import Path

HERE=Path(__file__).resolve().parent


def sha(raw):return hashlib.sha256(raw).hexdigest()


def chosen(repo, directory):
    directory=Path(directory)
    if not (directory/'map-audio-provenance.json').is_file():return None
    rows=json.loads((directory/'map-audio-provenance.json').read_text())['sources']
    if not any(r['profile'].startswith(('map_creek:','map_forest:')) for r in rows):return None
    manifest=json.loads((directory/'package.json').read_text())
    if manifest['id'] not in {'map_creek','map_forest'}:return None
    config=(Path(repo)/'tools/map_ambience/original-emitter-policy.json').read_bytes()
    allowed=json.loads(config)['enabled_profiles']
    selected=[r for r in rows if r['profile'] in allowed]
    rules=(directory.parent/(manifest['id']+'_ambience_rules')).resolve()
    if not rules.is_relative_to(directory.parent.resolve()):raise ValueError('Original policy escapes native package root')
    return config,rows,selected,rules/'map-audio.rhai'


def script(rows):
    maps={}
    for r in rows:
        expected='map_audio/'+sha(r['profile'].encode())[:20]
        if r.get('ambient_key')!=expected:raise ValueError('Refresh original emitter conversion before selecting playback')
        maps.setdefault(r['map'],[]).append(r)
    text='// Authored active-emitter selection; original conversion/media retained.\nfn map_beds() {\n'
    for name,entries in sorted(maps.items()):
        beds=','.join('#{key:'+json.dumps(r['ambient_key'])+',track:'+json.dumps(r['profile'])+',at:'+
                      (json.dumps(r['position']) if r.get('spatial') else '()')+'}' for r in entries)
        text+='if map_id() == '+json.dumps(name)+' { return ['+beds+']; }\n'
    return (text+'[]\n}\n'+(HERE/'original-overlay.rhai').read_text()).encode()


def apply(repo,directory):
    selection=chosen(repo,directory)
    if selection is None:return None
    config,rows,selected,path=selection
    receipt_path=Path(directory)/'map-audio-policy-override.json'
    original=path.read_bytes()
    if receipt_path.exists():
        previous=json.loads(receipt_path.read_text())
        if sha(original)!=previous['applied_rule_sha256']:
            raise ValueError('Original map policy changed outside its override; preserve edits and regenerate fresh')
        original=previous['original_rule_source'].encode()
    output=script(selected)
    receipt={'schema_version':1,'policy_sha256':sha(config),'original_rule_sha256':sha(original),
             'original_rule_source':original.decode(),'applied_rule_sha256':sha(output),
             'enabled_profiles':[r['profile'] for r in selected],
             'inactive_profiles':[r['profile'] for r in rows if r not in selected],
             'note':'Authored playback override only; original recordings/profiles/scene/conversion identity retained'}
    path.write_bytes(output)
    receipt_path.write_text(json.dumps(receipt,indent=2)+'\n')
    return receipt


def problems(repo,directory):
    try:
        selection=chosen(repo,directory)
        if selection is None:return []
        config,rows,selected,path=selection
        receipt=json.loads((Path(directory)/'map-audio-policy-override.json').read_text())
        expected=script(selected)
        if (receipt.get('schema_version')!=1 or receipt['policy_sha256']!=sha(config)
                or receipt['applied_rule_sha256']!=sha(expected) or path.read_bytes()!=expected
                or receipt['enabled_profiles']!=[r['profile'] for r in selected]
                or receipt['inactive_profiles']!=[r['profile'] for r in rows if r not in selected]
                or sha(receipt['original_rule_source'].encode())!=receipt['original_rule_sha256']):
            return ['Original map playback differs from the explicit waterfall/shore-only policy']
        return []
    except (OSError,ValueError,KeyError,TypeError) as error:
        return [f'Original map playback selection needs a fresh authored override: {error}']
