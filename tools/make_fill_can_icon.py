"""Write the Fill Can's item icon: original art made here, so it carries no
one else's rights (it is not a render of any game model).

Output, 128x128 RGBA PNG:
  packages/fill-can/fill-can-tool/assets/icons/fill_can.png
    a tipped paint tin pouring paint, drawn like the game's other item
    icons: a small solid model, lit and shaded, seen from above and to one
    side, on a clear background with no outline or glow. The pail pours to
    the right, where the Hammer and Wrench point.

The model is a few rounded shapes, ray marched with a key light, a fill and
a highlight, as `make_showcase_icons.py` draws the Gravity Gun. Run it again
after changing the model below; the output is the same every run. Only the
Python standard library is used.
"""
import math
import struct
import zlib
from pathlib import Path

OUT = (Path(__file__).resolve().parent.parent / 'packages' / 'fill-can' / 'fill-can-tool'
       / 'assets' / 'icons' / 'fill_can.png')
SIZE = 128
SAMPLES = 3  # per side, per pixel


def norm(v):
    n = math.sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]) or 1.0
    return (v[0] / n, v[1] / n, v[2] / n)


def rot_z(p, a):
    c, s = math.cos(a), math.sin(a)
    return (p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2])


def length(v):
    return math.sqrt(sum(c * c for c in v))


def smin(a, b, k):
    h = max(k - abs(a - b), 0.0) / k
    return min(a, b) - h * h * k * 0.25


def cylinder(p, radius, half, r):
    """Rounded cylinder along y, centred on the origin."""
    d = (math.hypot(p[0], p[2]) - radius + r, abs(p[1]) - half + r)
    return min(max(d[0], d[1]), 0.0) + length((max(d[0], 0.0), max(d[1], 0.0))) - r


def torus_y(p, major, minor):
    """Ring lying in the xz plane."""
    return math.hypot(math.hypot(p[0], p[2]) - major, p[1]) - minor


def capsule(p, a, b, r):
    pa = tuple(p[i] - a[i] for i in range(3))
    ba = tuple(b[i] - a[i] for i in range(3))
    h = max(0.0, min(1.0, sum(pa[i] * ba[i] for i in range(3)) / sum(c * c for c in ba)))
    return length(tuple(pa[i] - ba[i] * h for i in range(3))) - r


# The pail, upright in its own frame (opening up, y), tipped this far over
# to pour to the right.
TIP = math.radians(-62)
HALF = 0.5
RADIUS = 0.5
MOUTH = (0.0, HALF, 0.0)


def pail_frame(p):
    return rot_z((p[0] - PAIL_AT[0], p[1] - PAIL_AT[1], p[2]), -TIP)


PAIL_AT = (-0.35, 0.05, 0.0)


def parts(p):
    """Each part's distance: the pail's tin, its dark inside, the paint."""
    q = pail_frame(p)
    # A tapering tin: wider at the mouth, as paint pails are.
    taper = RADIUS - 0.07 * (HALF - q[1]) / (2 * HALF)
    outer = cylinder(q, taper, HALF, 0.05)
    hollow = cylinder((q[0], q[1] - 0.06, q[2]), taper - 0.06, HALF, 0.03)
    tin = max(outer, -hollow)
    rim = torus_y((q[0], q[1] - HALF + 0.02, q[2]), taper, 0.045)
    band = torus_y((q[0], q[1] + HALF * 0.35, q[2]), taper + 0.005, 0.03)
    tin = min(min(tin, rim), band)
    inside = max(hollow, q[1] - HALF + 0.12)
    # Paint: a pool in the mouth, a thick tongue over the lip, a fall and a splash.
    lip = rot_z((0.0, HALF, 0.0), TIP)
    lip = (lip[0] + PAIL_AT[0] + 0.3, lip[1] + PAIL_AT[1] - 0.06, 0.0)
    pool = cylinder((q[0], q[1] - HALF + 0.16, q[2]), taper - 0.07, 0.06, 0.05)
    tongue = capsule(p, (lip[0] - 0.12, lip[1] + 0.02, 0.0), (lip[0] + 0.18, lip[1] - 0.12, 0.0), 0.15)
    fall = capsule(p, (lip[0] + 0.18, lip[1] - 0.12, 0.0), (lip[0] + 0.34, -0.85, 0.0), 0.1)
    splash = length(((p[0] - lip[0] - 0.36) / 1.9, (p[1] + 0.95) / 0.55, p[2] / 1.4)) * 0.55 - 0.16
    drop = length((p[0] - lip[0] - 0.62, p[1] + 0.35, p[2] - 0.05)) - 0.07
    paint = smin(smin(smin(tongue, fall, 0.14), splash, 0.2), pool, 0.08)
    paint = min(paint, drop)
    return tin, inside, paint


