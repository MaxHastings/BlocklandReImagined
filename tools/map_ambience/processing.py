"""Shared DSP for Maxwell's selected recessed-wind treatment; no gain trim."""
import io
import math


def user_loop(raw):
    import numpy as np
    import soundfile as sf
    pcm,rate=sf.read(io.BytesIO(raw),always_2d=True,dtype='float64')
    if not np.isfinite(pcm).all() or np.abs(pcm).max()>=1:
        raise ValueError('Invalid user base PCM')
    overlap=round(rate*0.25)
    if len(pcm)<overlap*4:
        raise ValueError('User base too short for the selected loop seam')
    mix=np.arange(overlap)[:,None]/overlap
    pcm=np.concatenate((pcm[overlap:-overlap],pcm[-overlap:]*(1-mix)+pcm[:overlap]*mix))
    output=io.BytesIO()
    # PCM24 avoids libsndfile's timestamped FLOAT/PEAK container metadata.
    sf.write(output,pcm,rate,format='WAV',subtype='PCM_24')
    return output.getvalue(),{'overlap_seconds':0.25,'gain':1.0,'eq':'unchanged','rate':rate,
                              'channels':pcm.shape[1],'frames':len(pcm),'normalization':False}


def user_mix_gain(raw, gain_db):
    """Apply the requested mix gain after the approved unity loop, without limiting."""
    import numpy as np
    import soundfile as sf
    if not math.isfinite(gain_db):
        raise ValueError('Nonfinite user mix gain')
    pcm,rate=sf.read(io.BytesIO(raw),always_2d=True,dtype='float64')
    amplitude=10**(gain_db/20)
    pcm=pcm*amplitude
    if not np.isfinite(pcm).all() or np.abs(pcm).max()>=1:
        raise ValueError('Requested user mix gain would clip; no normalization applied')
    output=io.BytesIO()
    sf.write(output,pcm,rate,format='WAV',subtype='PCM_24')
    return output.getvalue(),{'gain_db':gain_db,'amplitude':amplitude,'normalization':False,
                              'limiting':False,'eq':'unchanged','rate':rate,
                              'channels':pcm.shape[1],'frames':len(pcm),
                              'peak':float(np.abs(pcm).max())}


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
