"""Write the showcase Add-Ons' sounds: short synthesized effects, original
and generated here, so they carry no one else's rights.

Outputs 16-bit mono WAV at 22050 Hz:
  packages/showcase/gravity-gun-fx/client/sounds/
    grab.wav    the beam catching something: a rising hum with a zing
    drop.wav    letting go: the hum falling away
    reach.wav   the beam reaching with nothing caught: a searching
                whirr, repeated while the trigger is held
  packages/showcase/steel-ball-fx/client/sounds/
    clank.wav   steel striking something: a bell-like ring of inharmonic partials
    thud.wav    the ball's weight landing: a low knock
  packages/showcase/grapple-rope-fx/client/sounds/
    throw.wav   the hook thrown: a whoosh with the rope rattling out
    bite.wav    the hook biting: a brass clank over a woody knock
    twang.wav   the rope snapping tight: a thick plucked string and a creak
    zip.wav     letting go: the rope zipping back in, a run of ratchet ticks

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

    # Even from start to end, so repeats run together as one whirr.
    whirr = [math.sin(2 * math.pi * (140 * t + 6 * math.sin(2 * math.pi * 4 * t))) for t in
             (i / RATE for i in range(int(0.5 * RATE)))]
    shimmer = [math.sin(2 * math.pi * 1650 * t) * (0.5 + 0.5 * math.sin(2 * math.pi * 8 * t)) for t in
               (i / RATE for i in range(int(0.5 * RATE)))]
    fizz = lowpass(noise(0.5), 2400)
    reach = mix((whirr, 1.0), (shimmer, 0.12), (fizz, 0.25))
    edge = int(0.015 * RATE)
    for i in range(edge):
        reach[i] *= i / edge
    write(out / 'reach.wav', fade_out(reach, 0.015), peak=0.5)


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


def pluck(seconds, f, damping):
    """A plucked string (Karplus-Strong): a burst of noise fed back
    through a delay one period long, averaged a little each pass."""
    period = max(2, int(RATE / f))
    line = [rng.uniform(-1, 1) for _ in range(period)]
    out = []
    for i in range(int(seconds * RATE)):
        x = line[i % period]
        nxt = line[(i + 1) % period]
        line[i % period] = damping * 0.5 * (x + nxt)
        out.append(x)
    return out


def grapple_rope():
    rng.seed(SEED + 2)
    out = ROOT / 'grapple-rope-fx' / 'client' / 'sounds'
    # A whoosh: noise swelling and fading, its brightness rising as the
    # hook leaves, with the rope rattling out behind it.
    n = int(0.4 * RATE)
    air = noise(0.4)
    swell = [math.sin(math.pi * i / n) ** 2 for i in range(n)]
    low = lowpass(air, 700)
    high = lowpass(air, 2600)
    whoosh = [(low[i] * (1 - i / n) + high[i] * (i / n)) * swell[i] for i in range(n)]
    rattle = [0.0] * n
    for k in range(14):
        at = int((0.05 + k * 0.022 + rng.uniform(0, 0.008)) * RATE)
        tick = envelope(lowpass(noise(0.02), 3500), 0.0005, 0.004)
        for i, x in enumerate(tick):
            if at + i < n:
                rattle[at + i] += x * (1 - k / 16)
    write(out / 'throw.wav', fade_out(mix((whoosh, 1.0), (rattle, 0.5))), peak=0.7)

    # The bite: a short brass ring on a woody knock.
    m = int(0.5 * RATE)
    ring = [0.0] * m
    for ratio, gain, decay in [(1.0, 1.0, 0.12), (2.32, 0.5, 0.08), (4.25, 0.3, 0.05), (6.8, 0.15, 0.03)]:
        f = 620.0 * ratio
        for i in range(m):
            t = i / RATE
            ring[i] += gain * math.sin(2 * math.pi * f * t) * math.exp(-t / decay)
    knock = envelope(sweep(0.5, 200, 110), 0.001, 0.035)
    crack = envelope(lowpass(noise(0.5), 5000), 0.0003, 0.008)
    write(out / 'bite.wav', fade_out(mix((ring, 0.45), (knock, 0.9), (crack, 0.5))), peak=0.8)

    # The twang: a thick rope plucked, dropping a little in pitch as it
    # settles, and the fibres creaking.
    string = lowpass(pluck(0.6, 98.0, 0.996), 1800)
    body = envelope(sweep(0.6, 140, 90), 0.002, 0.08)
    creak = envelope([math.sin(2 * math.pi * 55 * i / RATE) * rng.uniform(0.3, 1.0)
                      for i in range(int(0.6 * RATE))], 0.01, 0.1)
    write(out / 'twang.wav', fade_out(mix((envelope(string, 0.001, 0.25), 1.0), (body, 0.5), (creak, 0.2))),
          peak=0.75)

    # The zip: a hiss rising in pitch over quickening ratchet ticks.
    z = int(0.25 * RATE)
    hiss = [x * (i / z) for i, x in enumerate(lowpass(noise(0.25), 3000))]
    ticks = [0.0] * z
    at, gap = 0.0, 0.03
    while at < 0.23 and gap > 0.006:
        tick = envelope(lowpass(noise(0.015), 5000), 0.0003, 0.003)
        start = int(at * RATE)
        for i, x in enumerate(tick):
            if start + i < z:
                ticks[start + i] += x
        at += gap
        gap *= 0.88
    write(out / 'zip.wav', fade_out(mix((hiss, 0.5), (ticks, 1.0))), peak=0.6)


def grappling_hook():
    rng.seed(SEED + 3)
    out = ROOT / 'grappling-hook-fx' / 'client' / 'sounds'
    # The shot: a sharp pneumatic crack, then the cable hissing out of the
    # drum.
    n = int(0.45 * RATE)
    crack = envelope(lowpass(noise(0.45), 6000), 0.0002, 0.012)
    thump = envelope(sweep(0.45, 160, 60), 0.0005, 0.03)
    spool = [x * math.exp(-i / (0.18 * RATE)) for i, x in enumerate(lowpass(noise(0.45), 2200))]
    write(out / 'fire.wav', fade_out(mix((crack, 0.9), (thump, 1.0), (spool, 0.35))), peak=0.8)

    # The bite: forged steel clanging into place, a bright ring over a
    # hard knock.
    m = int(0.6 * RATE)
    ring = [0.0] * m
    for ratio, gain, decay in [(1.0, 1.0, 0.18), (2.76, 0.6, 0.1), (5.4, 0.35, 0.06), (8.9, 0.2, 0.03)]:
        f = 880.0 * ratio
        for i in range(m):
            t = i / RATE
            ring[i] += gain * math.sin(2 * math.pi * f * t) * math.exp(-t / decay)
    knock = envelope(sweep(0.6, 320, 140), 0.0005, 0.025)
    write(out / 'clamp.wav', fade_out(mix((ring, 0.5), (knock, 0.9))), peak=0.8)

    # The winch: an electric motor spinning up and pulling, with the pawl
    # clicking over the ratchet.
    w = int(0.8 * RATE)
    motor = [0.0] * w
    phase = 0.0
    for i in range(w):
        t = i / RATE
        f = 90 + 160 * min(1.0, t / 0.15)
        phase += 2 * math.pi * f / RATE
        motor[i] = (math.sin(phase) + 0.5 * math.sin(2 * phase) + 0.25 * math.sin(3 * phase))
        motor[i] *= min(1.0, t / 0.03) * (1.0 - max(0.0, (t - 0.55) / 0.25))
    clicks = [0.0] * w
    at = 0.04
    while at < 0.7:
        tick = envelope(lowpass(noise(0.012), 4500), 0.0003, 0.002)
        start = int(at * RATE)
        for i, x in enumerate(tick):
            if start + i < w:
                clicks[start + i] += x
        at += 0.028
    write(out / 'winch.wav', fade_out(mix((lowpass(motor, 1400), 0.8), (clicks, 0.4))), peak=0.6)

    # Letting go: the pawl freed, the drum spinning the cable back in, a
    # clack as the grapnel seats in the muzzle.
    r = int(0.3 * RATE)
    whirr = [0.0] * r
    phase = 0.0
    for i in range(r):
        t = i / RATE
        phase += 2 * math.pi * (400 + 900 * t / 0.3) / RATE
        whirr[i] = math.sin(phase) * (1 - t / 0.3) * 0.6
    hiss = lowpass(noise(0.3), 3500)
    clack = [0.0] * r
    seat = envelope(sweep(0.05, 900, 500), 0.0003, 0.01)
    for i, x in enumerate(seat):
        at = int(0.24 * RATE) + i
        if at < r:
            clack[at] += x
    write(out / 'release.wav', fade_out(mix((whirr, 0.5), (hiss, 0.25), (clack, 1.0))), peak=0.6)


def chain_rattle(seconds, start, every, jitter, pitch):
    """Chain links chinking past a lip, `every` seconds apart from `start`."""
    n = int(seconds * RATE)
    out = [0.0] * n
    at = start
    k = 0
    while at < seconds - 0.02:
        f = pitch * (1.0 + 0.25 * rng.random())
        m = int(0.03 * RATE)
        start_i = int(at * RATE)
        for i in range(m):
            t = i / RATE
            v = math.sin(2 * math.pi * f * t) + 0.5 * math.sin(2 * math.pi * f * 2.7 * t)
            v *= math.exp(-t / 0.006)
            if start_i + i < n:
                out[start_i + i] += v * (0.6 + 0.4 * rng.random())
        at += every * (1.0 + jitter * (rng.random() - 0.5))
        k += 1
    return out


def hookshot():
    rng.seed(SEED + 4)
    out = ROOT / 'hookshot-fx' / 'client' / 'sounds'
    # The shot: a spring let go with a twang, and the chain rattling out
    # of the barrel after the spearhead.
    n = 0.5
    w = int(n * RATE)
    twang = [0.0] * w
    for ratio, gain, decay in [(1.0, 1.0, 0.09), (2.1, 0.5, 0.05), (3.3, 0.3, 0.03)]:
        for i in range(w):
            t = i / RATE
            f = 210.0 * ratio * (1.0 + 0.6 * math.exp(-t / 0.02))
            twang[i] += gain * math.sin(2 * math.pi * f * t) * math.exp(-t / decay)
    thunk = envelope(sweep(n, 180, 70), 0.0004, 0.02)
    rattle = chain_rattle(n, 0.02, 0.012, 0.6, 2600)
    rattle = [x * math.exp(-i / (0.16 * RATE)) for i, x in enumerate(rattle)]
    write(out / 'shoot.wav', fade_out(mix((twang, 0.6), (thunk, 0.8), (rattle, 0.35))), peak=0.8)

    # The bite: the spearhead's point chunking in, a short bright chink.
    m = 0.4
    c = int(m * RATE)
    ring = [0.0] * c
    for ratio, gain, decay in [(1.0, 1.0, 0.07), (2.4, 0.5, 0.04), (4.1, 0.3, 0.02)]:
        f = 1500.0 * ratio
        for i in range(c):
            t = i / RATE
            ring[i] += gain * math.sin(2 * math.pi * f * t) * math.exp(-t / decay)
    chunk = envelope(lowpass(noise(m), 1800), 0.0003, 0.015)
    write(out / 'chink.wav', fade_out(mix((ring, 0.45), (chunk, 1.0))), peak=0.8)

    # The haul: the chain reeling in hard, its links racing over the
    # barrel's lip faster and faster.
    r = 0.7
    links = [0.0] * int(r * RATE)
    at = 0.0
    every = 0.03
    while at < r - 0.03:
        piece = chain_rattle(0.04, 0.0, 1.0, 0.0, 2200)
        start = int(at * RATE)
        for i, x in enumerate(piece):
            if start + i < len(links):
                links[start + i] += x * min(1.0, (r - at) / 0.15)
        at += every
        every = max(0.008, every * 0.9)
    drag = [x * min(1.0, i / (0.05 * RATE)) * max(0.0, 1.0 - i / (r * RATE))
            for i, x in enumerate(lowpass(noise(r), 900))]
    write(out / 'reel.wav', fade_out(mix((links, 0.5), (drag, 0.5))), peak=0.65)

    # Letting go: the chain whips back into the barrel and the spearhead
    # seats with a clack.
    b = 0.3
    back = chain_rattle(b, 0.0, 0.007, 0.5, 2400)
    back = [x * (1.0 - i / (b * RATE)) for i, x in enumerate(back)]
    clack = [0.0] * int(b * RATE)
    seat = envelope(sweep(0.05, 800, 420), 0.0003, 0.01)
    for i, x in enumerate(seat):
        at = int(0.22 * RATE) + i
        if at < len(clack):
            clack[at] += x
    write(out / 'retract.wav', fade_out(mix((back, 0.5), (clack, 1.0))), peak=0.6)


if __name__ == '__main__':
    gravity_gun()
    steel_ball()
    grapple_rope()
    grappling_hook()
    hookshot()
    for path in sorted(ROOT.glob('*/client/sounds/*.wav')):
        print(path.relative_to(ROOT), path.stat().st_size)
