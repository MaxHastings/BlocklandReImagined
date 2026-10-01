"""Write the showcase Add-Ons' item icons: original art made here, so it
carries no one else's rights (it is not a render of any game model).

Outputs 128x128 RGBA PNG:
  packages/showcase/gravity-gun-tool/assets/icons/gravity_gun.png
    the Gravity Gun drawn like the game's other item icons: a small solid
    model, lit and shaded, seen from above and to one side, on a clear
    background (no outline or glow), pointing up and to the right as the
    Hammer and Wrench do. Its looks are the gun's in play: a dark shell,
    teal veins, green-lit edges and a teal muzzle.
  packages/showcase/grapple-rope-tool/assets/icons/grapple_rope.png
    the Grapple Rope the same way: a carved wooden launcher bound with
    bamboo bands and vine, a brass muzzle, and the three-pronged hook
    sitting in it, as the launcher looks in play.
  packages/showcase/grappling-hook-tool/assets/icons/grappling_hook.png
    the Grappling Hook the same way: a gunmetal winch gun with a brass
    drum wound with steel cable, and the four-claw grapnel with its red
    band seated in the muzzle.

Each model is a few rounded boxes, capsules and rings, ray marched with a
key light, a fill and a highlight. Run it again after changing the model below; the output is the
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


def shade(px, py, model=None):
    """Straight RGBA for the ray through one sample point."""
    if model is not None:
        return model(px, py)
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


# ---- The Grapple Rope ----

def capsule(p, a, b, r):
    """Distance to a capsule from `a` to `b`, radius `r`."""
    pa = tuple(p[i] - a[i] for i in range(3))
    ba = tuple(b[i] - a[i] for i in range(3))
    h = max(0.0, min(1.0, sum(pa[i] * ba[i] for i in range(3)) / sum(v * v for v in ba)))
    d = tuple(pa[i] - ba[i] * h for i in range(3))
    return math.sqrt(sum(v * v for v in d)) - r


def ring_x(p, cx, radius, tube):
    """A ring round the x axis at x = `cx`."""
    q = math.sqrt(p[1] * p[1] + p[2] * p[2]) - radius
    return math.sqrt((p[0] - cx) ** 2 + q * q) - tube


def cyl_x(p, x0, x1, r, round_=0.03):
    """A rounded cylinder along x from `x0` to `x1`."""
    dx = abs(p[0] - (x0 + x1) * 0.5) - (x1 - x0) * 0.5 + round_
    dr = math.sqrt(p[1] * p[1] + p[2] * p[2]) - r + round_
    return min(max(dx, dr), 0.0) + math.hypot(max(dx, 0.0), max(dr, 0.0)) - round_


def grapple_parts(p):
    """Wood (stock and barrel), bamboo bands, brass (muzzle, hook) and vine."""
    x, y, z = p
    barrel = cyl_x((x, y - 0.25, z), -0.55, 0.85, 0.2)
    stock = rbox(p, (-0.72, 0.18, 0.0), (0.3, 0.2, 0.16), 0.1)
    g = rot_z((x + 0.45, y, z), -GRIP)
    grip = rbox(g, (0.0, -0.3, 0.0), (0.13, 0.38, 0.14), 0.07)
    wood = smin(smin(barrel, stock, 0.12), grip, 0.12)
    bands = min(ring_x((x, y - 0.25, z), -0.2, 0.205, 0.045),
                ring_x((x, y - 0.25, z), 0.35, 0.205, 0.045))
    muzzle = cyl_x((x, y - 0.25, z), 0.8, 1.02, 0.24, 0.05)
    # The hook: a shaft out of the muzzle and three curled prongs.
    tip = (1.34, 0.25, 0.0)
    hook = capsule(p, (1.0, 0.25, 0.0), tip, 0.05)
    for a in (0.0, 2.094, 4.189):
        c, s_ = math.cos(a), math.sin(a)
        out = (1.24, 0.25 + 0.2 * c, 0.2 * s_)
        back = (1.12, 0.25 + 0.26 * c, 0.26 * s_)
        hook = min(hook, capsule(p, tip, out, 0.042), capsule(p, out, back, 0.036))
    brass = min(muzzle, hook)
    # A vine wound round the barrel between the bands.
    t = x * 10.0
    vine_centre = (y - 0.25 - 0.215 * math.cos(t), z - 0.215 * math.sin(t))
    vine = math.hypot(*vine_centre) - 0.03 if -0.12 < x < 0.28 else 9.0
    return wood, bands, brass, vine


def grapple_scene(p):
    return min(grapple_parts(p))


WOOD = (0.42, 0.24, 0.12)
WOOD_DARK = (0.24, 0.12, 0.05)
BAMBOO = (0.78, 0.68, 0.36)
BRASS = (0.86, 0.62, 0.24)
VINE = (0.2, 0.5, 0.16)


def march(px, py, scene):
    """The point and normal where the ray through pixel (px, py) meets
    `scene`, framed as the showcase launchers are; None on a miss."""
    scale = 2.35 / SIZE
    origin = to_object(((px - SIZE / 2) * scale + 0.24, (SIZE / 2 - py) * scale + 0.16, 4.0))
    ray = to_object((0.0, 0.0, -1.0))
    t = 0.0
    for _ in range(128):
        p = tuple(origin[i] + ray[i] * t for i in range(3))
        d = scene(p)
        if d < 1e-3:
            break
        t += d * 0.8
        if t > 8.0:
            return None
    else:
        return None
    e = 1e-3
    n = norm(tuple(
        scene(tuple(p[j] + (e if j == i else 0.0) for j in range(3)))
        - scene(tuple(p[j] - (e if j == i else 0.0) for j in range(3)))
        for i in range(3)))
    return p, n


def lit(n, base, gloss, power=20):
    """`base` lit by the key, fill and highlight, facing `n`."""
    nc = to_camera(n)
    key = max(0.0, sum(nc[i] * KEY[i] for i in range(3)))
    fill = max(0.0, sum(nc[i] * FILL[i] for i in range(3)))
    half = norm((KEY[0], KEY[1], KEY[2] + 1.0))
    spec = max(0.0, sum(nc[i] * half[i] for i in range(3))) ** power * gloss
    light = 0.45 + 0.95 * key + 0.3 * fill
    colour = tuple(base[i] * light + spec * 0.6 for i in range(3))
    return (*colour, 1.0)


def grapple_shade(px, py):
    hit = march(px, py, grapple_scene)
    if hit is None:
        return (0.0, 0.0, 0.0, 0.0)
    p, n = hit
    wood, bands, brass, vine = grapple_parts(p)
    nearest = min(wood, bands, brass, vine)
    if nearest == brass:
        base, gloss = BRASS, 1.0
    elif nearest == bands:
        base, gloss = BAMBOO, 0.35
    elif nearest == vine:
        base, gloss = VINE, 0.2
    else:
        # Grain running along the barrel, darker in its streaks.
        grain = 0.5 + 0.5 * math.sin(p[1] * 38.0 + math.sin(p[0] * 6.0) * 2.0 + p[2] * 11.0)
        base, gloss = mix(WOOD_DARK, WOOD, 0.35 + 0.65 * grain), 0.25
    return lit(n, base, gloss)


# ---- The Grappling Hook ----

def hook_parts(p):
    """Gunmetal (barrel, body, grip), brass (the winch drum's cheeks),
    steel cable (wound on the drum), bright steel (the grapnel's claws)
    and red (the band on its shank)."""
    x, y, z = p
    barrel = cyl_x((x, y - 0.25, z), -0.5, 0.9, 0.15)
    body = rbox(p, (-0.62, 0.2, 0.0), (0.32, 0.2, 0.15), 0.07)
    g = rot_z((x + 0.45, y, z), -GRIP)
    grip = rbox(g, (0.0, -0.3, 0.0), (0.12, 0.36, 0.13), 0.06)
    metal = smin(smin(barrel, body, 0.08), grip, 0.08)
    # The winch drum under the barrel: brass cheeks with cable between.
    drum_y = y - 0.02
    cheeks = min(ring_x((x, drum_y, z), -0.05, 0.13, 0.05), ring_x((x, drum_y, z), 0.35, 0.13, 0.05))
    coil = cyl_x((x, drum_y, z), -0.03, 0.33, 0.14, 0.02)
    muzzle = cyl_x((x, y - 0.25, z), 0.84, 1.0, 0.19, 0.04)
    brass = min(cheeks, muzzle)
    # The grapnel seated in the muzzle: a shank and four claws hooked back.
    tip = (1.38, 0.25, 0.0)
    shank = capsule(p, (1.0, 0.25, 0.0), tip, 0.05)
    band = capsule(p, (1.08, 0.25, 0.0), (1.16, 0.25, 0.0), 0.058)
    claws = shank
    for a in (0.4, 0.4 + 1.571, 0.4 + 3.142, 0.4 + 4.712):
        c, s_ = math.cos(a), math.sin(a)
        out = (1.3, 0.25 + 0.24 * c, 0.24 * s_)
        back = (1.14, 0.25 + 0.28 * c, 0.28 * s_)
        claws = min(claws, capsule(p, tip, out, 0.045), capsule(p, out, back, 0.03))
    return metal, brass, coil, claws, band


def hook_scene(p):
    return min(hook_parts(p))


GUNMETAL = (0.26, 0.28, 0.3)
CABLE = (0.66, 0.68, 0.7)
STEEL = (0.8, 0.82, 0.86)
RED = (0.7, 0.1, 0.06)


def hook_shade(px, py):
    hit = march(px, py, hook_scene)
    if hit is None:
        return (0.0, 0.0, 0.0, 0.0)
    p, n = hit
    metal, brass, coil, claws, band = hook_parts(p)
    nearest = min(metal, brass, coil, claws, band)
    if nearest == brass:
        return lit(n, BRASS, 1.0)
    if nearest == band:
        return lit(n, RED, 0.4)
    if nearest == claws:
        return lit(n, STEEL, 1.0, 30)
    if nearest == coil:
        # Turns of cable round the drum.
        turns = 0.6 + 0.4 * abs(math.sin(p[0] * 70.0))
        return lit(n, tuple(c * turns for c in CABLE), 0.8)
    # Brushed gunmetal with a row of rivets along the body.
    rivet = 1.0 if (abs(p[1] - 0.33) < 0.035 and (p[0] * 8.0) % 1.0 < 0.28 and p[0] < -0.35) else 0.0
    return lit(n, mix(GUNMETAL, (0.5, 0.5, 0.52), rivet), 0.6)


def render(model=None):
    rows = []
    step = 1.0 / SAMPLES
    for py in range(SIZE):
        row = bytearray([0])
        for px in range(SIZE):
            r = g = b = a = 0.0
            for sy in range(SAMPLES):
                for sx in range(SAMPLES):
                    cr, cg, cb, ca = shade(px + (sx + 0.5) * step, py + (sy + 0.5) * step, model)
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
    out = ROOT / 'grapple-rope-tool' / 'assets' / 'icons' / 'grapple_rope.png'
    out.write_bytes(png(render(grapple_shade)))
    print(out.relative_to(ROOT), out.stat().st_size)
    out = ROOT / 'grappling-hook-tool' / 'assets' / 'icons' / 'grappling_hook.png'
    out.write_bytes(png(render(hook_shade)))
    print(out.relative_to(ROOT), out.stat().st_size)