def scene(p):
    return min(parts(p))


def normal(p):
    e = 1e-3
    return norm(tuple(
        scene(tuple(p[j] + (e if j == i else 0.0) for j in range(3)))
        - scene(tuple(p[j] - (e if j == i else 0.0) for j in range(3)))
        for i in range(3)))


TIN = (0.62, 0.65, 0.7)
INSIDE = (0.2, 0.21, 0.24)
PAINT = (0.16, 0.45, 1.0)

# The object seen from above and in front, turned a little to one side.
YAW = math.radians(-28)
PITCH = math.radians(24)
KEY = norm((-0.5, 0.8, 0.6))
FILL = norm((0.6, -0.2, 0.5))


def to_object(v):
    """Camera space to object space (the inverse of the view turn)."""
    x, y, z = v
    c, s = math.cos(-PITCH), math.sin(-PITCH)
    y, z = y * c - z * s, y * s + z * c
    c, s = math.cos(-YAW), math.sin(-YAW)
    return (x * c + z * s, y, -x * s + z * c)


def to_camera(v):
    x, y, z = v
    c, s = math.cos(YAW), math.sin(YAW)
    x, z = x * c + z * s, -x * s + z * c
    c, s = math.cos(PITCH), math.sin(PITCH)
    return (x, y * c - z * s, y * s + z * c)


def mix(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def shade(px, py):
    """Straight RGBA for the ray through one sample point."""
    # Orthographic, as the game's icons read: the model spans the frame.
    scale = 2.55 / SIZE
    origin = to_object(((px - SIZE / 2) * scale + 0.05, (SIZE / 2 - py) * scale - 0.12, 4.0))
    ray = to_object((0.0, 0.0, -1.0))
    t = 0.0
    for _ in range(128):
        p = tuple(origin[i] + ray[i] * t for i in range(3))
        d = scene(p)
        if d < 1e-3:
            break
        t += d * 0.9
        if t > 8.0:
            return (0.0, 0.0, 0.0, 0.0)
    else:
        return (0.0, 0.0, 0.0, 0.0)
    n = normal(p)
    tin, inside, paint = parts(p)
    nearest = min(tin, inside, paint)
    if nearest == paint:
        base, gloss, power = PAINT, 1.0, 40
    elif nearest == inside:
        base, gloss, power = INSIDE, 0.2, 12
    else:
        base, gloss, power = TIN, 0.7, 20
    nc = to_camera(n)
    key = max(0.0, sum(nc[i] * KEY[i] for i in range(3)))
    fill = max(0.0, sum(nc[i] * FILL[i] for i in range(3)))
    half = norm((KEY[0], KEY[1], KEY[2] + 1.0))
    spec = max(0.0, sum(nc[i] * half[i] for i in range(3))) ** power * gloss
    light = 0.42 + 0.9 * key + 0.3 * fill
    colour = tuple(base[i] * light + spec * 0.75 for i in range(3))
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
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(png(render()))
    print(OUT, OUT.stat().st_size)
