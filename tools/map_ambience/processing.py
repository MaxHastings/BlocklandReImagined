"""Shared DSP for Maxwell's selected recessed-wind treatment; no gain trim."""
import io
import math


def recessed_pcm(pcm, rate, periodic=False):
    import numpy as np
    from scipy.signal import butter, sosfilt
    amp=10**(-4/40)
    w=2*math.pi*2500/rate
    c=math.cos(w)
    alpha=math.sin(w)/2*math.sqrt(2)
    root=2*math.sqrt(amp)*alpha
    num=[amp*((amp+1)+(amp-1)*c+root),-2*amp*((amp-1)+(amp+1)*c),amp*((amp+1)+(amp-1)*c-root)]
    den=[(amp+1)-(amp-1)*c+root,2*((amp-1)-(amp+1)*c),(amp+1)-(amp-1)*c-root]
    sos=np.vstack([np.array(num+den)/den[0],butter(1,8000,'lowpass',fs=rate,output='sos')])
    if periodic:
        # Warm on one full period, then preserve state over the actual wrap.
        _,state=sosfilt(sos,pcm,axis=0,zi=np.zeros((len(sos),2,pcm.shape[1])))
        return sosfilt(sos,pcm,axis=0,zi=state)[0]
    return sosfilt(sos,pcm,axis=0)


def recessed_loop(raw):
    import soundfile as sf
    pcm,rate=sf.read(io.BytesIO(raw),always_2d=True,dtype='float64')
    output=io.BytesIO()
    sf.write(output,recessed_pcm(pcm,rate,periodic=True),rate,format='WAV',subtype='PCM_16')
    return output.getvalue()
