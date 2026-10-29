#!/usr/bin/env python3
"""Build the fixed map palette. Media stays outside Git; no automatic approval.

--audition-only permits a private candidate that release verification rejects.
The shipping path requires explicit hear-first source decisions before writing.
"""
import argparse
import hashlib
import html
import json
from pathlib import Path
import shutil
import tempfile

import generated_addons
from map_ambience.approvals import source_problems
import prepare_map_ambience_audition as beds
import prepare_map_ambience_details as details

REPO = Path(__file__).resolve().parent.parent
HERE = REPO / 'tools/map_ambience'
DETAILS = {'bird-robin': 'robin', 'bird-blackbird': 'blackbird'}


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def approval_receipt(registry, source_hash):
    row = next((r for r in registry['sources'] if r['input_sha256'] == source_hash), None)
    return None if row is None else {k:row[k] for k in ['id','input_sha256','state','approval_evidence']}


def selection(repo=REPO):
    here = Path(repo) / 'tools/map_ambience'
    palette = json.loads((here / 'palette.json').read_text(encoding='utf-8'))
    inventory = json.loads((here / 'maps.json').read_text(encoding='utf-8'))['maps']
    if palette['schema_version'] != 1 or len(palette['maps']) != 21 or {m['map'] for m in palette['maps']} != {m['id'] for m in inventory}:
        raise ValueError('palette must cover exactly the fixed21-map inventory')
    chosen_beds = {c for m in palette['maps'] for c in m['beds']}
    chosen_details = {c for m in palette['maps'] for c in m['details']}
    if palette['detail_gap_seconds'] != [35,85]:
        raise ValueError('policy timing differs from the fixed35..85s gap')
    if not chosen_details <= DETAILS.keys():
        raise ValueError('detail has no pinned original recording')
    for m in palette['maps']:
        if len(m['beds']) > 1 or len(m['details']) > 2 or len(m['anchors']) > 32:
            raise ValueError('palette exceeds the chosen layer/anchor budget')
        if bool(m['details']) != bool(m['anchors']):
            raise ValueError('localized details need evidenced fixed anchors')
    catalog = {r['key']: r for r in json.loads((here / 'candidates.json').read_text())['clips']}
    masters = {r['key']: r for r in json.loads((here / 'masters.json').read_text())}
    selected = {k: dict(catalog[k], kind='bed', source_id='freesound-preview:'+k) for k in chosen_beds}
    selected.update({k: dict(masters[DETAILS[k]], key=k, kind='detail', source_id='original-wav:'+DETAILS[k]) for k in chosen_details})
    return palette, selected


def policy(palette):
    text = 'fn palette() {\n'
    for m in palette['maps']:
        text += ('if map_id() == '+json.dumps(m['map'])+' { return #{beds:'+
                 json.dumps(m['beds'])+',details:'+json.dumps(m['details'])+
                 ',anchors:'+json.dumps(m['anchors'])+'}; }\n')
    return text + '#{beds:[],details:[],anchors:[]}\n}\n'


