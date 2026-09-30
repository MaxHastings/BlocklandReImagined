"""Write the Trench Warfare Add-Ons' sounds and the Trench Pick's fallback
icon: original, made here, so they carry no one else's rights.

Outputs, under packages/trench-warfare/trench-kit/assets/:
  sounds/dig.wav      the pick biting into soil: a gritty scrape over a
                      dull knock (16-bit mono, 22050 Hz)
  sounds/place.wav    a cube of dirt patted down: a soft low thump
  sounds/whistle.wav  the officer's whistle that ends the ceasefire: a
                      trilling pea whistle, two short blasts and a long one
  icons/trench_pick.png  a 128x128 pick drawn in the stock icons' manner
                      (a small shaded model on a clear background, pointing
                      up and to the right), shown only when the game cannot
                      draw the icon from the tool's own model
                      (trench_pick.render.json)

Run it again after changing a recipe below; the output is the same every
run (fixed seed, Python standard library only).
"""
import math
import random
import struct
import wave
import zlib
from pathlib import Path

RATE = 22050
ROOT = Path(__file__).resolve().parent.parent / 'packages' / 'trench-warfare' / 'trench-kit' / 'assets'
rng = random.Random(19160701)


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
    return [x - l for x, l in zip(samples, low)]


def sweep(seconds, f0, f1):
    out, phase = [], 0.0
    n = int(seconds * RATE)
    for i in range(n):
        f = f0 * (f1 / f0) ** (i / n)
        phase += 2 * math.pi * f / RATE
        out.append(math.sin(phase))
    return out


def envelope(samples, attack, decay):
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


def dig():
    # Grit: noise in the soil's band, grains made by gating it unevenly.
    grit = highpass(lowpass(noise(0.32), 3200), 400)
    gate = lowpass([1.0 if rng.random() < 0.35 else 0.15 for _ in grit], 900)
    scrape = envelope([g * x for g, x in zip(gate, grit)], 0.004, 0.09)
    knock = envelope(sweep(0.32, 150, 70), 0.002, 0.045)
    crumble = envelope(lowpass(noise(0.32), 500), 0.02, 0.12)
    return fade_out(mix((scrape, 1.0), (knock, 0.8), (crumble, 0.5)))


def place():
    body = envelope(sweep(0.3, 95, 50), 0.003, 0.07)
    pat = envelope(lowpass(noise(0.3), 900), 0.002, 0.035)
    settle = envelope(highpass(lowpass(noise(0.3), 2500), 600), 0.03, 0.06)
    return fade_out(mix((body, 1.0), (pat, 0.6), (settle, 0.15)))


def whistle():
    out = []
    # Two short blasts and a long one, as an officer's whistle.
    for length, gap in ((0.16, 0.08), (0.16, 0.1), (0.75, 0.0)):
        n = int(length * RATE)
        phase = 0.0
        breath = highpass(lowpass(noise(length), 6000), 1500)
        for i in range(n):
            t = i / RATE
            # The pea rattles: the pitch and loudness warble about 28 times a second.
            trill = math.sin(2 * math.pi * 28 * t)
            f = 2850 + 110 * trill
            phase += 2 * math.pi * f / RATE
            tone = math.sin(phase) + 0.25 * math.sin(2 * phase)
            level = (0.7 + 0.3 * trill) * min(1.0, t / 0.01) * min(1.0, (length - t) / 0.03)
            out.append((tone * 0.8 + breath[i] * 0.35) * level)
        out.extend([0.0] * int(gap * RATE))
    return out


# ---- The icon ----

SIZE = 128
SAMPLES = 3


def seg(px, py, ax, ay, bx, by):
    """Distance from (px, py) to segment a-b, and how far along it (0 to 1)."""
    dx, dy = bx - ax, by - ay
    t = max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)))
    qx, qy = ax + dx * t - px, ay + dy * t - py
    return math.hypot(qx, qy), t


HANDLE = ((0.22, 0.86), (0.64, 0.32))
_dx, _dy = HANDLE[1][0] - HANDLE[0][0], HANDLE[1][1] - HANDLE[0][1]
_n = math.hypot(_dx, _dy)
ALONG = (_dx / _n, _dy / _n)
ACROSS = (-ALONG[1], ALONG[0])
HEAD = [(HANDLE[1][0] + ACROSS[0] * s * 0.34 - ALONG[0] * 0.13 * s * s,
         HANDLE[1][1] + ACROSS[1] * s * 0.34 - ALONG[1] * 0.13 * s * s, s)
        for s in (i / 40 - 1 for i in range(81))]


def head_near(u, v):
    return abs(u - HANDLE[1][0]) < 0.4 and abs(v - HANDLE[1][1]) < 0.4


def head(u, v):
    """Distance to the head's spine, where along it (-1 to 1), and its half width there."""
    best = (9.0, 0.0)
    for x, y, s in HEAD:
        d = math.hypot(u - x, v - y)
        if d < best[0]:
            best = (d, s)
    d, s = best
    return d, s, 0.055 * (1 - abs(s) ** 1.6) + 0.007


def shade(base, light):
    return tuple(min(255, int(c * light)) for c in base)


def icon():
    wood = (150, 104, 60)
    steel = (150, 156, 164)
    pixels = bytearray()
    for y in range(SIZE):
        pixels.append(0)
        for x in range(SIZE):
            acc = [0.0, 0.0, 0.0, 0.0]
            for sy in range(SAMPLES):
                for sx in range(SAMPLES):
                    u = (x + (sx + 0.5) / SAMPLES) / SIZE
                    v = (y + (sy + 0.5) / SAMPLES) / SIZE
                    color = None
                    # The head: a blade across the top of the handle,
                    # both points curving back toward the hand.
                    if head_near(u, v):
                        d, s_at, width = head(u, v)
                        if d < width:
                            color = shade(steel, 1.2 - 0.4 * (d / width) - 0.25 * (s_at + 1) / 2)
                    # The handle: from the lower left up into the head.
                    d, t = seg(u, v, HANDLE[0][0], HANDLE[0][1], HANDLE[1][0], HANDLE[1][1])
                    if color is None and d < 0.045:
                        across = d / 0.045
                        color = shade(wood, 1.12 - 0.45 * across * across - 0.2 * t)
                    if color is not None:
                        acc[0] += color[0]
                        acc[1] += color[1]
                        acc[2] += color[2]
                        acc[3] += 255
            n = SAMPLES * SAMPLES
            a = acc[3] / n
            if a > 0:
                pixels += bytes([int(acc[0] / n * 255 / a), int(acc[1] / n * 255 / a), int(acc[2] / n * 255 / a), int(a)])
            else:
                pixels += bytes(4)
    return bytes(pixels)


def png(pixels, width=SIZE, height=SIZE):
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    header = struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0)
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', header) + chunk(b'IDAT', zlib.compress(pixels, 9)) + chunk(b'IEND', b'')


if __name__ == '__main__':
    write(ROOT / 'sounds' / 'dig.wav', dig(), peak=0.8)
    write(ROOT / 'sounds' / 'place.wav', place(), peak=0.75)
    write(ROOT / 'sounds' / 'whistle.wav', fade_out(whistle(), 0.02), peak=0.7)
    (ROOT / 'icons').mkdir(parents=True, exist_ok=True)
    (ROOT / 'icons' / 'trench_pick.png').write_bytes(png(icon()))
    for path in sorted(ROOT.rglob('*')):
        if path.suffix in ('.wav', '.png'):
            print(path.relative_to(ROOT), path.stat().st_size)
