"""Rebuild the local playtest derivative from Maxwell's accepted stereo mix.

Usage: python assets/blockhead-jets/make_jet.py /path/to/jet-sustain-less-hiss-toothbrush.wav
Source recordings and the author's editable mix stay outside the repository.
Requires NumPy and SoundFile, only for offline asset authoring.
"""
from pathlib import Path
import hashlib
import sys
import wave
import numpy as np
import soundfile as sf

ROOT = Path(__file__).resolve().parent
EXPECTED = 'd1628f59d9c0aff67d724562ddce839e3208e84a39caaa6fb4e1effb548b503e'
source = Path(sys.argv[1])
assert hashlib.sha256(source.read_bytes()).hexdigest() == EXPECTED, 'Wrong authored source revision'
x, rate = sf.read(source, always_2d=True)
x = x.mean(axis=1)[round(1.5*rate):round(7.35*rate)]
cross = round(.08*rate)
start = round(.25*rate)
angle = np.linspace(0, np.pi/2, cross)
clip = np.concatenate([x[:-cross], x[-cross:]*np.cos(angle)+x[start:start+cross]*np.sin(angle)])
assert np.isfinite(clip).all() and max(abs(clip)) < .5
sf.write(ROOT/'jet.wav', clip, rate, subtype='PCM_16')
assert hashlib.sha256((ROOT/'jet.wav').read_bytes()).hexdigest() == 'b055b73e23f0b23535bb3175188b56b8071ae09e661c8a47ff13a4598d730710'
with wave.open(str(ROOT/'jet.wav'), 'rb') as f:
    params = f.getparams()
    pcm = np.frombuffer(f.readframes(f.getnframes()), dtype='<i2').astype(np.float64)
for gain_db in [4.0, 4.0, 3.0]:
    pcm = np.rint(pcm * 10**(gain_db/20))
assert np.isfinite(pcm).all() and max(abs(pcm)) < 32767
with wave.open(str(ROOT/'jet.wav'), 'wb') as f:
    f.setparams(params)
    f.writeframes(pcm.astype('<i2').tobytes())
assert hashlib.sha256((ROOT/'jet.wav').read_bytes()).hexdigest() == '97635d15d97765efea7e2f2a93d8c98665fef4d06191f9bcb8ff98dfce52b8b7'
print('Verified authored intro and held-loop asset.')
