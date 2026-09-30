"""Write the showcase Add-Ons' sounds: short synthesized effects, original
and generated here, so they carry no one else's rights.

Outputs 16-bit mono WAV at 22050 Hz:
  packages/showcase/gravity-gun-fx/client/sounds/
    grab.wav    the beam catching something: a rising hum with a zing
    drop.wav    letting go: the hum falling away
  packages/showcase/steel-ball-fx/client/sounds/
    clank.wav   steel striking something: a bell-like ring of inharmonic partials
    thud.wav    the ball's weight landing: a low knock

Run it again after changing the recipes below; the Add-Ons' tests check the
files are there, and the game decodes them when the Add-On starts. Only the
Python standard library is used, with a fixed seed for each Add-On, so the
output is the same every run.
"""
import math
import random
import struct
import wave
from pathlib import Path

RATE = 22050
ROOT = Path(__file__).resolve().parent.parent / 'packages' / 'showcase'
SEED = 20260928
rng = random.Random(SEED)


def lowpass(samples, cutoff):
    a = 1.0 - math.exp(-2 * math.pi * cutoff / RATE)
    out, y = [], 0.0
    for x in samples:
        y += a * (x - y)
        out.append(y)
    return out


def noise(seconds):
    return [rng.uniform(-1, 1) for _ in range(int(seconds * RATE))]


def sweep(seconds, f0, f1, shape=lambda p: math.sin(p)):
    out, phase = [], 0.0
    n = int(seconds * RATE)
    for i in range(n):
        f = f0 * (f1 / f0) ** (i / n)
        phase += 2 * math.pi * f / RATE
        out.append(shape(phase))
    return out


def envelope(samples, attack, decay):
    n = len(samples)
    out = []
    for i, x in enumerate(samples):
        t = i / RATE
        a = min(1.0, t / attack) if attack > 0 else 1.0
        out.append(x * a * math.exp(-t / decay))
    return out


def mix(*tracks):
    n = max(len(t) for t, _ in tracks)
    out = [0.0] * n
    for track, gain in tracks:
        for i, x in enumerate(track):
            out[i] += x * gain
    return out


def fade_out(samples, seconds=0.03):
    n = int(seconds * RATE)
    for i in range(min(n, len(samples))):
        samples[-1 - i] *= i / n
    return samples


def write(path, samples, peak=0.85):
    top = max(1e-6, max(abs(x) for x in samples))
    scale = peak / top
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), 'wb') as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(b''.join(struct.pack('<h', int(max(-1, min(1, x * scale)) * 32767)) for x in samples))


def gravity_gun():
    rng.seed(SEED)
    out = ROOT / 'gravity-gun-fx' / 'client' / 'sounds'
    hum = envelope(sweep(0.45, 90, 190), 0.01, 0.16)
    zing = envelope(sweep(0.45, 1300, 2100), 0.005, 0.06)
    crackle = envelope(lowpass(noise(0.45), 3000), 0.002, 0.05)
    write(out / 'grab.wav', fade_out(mix((hum, 1.0), (zing, 0.25), (crackle, 0.35))))

    fall = envelope(sweep(0.35, 230, 75), 0.005, 0.12)
    hiss = envelope(lowpass(noise(0.35), 1500), 0.005, 0.08)
    write(out / 'drop.wav', fade_out(mix((fall, 1.0), (hiss, 0.3))), peak=0.6)


def steel_ball():
    rng.seed(SEED + 1)
    out = ROOT / 'steel-ball-fx' / 'client' / 'sounds'
    f0 = 330.0
    partials = [(1.0, 1.0, 0.55), (2.76, 0.6, 0.35), (5.40, 0.35, 0.2), (8.93, 0.2, 0.12), (13.34, 0.1, 0.07)]
    n = int(1.2 * RATE)
    ring = [0.0] * n
    for ratio, gain, decay in partials:
        f = f0 * ratio
        for i in range(n):
            t = i / RATE
            ring[i] += gain * math.sin(2 * math.pi * f * t) * math.exp(-t / decay)
    click = envelope(lowpass(noise(1.2), 6000), 0.0005, 0.006)
    knock = envelope(sweep(1.2, 120, 70), 0.001, 0.05)
    write(out / 'clank.wav', fade_out(mix((ring, 0.6), (click, 0.6), (knock, 0.7))), peak=0.8)

    body = envelope(sweep(0.5, 80, 45), 0.002, 0.09)
    grit = envelope(lowpass(noise(0.5), 700), 0.002, 0.05)
    write(out / 'thud.wav', fade_out(mix((body, 1.0), (grit, 0.5))), peak=0.85)


if __name__ == '__main__':
    gravity_gun()
    steel_ball()
    for path in sorted(ROOT.glob('*/client/sounds/*.wav')):
        print(path.relative_to(ROOT), path.stat().st_size)
