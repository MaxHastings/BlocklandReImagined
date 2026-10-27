"""Rebuild selected landing B from supplied Foley; not needed to build the game."""
import argparse
from fractions import Fraction
from pathlib import Path
import hashlib
import io
import json
import math
import wave

import numpy as np
import soundfile as sf
from scipy.signal import butter, resample_poly, sosfilt

ROOT = Path(__file__).resolve().parent
RATE = 48000
LENGTH = round(0.28 * RATE)
WINDOW = round(0.15 * RATE)
SOURCES = {
    'impact_land_light2.wav': '7ebab0cda1dc9d8928f825dc89bf39f2d1066a860b3cf2e1b3652a9c8b4bdbb5',
    'impact_medium2.wav': '979d1ab9478bbe72e19f5821f8d840c891ac9568c17f1025fbb2af3daf334bca',
}


def rms(x):
    return np.sqrt(np.mean(x[:WINDOW] ** 2))


def shape(source, name, low, high, decay):
    path = source / name
    assert hashlib.sha256(path.read_bytes()).hexdigest() == SOURCES[name]
    x, rate = sf.read(path, dtype='float64')
    assert x.ndim == 1
    onset = np.flatnonzero(abs(x) > max(abs(x)) * 0.03)[0]
    x = x[max(0, onset - round(rate * 0.002)):]
    ratio = Fraction(RATE, rate)
    x = resample_poly(x, ratio.numerator, ratio.denominator)
    x = sosfilt(butter(2, [low, high], btype='bandpass', fs=RATE, output='sos'), x)
    x = np.pad(x[:LENGTH], (0, max(0, LENGTH - len(x))))
    x *= np.exp(-np.arange(LENGTH) / (RATE * decay))
    attack, tail = round(RATE * 0.006), round(RATE * 0.04)
    x[:attack] *= np.sin(np.linspace(0, np.pi / 2, attack)) ** 2
    x[-tail:] *= np.cos(np.linspace(0, np.pi / 2, tail)) ** 2
    x[-round(RATE * 0.015):] = 0
    return x


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True, help='Read-only supplied Foley directory')
    args = parser.parse_args()
    x = shape(args.source, 'impact_land_light2.wav', 75, 2000, 0.075)
    body = shape(args.source, 'impact_medium2.wav', 90, 650, 0.055)
    x += body * rms(x) / rms(body) * 10 ** (-8 / 20)
    x *= 10 ** (-34 / 20) / rms(x)
    # Preserve the audition's PCM encoding, then the two accepted gain edits.
    audition = io.BytesIO()
    sf.write(audition, x, RATE, format='WAV', subtype='PCM_16')
    assert hashlib.sha256(audition.getvalue()).hexdigest() == '5469b0bf0e4e5b91f1452682ec878c29d0e53ae3f36b71dc8eec1b9128faf096'
    with wave.open(io.BytesIO(audition.getvalue()), 'rb') as f:
        samples = np.frombuffer(f.readframes(f.getnframes()), dtype='<i2').astype(np.float64)
    for db in [3.0, 1.5]:
        samples = np.rint(samples * 10 ** (db / 20))
    assert np.isfinite(samples).all() and max(abs(samples)) < 32767
    out = ROOT / 'land_01.wav'
    with wave.open(str(out), 'wb') as f:
        f.setparams((1, 2, RATE, 0, 'NONE', 'not compressed'))
        f.writeframes(samples.astype('<i2').tobytes())
    digest = hashlib.sha256(out.read_bytes()).hexdigest()
    assert digest == '9521f48b190f53d8b698d5faa1b242888ef601051decbc1ee890254d39650f62'
    report = {
        'selection': 'B-round-with-body',
        'selected_by': 'Maxwell chose B, requested +3 dB then +1.5 dB, and requested merge.',
        'distribution': 'Inclusion authorized by Maxwell request to merge this selected sound; no broader source license or CC0 assertion.',
        'sources_sha256': SOURCES,
        'processing': {'sample_rate': RATE, 'onset_threshold_fraction_of_peak': 0.03,
                       'pre_onset_seconds': 0.002, 'landing_bandpass_hz': [75, 2000],
                       'landing_decay_seconds': 0.075, 'body_bandpass_hz': [90, 650],
                       'body_decay_seconds': 0.055, 'body_relative_rms_db': -8,
                       'attack_seconds': 0.006, 'tail_fade_seconds': 0.04,
                       'last_silent_seconds': 0.015, 'audition_first150ms_rms_dbfs': -34,
                       'accepted_gain_edits_db': [3.0, 1.5], 'total_gain_db_over_B': 4.5,
                       'runtime_profile_volume': 1.0, 'runtime_pitch': 1.0},
        'output': {'file': out.name, 'sha256': digest, 'sample_rate': RATE,
                   'channels': 1, 'subtype': 'PCM_16', 'seconds': LENGTH / RATE,
                   'peak_dbfs': 20 * math.log10(max(abs(samples)) / 32768)},
        'limits': 'One fixed landing level/variation. No surface or hard-fall scaling; new ReImagined feedback.'
    }
    (ROOT / 'landing-recipe.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['output'], indent=2))


if __name__ == '__main__':
    main()
