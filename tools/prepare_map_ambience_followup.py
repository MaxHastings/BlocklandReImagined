"""Prepare finite hear-first fridge/desert choices and one matched wind A/B.

No game content is changed. New source/excerpt/processing choices stay pending.
"""
import argparse
import hashlib
import html
import io
import json
import math
from pathlib import Path
import subprocess

import build_map_ambience
import prepare_map_ambience_audition

HERE=Path(__file__).resolve().parent/'map_ambience'
RATE=48000


def sha(raw): return hashlib.sha256(raw).hexdigest()


def lufs(raw):
    import imageio_ffmpeg
    r=subprocess.run([imageio_ffmpeg.get_ffmpeg_exe(),'-hide_banner','-i','pipe:0',
                      '-af','loudnorm=I=-30:TP=-1:LRA=11:print_format=json','-f','null','-'],
                     input=raw,stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=True)
    text=r.stderr.decode(errors='replace')
    return float(json.JSONDecoder().raw_decode(text[text.rfind('{'):])[0]['input_i'])


def build(cache,current,output):
    import numpy as np
    import soundfile as sf
    from scipy.signal import butter,sosfilt,find_peaks
    output.mkdir(parents=True,exist_ok=True)
    masters={r['key']:r for r in json.loads((HERE/'masters.json').read_text())}
    candidates={r['key']:r for r in json.loads((HERE/'candidates.json').read_text())['clips']}
    receipts=[]

    def write(name,pcm,source,treatment,**extra):
        out=io.BytesIO();sf.write(out,pcm,RATE,format='WAV',subtype='PCM_16');raw=out.getvalue()
        if not np.isfinite(pcm).all() or np.abs(pcm).max()>=1: raise ValueError('invalid/clipped candidate')
        (output/(name+'.wav')).write_bytes(raw)
        row={'id':name,'file':name+'.wav','sha256':sha(raw),'rate':RATE,'frames':len(pcm),'seconds':len(pcm)/RATE,
             'source':source,'treatment':treatment,'peak_dbfs':20*math.log10(max(float(np.abs(pcm).max()),1e-9)),
             'rms_dbfs':20*math.log10(max(float(np.sqrt(np.mean(pcm*pcm))),1e-9)),
             'approval':'pending user heard choice; not adopted in game',**extra}
        receipts.append(row);return raw,row

    f=masters['fridge-master'];raw=(cache/f['input']).read_bytes()
    if sha(raw)!=f['input_sha256']: raise ValueError('fridge input changed')
    loop,_,processing=prepare_map_ambience_audition.prepare(raw,-35)
    pcm,_=sf.read(io.BytesIO(loop),always_2d=True,dtype='float64')
    write('fridge-loop',pcm,f,'Original PCM WAV;35Hz high-pass,2s overlap loop, provisional-35dBFS RMS. Same recording as earlier local candidate, full loop now auditioned.',processing=processing)

    wind=candidates['dry-air'];raw=(cache/wind['input']).read_bytes()
    if sha(raw)!=wind['input_sha256']:raise ValueError('dry wind source changed')
    prepared,_,_=prepare_map_ambience_audition.prepare(raw,-36)
    pcm,_=sf.read(io.BytesIO(prepared),always_2d=True,dtype='float64')
    seconds=len(pcm)//RATE
    envelope=np.sqrt(np.mean(pcm[:seconds*RATE].reshape(seconds,RATE,pcm.shape[1])**2,axis=(1,2)))
    peaks,_=find_peaks(envelope,distance=35)
    peaks=sorted([int(i) for i in peaks if 10<=i<=seconds-12],key=lambda i:float(envelope[i]),reverse=True)
    if len(peaks)<2:raise ValueError('not enough separate natural gusts')
    for name,center,length in [('desert-gust-a',peaks[0],12),('desert-gust-b',peaks[1],18)]:
        start=center-length//2;clip=pcm[start*RATE:(start+length)*RATE].copy()
        gain=min(10**(-36/20)/float(np.sqrt(np.mean(clip*clip))),0.2/float(np.abs(clip).max()))
        clip*=gain;fade=RATE//2
        clip[:fade]*=np.linspace(0,1,fade)[:,None];clip[-fade:]*=np.linspace(1,0,fade)[:,None]
        write(name,clip,wind,'Natural gust phrase from existing approved dry-air recording; quiet framing fades; no animal/foley synthesis. Isolated choice, not another continuously stacked wind bed.',
              excerpt_of_prepared_loop={'start_seconds':start,'seconds':length,'gain_db':20*math.log10(gain)},
              quality='Original approved input is a public HQ MP3 preview; WAV output does not restore original lossless quality.')

    current_pcm,current_rate=sf.read(current/'assets/clips/dry-air.ogg',always_2d=True,dtype='float64')
    if current_rate!=RATE:raise ValueError('unexpected current game mix rate')
    a=current_pcm[20*RATE:40*RATE].copy()
    # RBJ high shelf -4dB at2.5kHz, followed by gentle8kHz lowpass.
    amp=10**(-4/40);w=2*math.pi*2500/RATE;c=math.cos(w);alpha=math.sin(w)/2*math.sqrt(2);root=2*math.sqrt(amp)*alpha
    num=[amp*((amp+1)+(amp-1)*c+root),-2*amp*((amp-1)+(amp+1)*c),amp*((amp+1)+(amp-1)*c-root)]
    den=[(amp+1)-(amp-1)*c+root,2*((amp-1)-(amp+1)*c),(amp+1)-(amp-1)*c-root]
    shelf=np.array(num+den)/den[0]
    b=sosfilt(np.vstack([shelf,butter(1,8000,'lowpass',fs=RATE,output='sos')]),a,axis=0)
    # Metering compensation is only for the fair A/B, never a master-volume edit.
    def wav(v):
        x=io.BytesIO();sf.write(x,v,RATE,format='WAV',subtype='PCM_16');return x.getvalue()
    la=lufs(wav(a));lb=lufs(wav(b));gain=10**((la-lb)/20);b*=gain
    source={'source':wind['source'],'author':wind['author'],'input_sha256':wind['input_sha256'],
            'game_file_sha256':sha((current/'assets/clips/dry-air.ogg').read_bytes()),'start_seconds':20,'seconds':20}
    write('wind-current',a,source,'Current game wind excerpt, no processing or gain change.',integrated_lufs=la)
    write('wind-recessed',b,source,'Same excerpt/stereo field; high shelf-4dB at2.5kHz and gentle8kHz LPF. No reverb/compression. Loudness matched ONLY for audition.',
          integrated_lufs=lufs(wav(b)),audition_match_gain_db=20*math.log10(gain))
    (output/'provenance.json').write_text(json.dumps(receipts,indent=2)+'\n')
    cards=''.join('<article><h2>'+html.escape(r['id'])+'</h2><audio controls preload="none" src="'+r['file']+'"></audio><p>'+html.escape(r['treatment'])+'</p></article>' for r in receipts)
    (output/'audition.html').write_text('<!doctype html><meta charset="utf-8"><title>Ambience follow-up choices</title><style>body{background:#171c22;color:#eef;font:18px system-ui;max-width:900px;margin:40px auto;padding:20px}article{background:#252c35;padding:16px;margin:16px 0}audio{width:100%}</style><h1>Fridge, desert gust choices and wind A/B</h1><p>No running game/mix changed. Fridge is a needed new source choice; desert phrases and recessed processing need listening selection. Current mix gains and Slate Storm stay fixed.</p>'+cards,encoding='utf-8')
    return receipts


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--cache',type=Path,required=True);p.add_argument('--current',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
    a=p.parse_args()
    for r in build(a.cache,a.current,a.output):print(r['id'],r['seconds'],r['rms_dbfs'])
