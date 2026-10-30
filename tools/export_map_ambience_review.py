"""Export the finite ambience palette for listening and external DAW editing.

Reads generated packages; never changes game content or original sources.
Editable WAV samples retain their level, channels and sample rate. Profile gain
stays separate so edited files can return without applying the game gain twice.
"""
import argparse
import csv
import hashlib
import html
import io
import json
from pathlib import Path
import shutil

import build_map_ambience as builder


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def priority_slots(output):
    """Independent editing names, never a claim of eleven unique recordings."""
    directory=output/'00-Priority-Map-Tracks'
    directory.mkdir()
    slots=[]
    for name in ['Bedroom','Kitchen']:
        for time in ['Day','Night']:
            for space,source in [('Indoor','room'),('Outdoor','dry-air')]:
                slots.append((f'{name}-{time}-{space}.wav',source,f'{name} / {time} / {space}'))
    slots += [('Slopes-Outdoor.wav','cold-air','The Slopes / all outdoors'),
              ('Slate-Outdoor.wav','dry-air','Slate / outdoors'),
              ('Slate-Desert-Outdoor.wav','dry-air','Slate Desert / outdoors')]
    rows=[]
    for name,source,placement in slots:
        target=directory/name
        source_file=name.lower() if (output/'01-Editable-WAVs'/name.lower()).is_file() else source+'.wav'
        shutil.copyfile(output/'01-Editable-WAVs'/source_file,target)
        rows.append({'editable_file':name,'placement':placement,'starting_source':source_file,
                     'status':'Independent editing slot; current approved starting sample, not a newly distinct recording',
                     'sha256':sha(target.read_bytes())})
    with (output/'PRIORITY-TRACKS.csv').open('w',newline='',encoding='utf-8-sig') as f:
        writer=csv.DictWriter(f,fieldnames=list(rows[0]))
        writer.writeheader();writer.writerows(rows)
    return rows


