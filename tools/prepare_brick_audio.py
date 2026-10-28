#!/usr/bin/env python3
"""Rebuild recorded brick foley from pinned public CC0 previews (offline tool).
WAV exports are committed; neither ffmpeg nor network access is needed to play.
"""
import argparse, array, hashlib, json, math, subprocess, sys, urllib.request, wave
from pathlib import Path

SOURCES = [{'id': 233654, 'author': 'rioforce', 'page': 'https://freesound.org/people/rioforce/sounds/233654/', 'license': 'CC0-1.0', 'license_url': 'https://creativecommons.org/publicdomain/zero/1.0/', 'preview': 'https://cdn.freesound.org/previews/233/233654_4067126-hq.mp3', 'sha256': 'ce0b834ae76fc968b2ae8b799bc9eb42bf8d5e18d723b0ebc72171203a133f03'}, {'id': 233653, 'author': 'rioforce', 'page': 'https://freesound.org/people/rioforce/sounds/233653/', 'license': 'CC0-1.0', 'license_url': 'https://creativecommons.org/publicdomain/zero/1.0/', 'preview': 'https://cdn.freesound.org/previews/233/233653_4067126-hq.mp3', 'sha256': '6b2e537f3330ae9e076f834fe5a2e463c99252cc6105e915ab83596826b46a04'}, {'id': 233661, 'author': 'rioforce', 'page': 'https://freesound.org/people/rioforce/sounds/233661/', 'license': 'CC0-1.0', 'license_url': 'https://creativecommons.org/publicdomain/zero/1.0/', 'preview': 'https://cdn.freesound.org/previews/233/233661_4067126-hq.mp3', 'sha256': 'e2d5d8ec42f318149109433bbfc318f380460910bc086c1e1968f0cb534dc664'}, {'id': 721040, 'author': 'C_Sander', 'page': 'https://freesound.org/people/C_Sander/sounds/721040/', 'license': 'CC0-1.0', 'license_url': 'https://creativecommons.org/publicdomain/zero/1.0/', 'preview': 'https://cdn.freesound.org/previews/721/721040_2853872-hq.mp3', 'sha256': '83001b9e3026e71d01e917054f074be8ff7feeafd9d830b660ab619666555bfe'}, {'id': 721039, 'author': 'C_Sander', 'page': 'https://freesound.org/people/C_Sander/sounds/721039/', 'license': 'CC0-1.0', 'license_url': 'https://creativecommons.org/publicdomain/zero/1.0/', 'preview': 'https://cdn.freesound.org/previews/721/721039_2853872-hq.mp3', 'sha256': '47ecb67f32990fd7cbbf461471b57844e052815adc668a7ed5b78e20cdf6f708'}]

# name, source, start/end seconds, normalized peak dBFS.
CLIPS = [
    ("snap1",233654,0.0,0.0299,-6),
    ("snap2",233653,0.0,0.0606,-6),
    ("click1",721040,0.42,0.64,-6),
    ("click2",721040,2.20,2.42,-6),
    ("click3",721040,6.50,6.71,-6),
    ("click4",721040,9.70,9.91,-6),
    ("click5",721040,13.60,13.81,-6),
    ("click6",721040,17.47,17.69,-6),
    ("landing1",233661,0.00,0.85,-9),
    ("landing2",233661,0.10,1.10,-9),
    ("landing3",233661,0.02,0.92,-9),
]
RATE = 48000
NAMESPACE = "brick-destruction-audio"