def build(cache, output, content, download=False, audition_only=False):
    palette, selected = selection()
    registry = json.loads((HERE / 'source-approvals.json').read_text(encoding='utf-8'))
    pending = [p for r in selected.values() for p in source_problems(registry, r['input_sha256'])]
    if pending and not audition_only:
        raise ValueError('; '.join(pending))
    if output.exists() and any(output.iterdir()) and not (output / 'map-ambience-generated.json').is_file():
        raise ValueError('refusing to overwrite a folder not owned by this generator')
    with tempfile.TemporaryDirectory(prefix='bri-ambience-') as folder:
        staged = Path(folder)
        bed_rows = beds.build(cache, staged, content, download)
        detail_rows = details.build(cache, staged / 'details')
        bed_rows = {r['key']: r for r in bed_rows}
        detail_rows = {r['file'][:-4]: r for r in detail_rows}
        output.mkdir(parents=True, exist_ok=True)
        clips_dir = output / 'assets/clips'
        clips_dir.mkdir(parents=True, exist_ok=True)
        for old in clips_dir.glob('*.wav'):
            if old.stem not in selected:
                old.unlink()  # Only generator-owned WAV names within this clip folder.
        sounds, receipts = {}, []
        for key, row in sorted(selected.items()):
            is_bed = row['kind'] == 'bed'
            source = staged / ('loops' if is_bed else 'details') / (key+'.wav')
            raw = source.read_bytes()
            (clips_dir / (key+'.wav')).write_bytes(raw)
            sound = {'file':'clips/'+key+'.wav','volume':1.0,'looping':is_bed,'music':is_bed}
            if key != 'room':
                sound['environment'] = 'exterior'
            if not is_bed:
                sound['spatial'] = {'reference_distance':12.0,'max_distance':160.0}
            sounds['map_ambience:'+key] = sound
            receipts.append(dict(row, file='assets/clips/'+key+'.wav', processed_sha256=sha(raw),
                                 source_approval=approval_receipt(registry,row['input_sha256']),
                                 processing=bed_rows[key]['processing'] if is_bed else detail_rows[key]))
        (output / 'assets/weapons.json').write_text(json.dumps({'schema_version':5,'id':'map_ambience','sounds':sounds},indent=2)+'\n')
        shutil.copyfile(HERE / 'package.json', output / 'package.json')
        rules = output.parent / 'map_ambience_rules'
        if rules.exists() and json.loads((rules/'package.json').read_text())['id'] != 'map_ambience_rules':
            raise ValueError('refusing to overwrite unrelated rules package')
        rules.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(HERE/'rules-package.json',rules/'package.json')
        for name in ['behaviour.json','ambience.rhai']:
            shutil.copyfile(HERE/name,rules/name)
        (rules/'palette.rhai').write_text(policy(palette),encoding='utf-8')
        # Remove only the known files of the invalid combined prototype.
        for name in ['behaviour.json','ambience.rhai','palette.rhai']:
            (output/name).unlink(missing_ok=True)
        files = {p.relative_to(output).as_posix():sha(p.read_bytes()) for p in output.rglob('*') if p.is_file() and p.name != 'map-ambience-generated.json'}
        rules_files = {p.relative_to(rules).as_posix():sha(p.read_bytes()) for p in rules.rglob('*') if p.is_file()}
        receipt = {'schema_version':1,'recipe_sha256':generated_addons.recipe_hash(REPO,'map-ambience'),
                   'audition_only':audition_only,'clips':receipts,'maps':palette['maps'],'files':files,'rules_files':rules_files,
                   'mix_acceptance':'Pending ordinary-game listening with footsteps/tools/weapons/NPC/voice; solo source approval does not accept the mix.'}
        (output / 'map-ambience-generated.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
        if audition_only:
            rows = []
            for r in receipts:
                maps = ', '.join(m['name'] for m in palette['maps'] if r['key'] in m['beds']+m['details'])
                quality = 'WAV derived from public HQ MP3 preview' if r['kind']=='bed' else 'Derived directly from original PCM WAV'
                rows.append('<article><h2>'+html.escape(r['key'])+'</h2><p>'+html.escape(maps)+
                            '</p><audio controls preload="none" src="'+html.escape(output.name+'/'+r['file'])+'"></audio><p>'+
                            html.escape(quality)+' · '+html.escape(r['author'])+' · CC0 · <a href="'+
                            html.escape(r['source'])+'">recording source</a></p></article>')
            document = ('<!doctype html><meta charset="utf-8"><title>Chosen ambience palette</title>'
                        '<style>body{background:#171c22;color:#e8edf2;font:18px system-ui;max-width:900px;margin:40px auto;padding:20px}'
                        'a{color:#97d5ff}article{padding:16px;margin:16px 0;background:#242c35}audio{width:100%}</style>'
                        '<h1>Eight proposed ambience sources</h1><p>Fixed candidate palette for the21 maps. '
                        'Listen to approve the recordings; final integrated levels remain pending. '
                        'One subtle bed per map at most; Forest birds are sparse fixed-source calls35–85s apart in the candidate policy. '
                        'Creek and Forest Dark use their separately approved originals; Construct and Destruct retain deliberate quiet. '
                        'These controls play clips solo and do not demonstrate runtime spatial treatment. No autoplay.</p>'+''.join(rows))
            (output.parent/'audition.html').write_text(document,encoding='utf-8')
    return receipt


def problems(repo, directory):
    directory = Path(directory)
    try:
        receipt = json.loads((directory / 'map-ambience-generated.json').read_text(encoding='utf-8'))
        if receipt.get('audition_only') is not False:
            return ['Private ambience audition is not a shipping-approved package']
        if receipt['recipe_sha256'] != generated_addons.recipe_hash(repo,'map-ambience'):
            return ['Generated ambience predates its current recipe; rebuild the bundle']
        palette, selected = selection(repo)
        if receipt['maps'] != palette['maps'] or {r['key'] for r in receipt['clips']} != set(selected) or len(receipt['clips']) != len(selected):
            return ['Generated ambience inventory differs from the fixed palette']
        registry = json.loads((Path(repo) / 'tools/map_ambience/source-approvals.json').read_text(encoding='utf-8'))
        out = []
        for r in receipt['clips']:
            pinned = selected[r['key']]
            if r['input_sha256'] != pinned['input_sha256'] or r['source_id'] != pinned['source_id']:
                out.append(r['key']+': recording differs from pinned source')
            out.extend(source_problems(registry,pinned['input_sha256']))
            if r.get('source_approval') != approval_receipt(registry,pinned['input_sha256']):
                out.append(r['key']+': packaged source approval evidence differs from the registry')
            clip = directory / 'assets/clips' / (r['key']+'.wav')
            if sha(clip.read_bytes()) != r['processed_sha256']:
                out.append(r['key']+': processed recording missing or changed')
        expected_policy = {'package.json':(Path(repo)/'tools/map_ambience/package.json').read_bytes()}
        for name, wanted in expected_policy.items():
            if (directory / name).read_bytes().replace(b'\r\n',b'\n') != wanted.replace(b'\r\n',b'\n'):
                out.append(name+': generated policy changed')
        actual = {p.relative_to(directory).as_posix():sha(p.read_bytes()) for p in directory.rglob('*') if p.is_file() and p.name != 'map-ambience-generated.json'}
        required = {'package.json','assets/weapons.json'} | {'assets/clips/'+key+'.wav' for key in selected}
        if set(actual) != required:
            out.append('Generated ambience has missing or unexpected files')
        if actual != receipt['files']:
            out.append('Generated ambience files differ from complete receipt')
        rules=directory.parent/'map_ambience_rules'
        expected_rules={'palette.rhai':policy(palette).encode()}
        expected_rules.update({name:(Path(repo)/'tools/map_ambience'/source).read_bytes() for name,source in [
            ('package.json','rules-package.json'),('behaviour.json','behaviour.json'),('ambience.rhai','ambience.rhai')]})
        rules_actual={p.relative_to(rules).as_posix():sha(p.read_bytes()) for p in rules.rglob('*') if p.is_file()}
        if set(rules_actual)!=set(expected_rules) or rules_actual!=receipt['rules_files']:
            out.append('Generated ambience server companion missing or changed')
        for name,wanted in expected_rules.items():
            if (rules/name).read_bytes().replace(b'\r\n',b'\n')!=wanted.replace(b'\r\n',b'\n'):
                out.append(name+': generated server policy changed')
        return out
    except (OSError,ValueError,KeyError,TypeError) as error:
        return [f'Incomplete generated ambience: {error}']


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cache',type=Path,default=REPO/'artifacts/map-ambience/inputs')
    parser.add_argument('--content',type=Path,default=REPO/'content')
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--download',action='store_true')
    parser.add_argument('--audition-only',action='store_true')
    args = parser.parse_args()
    result = build(args.cache,args.output,args.content,args.download,args.audition_only)
    print(f"Prepared {len(result['clips'])} clips for {len(result['maps'])} maps; audition_only={result['audition_only']}")
