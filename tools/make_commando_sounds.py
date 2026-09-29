"""Write the Commando sample's sounds: short synthesized effects, original
and generated here, so they carry no one else's rights.

Outputs 16-bit mono WAV at 22050 Hz in
packages/samples/sample-commando-rifle/assets/sounds/:
  shot.wav     the rifle firing: a crack, a low boom and a noisy tail
  reload.wav   a magazine out, a magazine in and the bolt: three clicks
  equip.wav    raising the rifle: a cloth rustle and one click
  empty.wav    the trigger on an empty clip: a dry tick

Run it again after changing the recipes below. Only the Python standard
library is used, with a fixed seed, so the output is the same every run.
"""
import math
import random
import struct
import wave
from pathlib import Path

RATE = 22050
OUT = (
    Path(__file__).resolve().parent.parent
    / 'packages' / 'samples' / 'sample-commando-rifle' / 'assets' / 'sounds'
)
rng = random.Random(20260929)


def silence(seconds):
    return [0.0] * int(seconds * RATE)


def noise(seconds):
    return [rng.uniform(-1, 1) for _ in range(int(seconds * RATE))]


def lowpass(samples, cutoff):
    a = 1.0 - math.exp(-2 * math.pi * cutoff / RATE)
    out, y = [], 0.0
    for x in samples:
        y += a * (x - y)
        out.append(y)
    return out


def highpass(samples, cutoff):
    low = lowpass(samples, cutoff)
    return [x - y for x, y in zip(samples, low)]


def decay(samples, seconds):
    return [x * math.exp(-i / RATE / seconds) for i, x in enumerate(samples)]


def tone(seconds, f0, f1):
    out, phase = [], 0.0
    n = int(seconds * RATE)
    for i in range(n):
        phase += 2 * math.pi * (f0 + (f1 - f0) * i / n) / RATE
        out.append(math.sin(phase))
    return out


def mix(*layers):
    n = max(len(layer) for layer, _ in layers)
    out = [0.0] * n
    for layer, gain in layers:
        for i, x in enumerate(layer):
            out[i] += x * gain
    return out


def then(*parts):
    out = []
    for part in parts:
        out.extend(part)
    return out


def click(pitch, seconds=0.03):
    body = highpass(noise(seconds), pitch)
    ring = tone(seconds, pitch * 1.5, pitch)
    return decay(mix((body, 0.8), (ring, 0.4)), seconds / 4)


def write(name, samples):
    peak = max(abs(x) for x in samples) or 1.0
    OUT.mkdir(parents=True, exist_ok=True)
    with wave.open(str(OUT / name), 'wb') as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(b''.join(
            struct.pack('<h', int(x / peak * 0.9 * 32767)) for x in samples
        ))


def main():
    crack = decay(highpass(noise(0.05), 2500), 0.006)
    boom = decay(tone(0.35, 110, 45), 0.09)
    tail = decay(lowpass(noise(0.6), 900), 0.16)
    write('shot.wav', mix((crack, 1.0), (boom, 0.8), (tail, 0.55)))
    write('reload.wav', then(
        click(1800), silence(0.35), click(1400), silence(0.4),
        click(2400, 0.05), silence(0.08), click(1600, 0.05),
    ))
    rustle = decay(lowpass(noise(0.25), 1500), 0.08)
    write('equip.wav', then(mix((rustle, 0.5)), click(2000)))
    write('empty.wav', click(3000, 0.02))


if __name__ == '__main__':
    main()