def digest(data):
    return hashlib.sha256(data).hexdigest()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inputs',type=Path,required=True)
    parser.add_argument('--ffmpeg',required=True)
    parser.add_argument('--output',type=Path,default=Path('packages/brick-destruction-audio'))
    parser.add_argument('--fetch',action='store_true',help='Fetch missing, publicly playable HQ MP3 previews')
    args=parser.parse_args()
    args.inputs.mkdir(parents=True,exist_ok=True)
    samples={}
    used={identity for _,identity,_,_,_ in CLIPS}
    for source in SOURCES:
        if source['id'] not in used:
            continue
        path=args.inputs/f"{source['id']}.mp3"
        if not path.exists() and args.fetch:
            with urllib.request.urlopen(source['preview'],timeout=30) as response:
                data=response.read()
            if digest(data)!=source['sha256']:
                raise ValueError(f"Input hash changed: {source['id']}")
            path.write_bytes(data)
        data=path.read_bytes()
        if digest(data)!=source['sha256']:
            raise ValueError(f"Input hash mismatch: {path}")
        raw=subprocess.run([args.ffmpeg,'-hide_banner','-loglevel','error','-i',str(path),
            '-ac','1','-ar',str(RATE),'-f','f32le','-c:a','pcm_f32le','-'],check=True,capture_output=True).stdout
        decoded=array.array('f');decoded.frombytes(raw)
        if sys.byteorder!='little': decoded.byteswap()
        samples[source['id']]=decoded
    sounds=args.output/'assets/sounds';sounds.mkdir(parents=True,exist_ok=True)
    outputs=[]
    for name,identity,start,end,target in CLIPS:
        clip=list(samples[identity][round(start*RATE):round(end*RATE)])
        if not clip or len(clip)!=round(end*RATE)-round(start*RATE):
            raise ValueError(f"Invalid trim: {name}")
        dc=sum(clip)/len(clip);clip=[v-dc for v in clip]
        fade_in=min(24,len(clip)//4)
        fade_out=min(round((.020 if name.startswith('landing') else .005)*RATE),len(clip)//3)
        for i in range(fade_in): clip[i]*=i/fade_in
        for i in range(fade_out): clip[-1-i]*=i/fade_out
        peak=max(map(abs,clip))
        if not math.isfinite(peak) or peak<.02:
            raise ValueError(f"Missing/quiet transient: {name}")
        window=round((.100 if name.startswith('landing') else .020)*RATE)
        active_rms=max(math.sqrt(sum(v*v for v in clip[i:i+window])/min(window,len(clip)-i)) for i in range(0,len(clip),max(1,window//2)))
        rms_target=-22 if name.startswith('landing') else -20
        gain=min(4.,10**(target/20)/peak,10**(rms_target/20)/max(active_rms,1e-9))
        pcm=array.array('h',(round(max(-.9999,min(.9999,v*gain))*32767) for v in clip))
        if sys.byteorder!='little':pcm.byteswap()
        path=sounds/f'{name}.wav'
        with wave.open(str(path),'wb') as wav:
            wav.setnchannels(1);wav.setsampwidth(2);wav.setframerate(RATE);wav.writeframes(pcm.tobytes())
        peak_out=max(abs(v*gain) for v in clip)
        rms=math.sqrt(sum((v*gain)**2 for v in clip)/len(clip))
        record=dict(file=f'assets/sounds/{name}.wav',source=identity,trim=[start,end],
            modifications='mono 48kHz PCM16; DC removal; 0.5ms entrance/5ms exit (20ms exit on landing); bounded peak/active-RMS gain only',
            peak_dbfs=round(20*math.log10(peak_out),2),rms_dbfs=round(20*math.log10(rms),2),sha256=digest(path.read_bytes()))
        outputs.append(record)
        print(f"{name}: {len(clip)/RATE:.3f}s, peak {record['peak_dbfs']:.2f}dBFS, RMS {record['rms_dbfs']:.2f}dBFS")
    profiles={}
    for name,_,_,_,_ in CLIPS:
        profiles[f'{NAMESPACE}:{name}']=dict(file=f'sounds/{name}.wav')
    for kind,volume,names in [('snap',.55,['snap1','snap2']),('impact',.65,[f'click{i}' for i in range(1,7)]),('landing',.75,[f'landing{i}' for i in range(1,4)])]:
        profiles[f'{NAMESPACE}:{kind}']=dict(file=f'sounds/{names[0]}.wav',volume=volume,
            variants=[dict(profile=f'{NAMESPACE}:{name}',weight=1) for name in names])
    pack=dict(schema_version=5,id=NAMESPACE,sounds=profiles,effects=dict(brick_debris_audio=dict(
        id=f'{NAMESPACE}:audio/foley',
        snap=dict(profile=f'{NAMESPACE}:snap',radius=24),impact=dict(profile=f'{NAMESPACE}:impact',radius=18),
        landing=dict(profile=f'{NAMESPACE}:landing',radius=40),min_speed=1.2,full_speed=8,cooldown=.14,landing_min_members=8,pitch=[.94,1.06])))
    (args.output/'assets/weapons.json').write_text(json.dumps(pack,indent=2)+'\n',encoding='utf-8',newline='\n')
    provenance=dict(format='Public Freesound HQ MP3 previews, not original WAV masters',
        processor=subprocess.run([args.ffmpeg,'-version'],check=True,capture_output=True,text=True).stdout.splitlines()[0],
        sources=[s for s in SOURCES if s['id'] in used],outputs=outputs)
    (args.output/'SOURCES.json').write_text(json.dumps(provenance,indent=2)+'\n',encoding='utf-8',newline='\n')
    manifest=dict(schema_version=1,id=NAMESPACE,version='1.0.0',api=1,name='Brick Destruction Audio',
        description='Restrained recorded plastic clicks and clatter for local brick destruction.',
        authors=['Blockland ReImagined','rioforce','C_Sander'],license='CC0-1.0',
        provenance=dict(source='adapted CC0 field recordings',details='SOURCES.json'),
        provides=[dict(kind='weapons',id=f'{NAMESPACE}:weapons/main',file='assets/weapons.json')])
    (args.output/'package.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8',newline='\n')

if __name__=='__main__':main()