def export(candidate, preview_content, cache, output):
    import numpy as np
    import soundfile as sf

    if output.exists():
        raise ValueError('Choose a new review folder; never overwrite Ableton edits')
    issues=builder.problems(builder.REPO,candidate)
    if issues:
        raise ValueError('; '.join(issues))
    receipt=json.loads((candidate/'map-ambience-generated.json').read_text())
    palette=receipt['maps']
    assets=json.loads((candidate/'assets/weapons.json').read_text())['sounds']
    map_names={m['map']:m['name'] for m in palette}
    records=[]
    placements=[]

    def place(key,m,status,pattern,position=None,reference='',maximum='',note=''):
        placements.append({'sound':key+'.wav','map':map_names.get(m,m),'map_id':m,
                           'status':status,'playback':pattern,'position_xyz':json.dumps(position) if position is not None else 'listener-relative / diffuse',
                           'reference_distance':reference,'max_distance':maximum,'notes':note})

    for row in receipt['clips']:
        key=row['key']
        definition=assets['map_ambience:'+key]
        records.append({'key':key,'raw':(candidate/row['file']).read_bytes(),'source':cache/row['input'],
                        'source_sha256':row['input_sha256'],'definition':definition,
                        'author':row['author'],'license':row['license'],'source_url':row['source'],
                        'quality':row.get('quality','See source provenance'),'state':'Generated r7 candidate; integrated listening pending'})
        for m in palette:
            if key in m['beds']:
                place(key,m['map'],'Current candidate','continuous loop',note=definition.get('environment','neutral')+' context; 2-second entry fade')
            if key in m['details']:
                for at in m['anchors']:
                    place(key,m['map'],'Current candidate','one sparse call every 35–85 seconds; alternates recordings and anchors',at,12,160,'Only near living human listeners; no invented tree offsets')
            for local in m.get('locals',[]):
                if local['clip']==key:
                    spatial=definition['spatial']
                    place(key,m['map'],'Current candidate','continuous local loop',local['at'],spatial['reference_distance'],spatial['max_distance'],'Measured FRIDGE1 surface; ordinary distance/obstruction, no listener context gate')

    # Original map media is read from the verified, isolated preview, not from
    # a mutable source installation or an archive lookup by guessed filename.
    for package in ['map_creek','map_forest']:
        root=preview_content/'addons'/package
        provenance=json.loads((root/'map-audio-provenance.json').read_text())
        definitions=json.loads((root/'assets/weapons.json').read_text())['sounds']
        for row in provenance['sources']:
            key=package+'-'+row['name'].lower()
            definition=definitions[row['profile']]
            source=root/'assets'/definition['file']
            raw=source.read_bytes()
            if sha(raw)!=row['sha256']:
                raise ValueError('Original map recording differs from verified provenance')
            records.append({'key':key,'raw':raw,'source':source,'source_sha256':row['sha256'],
                            'definition':definition,'author':'Original bundled map authors; see source-approvals.json',
                            'license':provenance['license'],'source_url':row['source'],
                            'quality':'Original map WAV; exact original bytes copied to source folder',
                            'state':'Restored original; present in preview1 and retained'})
            spatial=row['spatial']
            place(key,row['map'],'Restored original','continuous authored local loop',row['position'],spatial['reference_distance'],spatial['max_distance'],
                  'Original emitter node '+str(row['node'])+'; game profile gain '+str(definition.get('volume',1)))

    choices=json.loads((builder.HERE/'followup-selections.json').read_text())['clips']
    for key in ['desert-gust-a','desert-gust-b']:
        choice=next(c for c in choices if c['id']==key)
        source=builder.REPO/'artifacts/map-ambience/followup-audition'/choice['file']
        raw=source.read_bytes()
        if choice['approval']!='approved' or sha(raw)!=choice['sha256']:
            raise ValueError('Approved gust audition missing or changed')
        wind=choice['source']
        records.append({'key':key,'raw':raw,'source':cache/wind['input'],'source_sha256':wind['input_sha256'],
                        'definition':{'volume':1.0,'looping':False,'environment':'exterior'},
                        'author':wind['author'],'license':wind['license'],'source_url':wind['source'],
                        'quality':choice['quality'],'state':'Previously approved excerpt; reference only after simpler-bed direction, not scheduled for integration'})

    for m in palette:
        if m['name'] in {'Bedroom','Bedroom - Dark','Kitchen','Kitchen - Dark'} and len(m['beds'])==1:
            place('dry-air',m['map'],'Planned context follow-up','continuous exterior loop',note='Approved recessed wind outside; complements Interior room bed; not yet added to r7 policy')

    # Validate every source before creating the deliverable directory.
    for r in records:
        if sha(r['source'].read_bytes())!=r['source_sha256']:
            raise ValueError('Pinned source changed: '+r['key'])
    output.mkdir(parents=True)
    edits=output/'01-Editable-WAVs';edits.mkdir()
    originals=output/'02-Original-Sources';originals.mkdir()
    evidence=output/'03-Provenance';evidence.mkdir()
    source_copies={}
    cards=[]
    manifest=[]
    for r in records:
        key=r['key']
        pcm,rate=sf.read(io.BytesIO(r['raw']),always_2d=True,dtype='float32')
        if not len(pcm) or not np.isfinite(pcm).all() or np.abs(pcm).max()>=1:
            raise ValueError('Invalid or clipped export input: '+key)
        path=edits/(key+'.wav')
        sf.write(path,pcm,rate,format='WAV',subtype='FLOAT')
        reread,rrate=sf.read(path,always_2d=True,dtype='float32')
        if rrate!=rate or not np.array_equal(pcm,reread):
            raise ValueError('Editable export changed decoded samples: '+key)
        source_hash=r['source_sha256']
        if source_hash not in source_copies:
            name=r['source'].name
            if (originals/name).exists():name=source_hash[:12]+'-'+name
            shutil.copyfile(r['source'],originals/name)
            source_copies[source_hash]='02-Original-Sources/'+name
        gain=r['definition'].get('volume',1.0)
        entry={k:v for k,v in r.items() if k not in {'raw','source'}}
        entry.update(file='01-Editable-WAVs/'+key+'.wav',sha256=sha(path.read_bytes()),
                     original_file=source_copies[source_hash],decoded_input_sha256=sha(r['raw']),
                     sample_rate=rate,channels=pcm.shape[1],frames=len(pcm),seconds=len(pcm)/rate,
                     format='32-bit float WAV, exact decoded samples; not a higher-quality source',
                     game_gain_separate=gain)
        manifest.append(entry)
        sites=[p for p in placements if p['sound']==key+'.wav']
        unique=list(dict.fromkeys(p['map']+' ('+p['status']+')' for p in sites))
        cards.append('<article><h2>'+html.escape(key)+'</h2><audio controls preload="none" data-gain="'+str(gain)+'" src="'+entry['file']+'"></audio><p>'+html.escape('; '.join(unique))+'</p><p>'+html.escape(r['quality'])+'</p><small>Profile gain '+str(gain)+' applied by this player only. Spatial distance, obstruction and other game sounds are absent.</small></article>')

    with (output/'PLACEMENTS.csv').open('w',newline='',encoding='utf-8-sig') as f:
        writer=csv.DictWriter(f,fieldnames=list(placements[0]))
        writer.writeheader();writer.writerows(placements)
    priority=priority_slots(output)
    (output/'manifest.json').write_text(json.dumps({'candidate_recipe_sha256':receipt['recipe_sha256'],
            'latest_human_direction':'11 simple priority beds; retain Kitchen fridge and original proximity/waterfall emitters; other maps keep current setup; no extra random events for priority maps',
            'priority_editing_slots':priority,'sounds':manifest,'placements':placements},indent=2)+'\n')
    for name in ['source-approvals.json','followup-selections.json','palette.json','priority-slots.json','requested-desert-source.json']:
        shutil.copyfile(builder.HERE/name,evidence/name)
    shutil.copyfile(candidate/'map-ambience-generated.json',evidence/'map-ambience-generated.json')
    for package in ['map_creek','map_forest']:
        shutil.copyfile(preview_content/'addons'/package/'map-audio-provenance.json',evidence/(package+'-provenance.json'))
    (output/'LISTEN.html').write_text('<!doctype html><meta charset="utf-8"><title>Ambience review</title><style>body{background:#171c22;color:#eef;font:18px system-ui;max-width:1000px;margin:40px auto;padding:20px}article{padding:16px;margin:16px 0;background:#252c35}audio{width:100%}</style><h1>All ambience sounds for review</h1><p>15 editable clips from13 recordings. Planned placements are identified explicitly. No YouTube audio. These are solo references, not final game-mix acceptance.</p>'+''.join(cards)+'<script>document.querySelectorAll("audio").forEach(a=>{a.volume=Number(a.dataset.gain);a.addEventListener("play",()=>document.querySelectorAll("audio").forEach(b=>{if(b!==a)b.pause()}))})</script>',encoding='utf-8')
    quiet=', '.join(m['name'] for m in palette if not m['beds'] and not m['details'] and not m.get('locals') and m['name'] not in {'Creek','Forest - Dark'})
    (output/'START-HERE.txt').write_text('AMBIENCE / ABLETON REVIEW\n\n01-Editable-WAVs: drag these15 files into Ableton. 32-bit float WAVs preserve decoded samples exactly; source sample rate/channels retained. MP3 or Vorbis sources do not become lossless originals through this export.\n02-Original-Sources:13 exact pinned recordings, before current processing where available. MP3 preview sources remain MP3; original WAV sources remain original WAV.\n03-Provenance: source/approval/hash records and current candidate policy.\nLISTEN.html: solo playback at separate game profile gain.\nPLACEMENTS.csv: exact current/planned maps, coordinates, ranges and event patterns. Every Forest bird anchor has its own row. Coordinates use game X/Y/Z; Y is up.\nmanifest.json: exact identities, formats, hashes and separate game gains.\n\nEDITING\nDisable Auto-Warp Long Samples before importing these ambience beds, or turn Warp off on each clip. Keep natural timing; no beat sync. Export each changed file under its same base name as WAV, preferably32-bit float or24-bit PCM, preserving rate/channels. Keep copies of your edited exports in a NEW folder so these references remain available.\nDo not bake the listed game profile gain into replacement assets: it remains applied by the game. In particular Creek frogs use0.8 and Forest Dark crickets0.5. Other clips here use1.0. No loudness normalization was added by this export. If you intentionally change the level, describe it when returning the export. Full loop duration edits are allowed but require a new seam/lifecycle check.\n\nSTATUS\nCurrent r7 candidate has9 selected clips; restored Creek/Forest add4. The2 approved gust excerpts are provided for editing but are not integrated in r7 yet. Planned exterior wind for Bedroom/Kitchen variants is listed separately from current placements. Interior/exterior beds blend through bounded shelter detection; fixed sources use distance/obstruction. Final combined gameplay balance is still pending Maxwell listening.\nThe requested YouTube recording was withdrawn by Maxwell and is excluded.\nDeliberate quiet maps: '+quiet+'.\n\nEditing/reviewing this folder does not alter the installed game or preview1.\n',encoding='utf-8')
    guide=output/'START-HERE.txt'
    text=guide.read_text()
    text=('LATEST DIRECTION\nStart in 00-Priority-Map-Tracks:11 independently named files for Bedroom/Kitchen Indoor+Outdoor Day+Night, Slopes outdoor, Slate and Slate Desert. These are EDITING SLOTS seeded from the existing approved room/wind beds. Several currently contain the SAME samples; day/night and Slate/Desert are NOT falsely claimed to already be unique recordings. Edit these named copies independently to make the versions you want. PRIORITY-TRACKS.csv maps every slot. Kitchen fridge and all original proximity emitters stay separate and retained. No new random bird/gust events are planned for these focused maps. Other maps keep their current setup.\n\n'+text)
    text=text.replace('The2 approved gust excerpts are provided for editing but are not integrated in r7 yet.',
                      'The2 previously approved gust excerpts are reference only after the simpler-bed direction; they are not scheduled for integration.')
    text=text.replace('these15 files',f'these {len(records)} files')
    text=text.replace('Current r7 candidate has9 selected clips',f'Current generated candidate has {len(receipt["clips"])} selected clips')
    text=text.replace('Planned exterior wind for Bedroom/Kitchen variants is listed separately from current placements.',
                      'The placement table distinguishes current generated policy from any pending context follow-up.')
    guide.write_text(text,encoding='utf-8')
    listen=output/'LISTEN.html'
    listen.write_text(listen.read_text().replace('15 editable clips from13 recordings.',f'{len(records)} editable/reference clips from {len(source_copies)} recordings. Earlier gusts are references only after the simpler-bed direction.'),encoding='utf-8')
    (output/'ABLETON-NOTES.txt').write_text('Turn Warp OFF to preserve natural ambience timing. Ableton official reference: https://www.ableton.com/en/live-manual/12/audio-clips-tempo-and-warping/\nExport changed slots as WAV under the same names to a new folder; preserve channels/rate, no normalize.32-bit float or24-bit PCM are suitable. Profile gains remain separate; do not bake them twice.\n',encoding='utf-8')
    return {'folder':str(output),'editable_clips':len(records),'priority_slots':len(priority),'unique_sources':len(source_copies),'placement_rows':len(placements)}


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate',type=Path,required=True)
    parser.add_argument('--preview-content',type=Path,required=True)
    parser.add_argument('--cache',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    print(json.dumps(export(args.candidate,args.preview_content,args.cache,args.output)))
