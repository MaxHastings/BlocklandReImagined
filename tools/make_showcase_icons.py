"""Write the showcase Add-Ons' item icons: original art made here, so it
carries no one else's rights (it is not a render of any game model).

Outputs 128x128 RGBA PNG:
  packages/showcase/gravity-gun-tool/assets/icons/gravity_gun.png
    the Gravity Gun drawn like the game's other item icons: a small solid
    model, lit and shaded, seen from above and to one side, on a clear
    background (no outline or glow), pointing up and to the right as the
    Hammer and Wrench do. Its looks are the gun's in play: a dark shell,
    teal veins, green-lit edges and a teal muzzle.

The model is a few rounded boxes, ray marched with a key light, a fill and
a highlight. Run it again after changing the model below; the output is the
same every run. Only the Python standard library is used.
"""
import math
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / 'packages' / 'showcase'
SIZE = 128
SAMPLES = 3  # per side, per pixel


def norm(v):
    n = math.sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]) or 1.0
    return (v[0] / n, v[1] / n, v[2] / n)


def rot_z(p, a):
    c, s = math.cos(a), math.sin(a)
    return (p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2])


def rbox(p, c, h, r):
    """Signed distance to a box centred on `c`, half size `h`, edges rounded by `r`."""
    q = [abs(p[i] - c[i]) - h[i] + r for i in range(3)]
    outside = math.sqrt(sum(max(v, 0.0) ** 2 for v in q))
    return outside + min(max(q[0], q[1], q[2]), 0.0) - r


def smin(a, b, k):
    h = max(k - abs(a - b), 0.0) / k
    return min(a, b) - h * h * k * 0.25


GRIP = math.radians(-16)


def parts(p):
    """Each part's distance: shell (body, emitter, grip) and the muzzle lens."""
    body = rbox(p, (-0.05, 0.22, 0.0), (0.82, 0.27, 0.22), 0.13)
    emitter = rbox(p, (0.93, 0.25, 0.0), (0.2, 0.4, 0.3), 0.09)
    g = rot_z((p[0] + 0.42, p[1], p[2]), -GRIP)
    grip = rbox(g, (0.0, -0.38, 0.0), (0.16, 0.44, 0.17), 0.08)
    shell = smin(smin(body, emitter, 0.08), grip, 0.14)
    lens = rbox(p, (1.16, 0.25, 0.0), (0.05, 0.24, 0.2), 0.05)
    return shell, lens


def scene(p):
    shell, lens = parts(p)
    return min(shell, lens)


def normal(p):
    e = 1e-3
    return norm(tuple(
        scene(tuple(p[j] + (e if j == i else 0.0) for j in range(3)))
        - scene(tuple(p[j] - (e if j == i else 0.0) for j in range(3)))
        for i in range(3)))


def veins(p):
    """Distance to the nearest of the wandering veins on the shell's sides."""
    x, y = p[0], p[1]
    best = 9.0
    for f, phase, off in ((7.0, 0.0, 0.34), (6.0, 1.9, 0.12)):
        wave = off + 0.05 * math.sin(x * f + phase) + 0.02 * math.sin(x * f * 2.7 + phase * 1.3)
        best = min(best, abs(y - wave))
    if y < 0.0 and x < 0.0:
        g = rot_z((x + 0.42, y, 0.0), -GRIP)
        best = min(best, abs(g[0] - 0.035 * math.sin(g[1] * 14.0)))
    return best


SHELL = (0.13, 0.14, 0.17)
RIM = (0.25, 0.78, 0.5)
VEIN = (0.2, 0.72, 0.76)
LENS = (0.5, 1.0, 0.92)

# The object turned to face up and right, then seen from above and in front.
YAW = math.radians(-40)
PITCH = math.radians(22)
ROLL = math.radians(28)
KEY = norm((-0.5, 0.8, 0.6))
FILL = norm((0.6, -0.2, 0.5))


