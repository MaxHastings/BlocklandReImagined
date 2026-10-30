#!/usr/bin/env python3
"""Prepare source-pinned listening examples of sparse ambience details.

Uses openly acquired original PCM WAVs. This offline illustrative mix does not
claim that in-game scheduling/spatial acoustics are installed or accepted.
"""
import argparse
import hashlib
import json
from pathlib import Path
from map_ambience.sources import ensure_master

HERE = Path(__file__).resolve().parent / 'map_ambience'
RATE = 48000


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build(cache, output, selected_only=False, download=False):
    import numpy as np
    import soundfile as sf
    from scipy.signal import butter, sosfilt, resample_poly
    import math

    output.mkdir(parents=True, exist_ok=True)
    masters = json.loads((HERE / 'masters.json').read_text(encoding='utf-8'))
    by_key = {row['key']: row for row in masters}
    receipts = []

    def source(key, seconds=None, rms_db=-34, mono=False):
        row = by_key[key]
        path = ensure_master(row,cache,download)
        if sha(path) != row['input_sha256']:
            raise ValueError(f'{key}: original recording changed')
        pcm, rate = sf.read(path, dtype='float64', always_2d=True)
        if not np.isfinite(pcm).all():
            raise ValueError('non-finite recording')
        if seconds is not None:
            pcm = pcm[:int(seconds * rate)]
        pcm = sosfilt(butter(2, 60, 'highpass', fs=rate, output='sos'), pcm, axis=0)
        divisor = math.gcd(rate, RATE)
        pcm = resample_poly(pcm, RATE // divisor, rate // divisor, axis=0)
        if mono:
            pcm = pcm.mean(axis=1, keepdims=True)
        elif pcm.shape[1] == 1:
            pcm = np.repeat(pcm, 2, axis=1)
        rms = float(np.sqrt(np.mean(pcm * pcm)))
        gain = min(10 ** (rms_db / 20) / max(rms, 1e-8),
                   0.1 / max(float(np.abs(pcm).max()), 1e-8))
        pcm *= gain
        fade = min(int(RATE * 0.025), len(pcm) // 4)
        pcm[:fade] *= np.linspace(0, 1, fade)[:, None]
        pcm[-fade:] *= np.linspace(1, 0, fade)[:, None]
        return pcm, dict(source_key=key, source=row['source'], author=row['author'],
                         license=row['license'], input_sha256=row['input_sha256'],
                         start=0, seconds=len(pcm) / RATE,
                         gain_db=20 * math.log10(max(gain, 1e-8)))

    def write(name, pcm, inputs, treatment):
        path = output / (name + '.wav')
        sf.write(path, pcm, RATE, subtype='PCM_16')
        receipts.append(dict(file=path.name, sha256=sha(path), seconds=len(pcm)/RATE,
                             peak_dbfs=20 * math.log10(max(float(np.abs(pcm).max()), 1e-8)),
                             inputs=inputs, treatment=treatment,
                             quality='Derived directly from original PCM WAV inputs',
                             acceptance='Human listening pending'))

    robin, r = source('robin', rms_db=-29, mono=True)
    blackbird, b = source('blackbird', rms_db=-29, mono=True)
    write('bird-robin', robin, [r], 'Whole original call, mono, gentle edge fades; no pitch change.')
    write('bird-blackbird', blackbird, [b], 'Whole original call, mono, gentle edge fades; no pitch change.')
    if selected_only:
        return receipts
    cricket, c = source('cricket', seconds=7, rms_db=-33, mono=True)
    grass, g = source('grass-master', seconds=18, rms_db=-36)
    fridge, f = source('fridge-master', seconds=14, rms_db=-35, mono=True)
    write('cricket-phrase', cricket, [c], 'First7s excerpt, mono, gentle edge fades; alternative detail, never stacked over Forest Dark original crickets.')
    write('grass-movement', grass, [g], 'First18s excerpt, stereo; foliage-local texture candidate.')
    write('fridge-local', fridge, [f], 'First14s excerpt, mono; localized appliance candidate, not a map-wide bed.')

    # Fixed event times make the illustration reproducible. The eventual game
    # policy will choose bounded varied intervals/locations with no immediate
    # repeat; this is a timing comparison, not a runtime-system demonstration.
    mix = np.zeros((45 * RATE, 2))
    mix[:len(grass)] += grass
    # Extend the texture with another real source excerpt, avoiding a 3s loop.
    texture, tex = source('grass-master', seconds=45, rms_db=-39)
    mix[:] = texture[:len(mix)]
    for at, clip, pan in [(4, robin, -0.5), (22, blackbird, 0.55), (39, robin, -0.2)]:
        gains = np.sqrt(np.array([(1-pan)/2, (1+pan)/2]))
        start = at * RATE
        mix[start:start+len(clip)] += clip * gains
    write('outdoor-details-example', mix, [tex, r, b],
          '45s illustration: quiet grass plus calls at4s,22s,39s with different stereo positions. Fixed example schedule, not in-game scheduling or map acceptance.')
    (output / 'details-provenance.json').write_text(json.dumps(receipts, indent=2)+'\n', encoding='utf-8')
    return receipts


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cache', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    for row in build(args.cache, args.output):
        print(row['file'], row['seconds'], row['peak_dbfs'])