def to_object(v):
    """Camera space to object space (the inverse of the view turn)."""
    x, y, z = v
    c, s = math.cos(-PITCH), math.sin(-PITCH)
    y, z = y * c - z * s, y * s + z * c
    c, s = math.cos(-YAW), math.sin(-YAW)
    x, z = x * c + z * s, -x * s + z * c
    return rot_z((x, y, z), -ROLL)


def to_camera(v):
    x, y, z = rot_z(v, ROLL)
    c, s = math.cos(YAW), math.sin(YAW)
    x, z = x * c + z * s, -x * s + z * c
    c, s = math.cos(PITCH), math.sin(PITCH)
    y, z = y * c - z * s, y * s + z * c
    return (x, y, z)


def mix(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def shade(px, py):
    """Straight RGBA for the ray through one sample point."""
    # Orthographic, as the game's icons read: the model spans the frame.
    scale = 2.75 / SIZE
    origin = to_object(((px - SIZE / 2) * scale + 0.02, (SIZE / 2 - py) * scale + 0.06, 4.0))
    ray = to_object((0.0, 0.0, -1.0))
    t = 0.0
    for _ in range(96):
        p = tuple(origin[i] + ray[i] * t for i in range(3))
        d = scene(p)
        if d < 1e-3:
            break
        t += d
        if t > 8.0:
            return (0.0, 0.0, 0.0, 0.0)
    else:
        return (0.0, 0.0, 0.0, 0.0)
    n = normal(p)
    shell, lens = parts(p)
    if lens < shell:
        base, gloss = LENS, 0.9
    else:
        # Green-lit where the shell rounds over an edge, veins on its faces.
        edge = 1.0 - max(abs(n[0]), abs(n[1]), abs(n[2]))
        base, gloss = SHELL, 0.45
        v = veins(p)
        if v < 0.028:
            base = mix(base, VEIN, (1.0 - v / 0.028) ** 1.2)
        if edge > 0.16:
            base = mix(base, RIM, min(1.0, (edge - 0.16) / 0.1))
    nc = to_camera(n)
    key = max(0.0, sum(nc[i] * KEY[i] for i in range(3)))
    fill = max(0.0, sum(nc[i] * FILL[i] for i in range(3)))
    half = norm((KEY[0], KEY[1], KEY[2] + 1.0))
    spec = max(0.0, sum(nc[i] * half[i] for i in range(3))) ** 24 * gloss
    light = 0.45 + 0.95 * key + 0.3 * fill
    colour = tuple(base[i] * light + spec * 0.7 for i in range(3))
    if lens < shell:
        colour = mix(colour, LENS, 0.55)
    return (*colour, 1.0)


def render():
    rows = []
    step = 1.0 / SAMPLES
    for py in range(SIZE):
        row = bytearray([0])
        for px in range(SIZE):
            r = g = b = a = 0.0
            for sy in range(SAMPLES):
                for sx in range(SAMPLES):
                    cr, cg, cb, ca = shade(px + (sx + 0.5) * step, py + (sy + 0.5) * step)
                    r += cr * ca
                    g += cg * ca
                    b += cb * ca
                    a += ca
            n = SAMPLES * SAMPLES
            if a > 0:
                r, g, b = r / a, g / a, b / a
            row += bytes(int(max(0, min(1, c)) * 255 + 0.5) for c in (r, g, b, a / n))
        rows.append(bytes(row))
    return b''.join(rows)


def png(pixels, width=SIZE, height=SIZE):
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    header = struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0)
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', header) + chunk(b'IDAT', zlib.compress(pixels, 9)) + chunk(b'IEND', b'')


if __name__ == '__main__':
    out = ROOT / 'gravity-gun-tool' / 'assets' / 'icons' / 'gravity_gun.png'
    out.write_bytes(png(render()))
    print(out.relative_to(ROOT), out.stat().st_size)
