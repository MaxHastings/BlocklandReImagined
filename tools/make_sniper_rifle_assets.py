"""Write the Sniper Rifle Add-On's generated art, sounds and data.

The Sniper Rifle is our own take on Kaje's classic Blockland Sniper Rifle
(and Conan's "Sniper Rifle Updated"): their design, built here from
nothing of theirs. Everything below is original and generated.

Outputs, under packages/showcase/sniper-rifle/assets/:
  models/sniper-rifle.shape.json  a bolt-action rifle: walnut stock, blued
                                  steel barrel and action, a black scope
                                  with coated lenses, and a bolt that
                                  works (its `Bolt` animation lifts, draws
                                  and closes it)
  textures/*.png                  the walnut's grain, the steel's tint and
                                  fine brushing, the black furniture and
                                  the lens coating
  scope/scope.png                 the scope's picture, drawn over the
                                  screen while aiming: a clear lens with a
                                  duplex mil-dot reticle, darkening to the
                                  tube's black at its rim
  icons/sniper_rifle.png          the item icon, drawn from the model as
                                  the stock icons are; the game draws its
                                  own from the model on each player's
                                  machine when it can
                                  (icons/sniper_rifle.render.json)
  sounds/shot.wav                 the shot: a supersonic crack, a heavy
                                  report and a long rolling echo
  sounds/bolt.wav                 working the bolt: lift, draw, the case
                                  flicking out, push and lock
  weapons.json                    the item, image, round and effects
  presentation.json,              the model and textures, bound to
  item-physics.json               weapons.json by its SHA-256

Run it again after changing anything below. Only the Python standard
library is used, with fixed seeds, so every run writes the same bytes.
"""
import hashlib
import json
import math
import random
import struct
import wave
import zlib
from pathlib import Path

ADDON = Path(__file__).resolve().parent.parent / 'packages' / 'showcase' / 'sniper-rifle'
ASSETS = ADDON / 'assets'
NS = 'sniper-rifle'
MODEL_KEY = f'{NS}:model/rifle'

# The bore's height above the grip, and where the scope's axis sits.
BORE_Y = 0.09
SCOPE_Y = 0.23
MUZZLE_Z = -1.54


# ---------------------------------------------------------------- geometry
# Native axes: x right, y up, -z forward. The origin is the top of the
# wrist, under the action, which the first-person eye offset places; the
# hand holds the pistol grip below it (`mountPoint`).

class Part:
    """Triangles of one material, in one node's space."""

    def __init__(self):
        self.positions, self.normals, self.uv, self.triangles = [], [], [], []

    def vertex(self, p, n, uv):
        self.positions.append([round(c, 6) for c in p])
        self.normals.append([round(c, 6) for c in n])
        self.uv.append([round(c, 6) for c in uv])
        return len(self.positions) - 1


def sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def norm(v):
    n = math.sqrt(v[0] ** 2 + v[1] ** 2 + v[2] ** 2) or 1.0
    return (v[0] / n, v[1] / n, v[2] / n)


def quad(part, a, b, c, d, uv_scale=1.0):
    """A flat quad a-b-c-d, counter-clockwise seen from outside."""
    n = norm(cross(sub(b, a), sub(c, a)))
    # Texture across the quad's own two edges, in world units, so grain
    # and brushing keep one scale on every face.
    u_len = math.dist(a, b) * uv_scale
    v_len = math.dist(a, d) * uv_scale
    i = [part.vertex(p, n, uv) for p, uv in
         ((a, (0, 0)), (b, (u_len, 0)), (c, (u_len, v_len)), (d, (0, v_len)))]
    part.triangles += [[i[0], i[1], i[2]], [i[0], i[2], i[3]]]


def hexahedron(part, c, uv_scale=1.0):
    """Eight corners, the first four the back face (+z) and the last four
    the front (-z), each face's corners in order round it: a box that may
    taper or lean. `c[i]` for i in 0..4 go left-bottom, right-bottom,
    right-top, left-top."""
    b0, b1, b2, b3, f0, f1, f2, f3 = c
    quad(part, b0, b1, b2, b3, uv_scale)          # back, facing +z
    quad(part, f1, f0, f3, f2, uv_scale)          # front, facing -z
    quad(part, f0, b0, b3, f3, uv_scale)          # left
    quad(part, b1, f1, f2, b2, uv_scale)          # right
    quad(part, b3, b2, f2, f3, uv_scale)          # top
    quad(part, f0, f1, b1, b0, uv_scale)          # bottom


def box(part, x, y, z, uv_scale=1.0):
    (x0, x1), (y0, y1), (z0, z1) = x, y, z
    # z1 is the back (+z), z0 the front.
    hexahedron(part, [
        (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1),
        (x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0),
    ], uv_scale)


def tapered(part, x_back, y_back, x_front, y_front, z_back, z_front, uv_scale=1.0):
    (a0, a1), (b0, b1) = x_back, y_back
    (c0, c1), (d0, d1) = x_front, y_front
    hexahedron(part, [
        (a0, b0, z_back), (a1, b0, z_back), (a1, b1, z_back), (a0, b1, z_back),
        (c0, d0, z_front), (c1, d0, z_front), (c1, d1, z_front), (c0, d1, z_front),
    ], uv_scale)


def tube(part, p0, p1, r0, r1, segments=20, cap0=True, cap1=True, uv_scale=1.0):
    """A cylinder or cone from `p0` (radius r0) to `p1` (radius r1),
    smooth sided, with flat caps."""
    axis = norm(sub(p1, p0))
    helper = (0.0, 1.0, 0.0) if abs(axis[1]) < 0.9 else (1.0, 0.0, 0.0)
    u = norm(cross(axis, helper))
    v = cross(axis, u)
    length = math.dist(p0, p1)
    slope = (r0 - r1) / length if length else 0.0
    ring0, ring1 = [], []
    for s in range(segments + 1):
        a = 2 * math.pi * s / segments
        d = tuple(math.cos(a) * u[k] + math.sin(a) * v[k] for k in range(3))
        n = norm(tuple(d[k] + axis[k] * slope for k in range(3)))
        su = s / segments * 2 * math.pi * max(r0, r1) * uv_scale
        ring0.append(part.vertex(tuple(p0[k] + d[k] * r0 for k in range(3)), n, (su, 0)))
        ring1.append(part.vertex(tuple(p1[k] + d[k] * r1 for k in range(3)), n, (su, length * uv_scale)))
    for s in range(segments):
        a, b, c, d = ring0[s], ring0[s + 1], ring1[s + 1], ring1[s]
        part.triangles += [[a, b, c], [a, c, d]]
    for cap, p, r, n in ((cap0, p0, r0, tuple(-c for c in axis)), (cap1, p1, r1, axis)):
        if not cap or r <= 0:
            continue
        centre = part.vertex(p, n, (0.5, 0.5))
        rim = []
        for s in range(segments):
            a = 2 * math.pi * s / segments
            d = tuple(math.cos(a) * u[k] + math.sin(a) * v[k] for k in range(3))
            rim.append(part.vertex(tuple(p[k] + d[k] * r for k in range(3)), n,
                                   (0.5 + 0.5 * math.cos(a), 0.5 + 0.5 * math.sin(a))))
        for s in range(segments):
            a, b = rim[s], rim[(s + 1) % segments]
            part.triangles.append([centre, a, b] if n is axis else [centre, b, a])


MATERIALS = ['walnut', 'steel', 'black', 'scope', 'lens', 'steel-detail']
# Where the hand holds it (`mountPoint`): the pistol grip.
GRIP = (0.0, -0.17, 0.13)
# The bolt's axis, where its node sits.
BOLT = (0.0, BORE_Y, 0.02)


def rifle():
    """Per node, per material, the parts."""
    body = {m: Part() for m in MATERIALS}
    bolt = {m: Part() for m in MATERIALS}
    wood, steel, black = body['walnut'], body['steel'], body['black']
    scope, lens = body['scope'], body['lens']
    grain = 4.0
    # Stock: butt, cheek riser, wrist, pistol grip, forend.
    tapered(wood, (-0.05, 0.05), (-0.25, 0.02), (-0.047, 0.047), (-0.16, 0.03), 0.80, 0.44, grain)
    tapered(wood, (-0.046, 0.046), (0.02, 0.085), (-0.04, 0.04), (0.03, 0.06), 0.74, 0.40, grain)
    tapered(wood, (-0.047, 0.047), (-0.16, 0.03), (-0.043, 0.043), (-0.09, 0.045), 0.44, 0.10, grain)
    hexahedron(wood, [
        (-0.038, -0.30, 0.25), (0.038, -0.30, 0.25), (0.04, -0.07, 0.18), (-0.04, -0.07, 0.18),
        (-0.038, -0.30, 0.13), (0.038, -0.30, 0.13), (0.04, -0.07, 0.04), (-0.04, -0.07, 0.04),
    ], grain)
    tapered(wood, (-0.048, 0.048), (-0.095, 0.05), (-0.04, 0.04), (-0.065, 0.045), 0.10, -0.95, grain)
    # Recoil pad and grip cap.
    tapered(black, (-0.052, 0.052), (-0.255, 0.025), (-0.052, 0.052), (-0.255, 0.025), 0.84, 0.80)
    box(black, (-0.04, 0.04), (-0.315, -0.30), (0.13, 0.25))
    # Action: receiver, bolt shroud, magazine floorplate.
    tube(steel, (0, BORE_Y, 0.14), (0, BORE_Y, -0.36), 0.046, 0.046, 24, uv_scale=6)
    box(black, (-0.042, 0.042), (-0.115, -0.09), (-0.30, -0.02))
    # Barrel, tapering, with a muzzle brake and its side ports.
    tube(steel, (0, BORE_Y, -0.36), (0, BORE_Y, -1.40), 0.031, 0.023, 20, cap0=False, uv_scale=6)
    tube(steel, (0, BORE_Y, -1.40), (0, BORE_Y, MUZZLE_Z), 0.034, 0.034, 20, uv_scale=6)
    for z in (-1.44, -1.49):
        box(black, (-0.036, 0.036), (BORE_Y - 0.012, BORE_Y + 0.012), (z - 0.016, z))
    # Trigger guard and trigger.
    box(black, (-0.012, 0.012), (-0.145, -0.13), (-0.10, 0.06))
    box(black, (-0.012, 0.012), (-0.13, -0.09), (-0.115, -0.10))
    box(steel, (-0.008, 0.008), (-0.125, -0.07), (-0.02, 0.0))
    # Scope: tube, objective and eyepiece bells, turrets, rings and bases.
    tube(scope, (0, SCOPE_Y, 0.10), (0, SCOPE_Y, -0.40), 0.032, 0.032, 24, False, False, 6)
    tube(scope, (0, SCOPE_Y, -0.40), (0, SCOPE_Y, -0.52), 0.032, 0.058, 24, False, False, 6)
    tube(scope, (0, SCOPE_Y, -0.52), (0, SCOPE_Y, -0.68), 0.058, 0.058, 24, False, True, 6)
    tube(scope, (0, SCOPE_Y, 0.10), (0, SCOPE_Y, 0.17), 0.032, 0.046, 24, False, False, 6)
    tube(scope, (0, SCOPE_Y, 0.17), (0, SCOPE_Y, 0.30), 0.046, 0.046, 24, False, True, 6)
    tube(scope, (0, SCOPE_Y + 0.025, -0.16), (0, SCOPE_Y + 0.075, -0.16), 0.022, 0.022, 16)
    tube(scope, (0.025, SCOPE_Y, -0.16), (0.075, SCOPE_Y, -0.16), 0.022, 0.022, 16)
    for z in (-0.30, 0.02):
        tube(black, (0, SCOPE_Y, z + 0.02), (0, SCOPE_Y, z - 0.02), 0.040, 0.040, 24)
        box(black, (-0.022, 0.022), (BORE_Y + 0.04, SCOPE_Y - 0.035), (z - 0.018, z + 0.018))
    # The lenses, set just inside the bells' rims.
    tube(lens, (0, SCOPE_Y, -0.672), (0, SCOPE_Y, -0.673), 0.052, 0.052, 24, False, True)
    tube(lens, (0, SCOPE_Y, 0.293), (0, SCOPE_Y, 0.294), 0.040, 0.040, 24, False, True)
    # The bolt, in its node's space (on the bore's axis): the part of its
    # body showing behind the receiver, and the handle out to the right
    # with its knob, which the Bolt animation turns up and draws back.
    bs = bolt['steel']
    tube(bs, (0, 0, 0.12), (0, 0, 0.23), 0.030, 0.030, 20)
    tube(bs, (0.028, 0, 0.10), (0.13, -0.035, 0.12), 0.011, 0.011, 12)
    tube(bs, (0.12, -0.032, 0.12), (0.17, -0.045, 0.125), 0.024, 0.024, 16)
    return body, bolt


def mesh_of(parts):
    """One mesh from per-material parts: positions, normals and uvs shared,
    a primitive per material."""
    mesh = {'frame_vertices': 0, 'positions': [], 'normals': [], 'uv': [], 'primitives': [],
            'skin': None, 'billboard': False, 'billboard_y': False}
    for index, name in enumerate(MATERIALS):
        part = parts[name]
        if not part.triangles:
            continue
        base = len(mesh['positions'])
        mesh['positions'] += part.positions
        mesh['normals'] += part.normals
        mesh['uv'] += part.uv
        mesh['primitives'].append({'material': index,
                                   'triangles': [[a + base, b + base, c + base] for a, b, c in part.triangles]})
    mesh['frame_vertices'] = len(mesh['positions'])
    return mesh


def quat_z(degrees):
    a = math.radians(degrees) / 2
    return [0.0, 0.0, round(math.sin(a), 6), round(math.cos(a), 6)]


def smooth(t):
    t = min(1.0, max(0.0, t))
    return t * t * (3 - 2 * t)


BOLT_SECONDS = 0.9
BOLT_FPS = 30


def bolt_animation():
    """Lift the handle, draw the bolt back, push it home and lock it down:
    the rotation is about the bore, the draw along it."""
    frames = int(BOLT_SECONDS * BOLT_FPS) + 1
    rotations, translations = [], []
    lift, draw = 62.0, 0.17
    for f in range(frames):
        t = f / (frames - 1)
        if t < 0.18:
            turn, back = lift * smooth(t / 0.18), 0.0
        elif t < 0.45:
            turn, back = lift, draw * smooth((t - 0.18) / 0.27)
        elif t < 0.72:
            turn, back = lift, draw * (1 - smooth((t - 0.45) / 0.27))
        elif t < 0.88:
            turn, back = lift * (1 - smooth((t - 0.72) / 0.16)), 0.0
        else:
            turn, back = 0.0, 0.0
        rotations.append(quat_z(turn))
        translations.append([BOLT[0], BOLT[1], round(BOLT[2] + back, 6)])
    return {'name': 'Bolt', 'frames': frames, 'duration': BOLT_SECONDS, 'looping': False,
            'additive': False, 'priority': 0,
            'nodes': [{'node': 'bolt', 'rotations': rotations, 'translations': translations,
                       'scales': [], 'scale_rotations': []}],
            'objects': [], 'ground_translations': [], 'ground_rotations': [], 'triggers': []}


def material(name, blend='opaque', metal=None):
    m = {'name': name, 'wrap_u': True, 'wrap_v': True, 'blend': blend, 'unlit': False,
         'environment': False, 'mipmaps': True, 'detail_map': None, 'bump_map': None,
         'reflectance_map': None, 'detail_scale': 1.0, 'reflectance': 1.0}
    if metal:
        m['metal'] = metal
    return m


DETAIL = MATERIALS.index('steel-detail')


def shape():
    body, bolt = rifle()
    nodes = [
        {'name': 'Root', 'parent': None, 'translation': [0.0, 0.0, 0.0], 'rotation': [0.0, 0.0, 0.0, 1.0]},
        {'name': 'mountPoint', 'parent': 0, 'translation': list(GRIP), 'rotation': [0.0, 0.0, 0.0, 1.0]},
        {'name': 'muzzlePoint', 'parent': 0, 'translation': [0.0, BORE_Y, MUZZLE_Z],
         'rotation': [0.0, 0.0, 0.0, 1.0]},
        {'name': 'ejectPoint', 'parent': 0, 'translation': [0.05, BORE_Y + 0.03, -0.06],
         'rotation': [0.0, 0.0, 0.0, 1.0]},
        {'name': 'bolt', 'parent': 0, 'translation': list(BOLT), 'rotation': [0.0, 0.0, 0.0, 1.0]},
    ]
    return {
        'schema_version': 1,
        'id': f'{NS}:file/models/sniper-rifle.shape.json',
        'nodes': nodes,
        'objects': [
            {'name': 'rifle', 'node': 0, 'meshes': [0], 'visibility': 1.0, 'frame': 0, 'material_frame': 0},
            {'name': 'bolt', 'node': 4, 'meshes': [1], 'visibility': 1.0, 'frame': 0, 'material_frame': 0},
        ],
        'details': [{'name': 'detail100', 'pixel_threshold': 100.0, 'object_start': 0, 'object_count': 2,
                     'mesh_offset': 0, 'collision': False}],
        'meshes': [mesh_of(body), mesh_of(bolt)],
        'materials': [
            material('walnut'),
            # Blued steel: dark, a little rough, brushed along its length.
            material('steel', metal={'color': [0.30, 0.31, 0.34], 'roughness': 0.34, 'detail': DETAIL,
                                     'detail_scale': 1.0, 'detail_strength': 0.6}),
            material('black'),
            # The scope's anodised tube: nearly black, satin.
            material('scope', metal={'color': [0.05, 0.05, 0.055], 'roughness': 0.42, 'detail': DETAIL,
                                     'detail_scale': 1.0, 'detail_strength': 0.3}),
            # Coated glass: a mirror-smooth violet and green sheen.
            material('lens', metal={'color': [0.16, 0.12, 0.30], 'roughness': 0.04}),
            # The steel's detail only: no face uses it.
            material('steel-detail'),
        ],
        'animations': [bolt_animation()],
    }


# ---------------------------------------------------------------- pictures

def png(width, height, pixel, channels=3):
    raw = bytearray()
    for y in range(height):
        raw.append(0)
        for x in range(width):
            raw.extend(pixel(x, y))

    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data) & 0xFFFFFFFF)
    colour = 6 if channels == 4 else 2
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, colour, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(bytes(raw), 9)) + chunk(b'IEND', b''))


class Noise:
    """Smooth value noise wrapping every `period` cells, so it tiles."""

    def __init__(self, seed, period):
        rng = random.Random(seed)
        self.period = period
        self.cells = [[rng.random() for _ in range(period)] for _ in range(period)]

    def at(self, x, y):
        p = self.period
        x0, y0 = int(math.floor(x)), int(math.floor(y))
        fx, fy = x - x0, y - y0
        fx, fy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
        c = self.cells
        a = c[y0 % p][x0 % p] + (c[y0 % p][(x0 + 1) % p] - c[y0 % p][x0 % p]) * fx
        b = c[(y0 + 1) % p][x0 % p] + (c[(y0 + 1) % p][(x0 + 1) % p] - c[(y0 + 1) % p][x0 % p]) * fx
        return a + (b - a) * fy


def clamp8(v):
    return max(0, min(255, int(round(v))))


def walnut():
    """Walnut: dark and warm, its grain running along u (the stock's
    length), with darker streaks and a few pores. Tiles."""
    size = 128
    warp = Noise(3, 8)
    streak = Noise(7, 16)
    rng = random.Random(41)
    pores = {(rng.randrange(size), rng.randrange(size)) for _ in range(90)}

    def pixel(x, y):
        u, v = x / size, y / size
        w = warp.at(u * 8, v * 8)
        ring = 0.5 + 0.5 * math.sin((v * 22 + w * 2.2) * math.pi)
        dark = 0.72 + 0.2 * ring + 0.12 * (streak.at(u * 2, v * 16) - 0.5)
        if (x, y) in pores:
            dark *= 0.8
        return bytes((clamp8(118 * dark), clamp8(70 * dark), clamp8(40 * dark), 255))
    return png(size, size, pixel, 4)


def flat(rgb):
    return png(8, 8, lambda x, y: bytes((*rgb, 255)), 4)


def steel_detail():
    """The steel's fine surface, in linear channels as the metal expects:
    red roughness (128 keeps it), green grime (255 clean), blue and alpha
    tilt along u and v (128 flat). Brushed along u, with light pitting."""
    size = 64
    rng = random.Random(97)
    rows = [rng.uniform(-1, 1) for _ in range(size)]
    height = [[0.004 * (rows[y] * 0.7 + rng.uniform(-0.3, 0.3)) for x in range(size)] for y in range(size)]
    pits = Noise(13, 8)

    def pixel(x, y):
        dv = height[(y + 1) % size][x] - height[(y - 1) % size][x]
        du = height[y][(x + 1) % size] - height[y][(x - 1) % size]
        p = pits.at(x / size * 8, y / size * 8)
        rough = 128 + 26 * (p - 0.5)
        grime = 255 - 30 * max(0.0, p - 0.6)
        return bytes((clamp8(rough), clamp8(grime), clamp8(128 - du * 4000), clamp8(128 - dv * 4000)))
    return png(size, size, pixel, 4)


def scope_overlay():
    """The view through the scope. A clear lens, darkening at its rim into
    the tube's black, with a duplex reticle: thick posts from the edge that
    thin to fine cross hairs at the middle, mil dots along them, and a gap
    at the very centre so the target stays in sight."""
    size = 1024
    ss = 3  # samples per side, per pixel
    c = size / 2
    lens = 0.47 * size
    rim = 0.06 * size

    def cover(px, py):
        """Reticle and tube, 0 clear to 1 black, at one sample."""
        dx, dy = px - c, py - c
        r = math.hypot(dx, dy)
        if r >= lens:
            return 1.0
        # The rim of the tube shades in, as a real eyepiece does.
        shade = max(0.0, (r - (lens - rim)) / rim) ** 1.6
        ax, ay = abs(dx), abs(dy)
        line = 0.0
        post, fine = 0.0105 * size, 0.0016 * size
        inner = 0.22 * size
        gap = 0.012 * size
        for along, across in ((ax, ay), (ay, ax)):
            if along > inner and across <= post:
                line = 1.0
            elif gap < along <= inner and across <= fine:
                line = 1.0
        # Mil dots: every 0.04 of the size out to the posts.
        step = 0.04 * size
        for along, across in ((ax, ay), (ay, ax)):
            k = round(along / step)
            if 1 <= k <= 5 and math.hypot(along - k * step, across) <= 0.0045 * size:
                line = 1.0
        return max(line, shade)

    def pixel(x, y):
        total = 0.0
        for sy in range(ss):
            for sx in range(ss):
                total += cover(x + (sx + 0.5) / ss, y + (sy + 0.5) / ss)
        a = total / (ss * ss)
        return bytes((0, 0, 0, clamp8(255 * a)))
    return png(size, size, pixel, 4)


# The icon: the model drawn as the stock icons are, small, lit and on a
# clear background, pointing up and to the right like the Rocket Launcher.
ICON = 128
ICON_BASE = (0.30, 0.30, 0.32)


def icon(model):
    tris = []
    for obj, mesh in zip(model['objects'], model['meshes']):
        offset = model['nodes'][obj['node']]['translation']
        for prim in mesh['primitives']:
            if prim['material'] == DETAIL:
                continue
            for t in prim['triangles']:
                tris.append([[mesh['positions'][i][k] + offset[k] for k in range(3)] for i in t])
    # Turn it side on (barrel to the right), tilt it up the diagonal, then
    # a little towards the viewer so the top of the scope shows.
    yaw, roll, tilt = math.radians(-90), math.radians(38), math.radians(20)

    def turn(p):
        x, y, z = p
        x, z = x * math.cos(yaw) - z * math.sin(yaw), x * math.sin(yaw) + z * math.cos(yaw)
        y, z = y * math.cos(tilt) - z * math.sin(tilt), y * math.sin(tilt) + z * math.cos(tilt)
        x, y = x * math.cos(roll) - y * math.sin(roll), x * math.sin(roll) + y * math.cos(roll)
        return (x, y, z)
    tris = [[turn(p) for p in t] for t in tris]
    xs = [p[0] for t in tris for p in t]
    ys = [p[1] for t in tris for p in t]
    span = max(max(xs) - min(xs), max(ys) - min(ys))
    scale = ICON * 0.9 / span
    cx, cy = (max(xs) + min(xs)) / 2, (max(ys) + min(ys)) / 2
    ss = 3
    n = ICON * ss
    depth = [[-1e9] * n for _ in range(n)]
    colour = [[None] * n for _ in range(n)]
    light = norm((-0.4, 0.8, 0.6))
    for t in tris:
        p = [((q[0] - cx) * scale * ss + n / 2, n / 2 - (q[1] - cy) * scale * ss, q[2]) for q in t]
        normal = norm(cross(sub(t[1], t[0]), sub(t[2], t[0])))
        if normal[2] <= 0:
            continue
        lit = 0.45 + 0.6 * max(0.0, sum(normal[k] * light[k] for k in range(3)))
        x0, x1 = max(0, int(min(q[0] for q in p))), min(n - 1, int(max(q[0] for q in p)) + 1)
        y0, y1 = max(0, int(min(q[1] for q in p))), min(n - 1, int(max(q[1] for q in p)) + 1)
        (ax, ay, az), (bx, by, bz), (qx, qy, qz) = p
        area = (bx - ax) * (qy - ay) - (by - ay) * (qx - ax)
        if abs(area) < 1e-9:
            continue
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                sx, sy = x + 0.5, y + 0.5
                w0 = ((bx - sx) * (qy - sy) - (by - sy) * (qx - sx)) / area
                w1 = ((qx - sx) * (ay - sy) - (qy - sy) * (ax - sx)) / area
                w2 = 1 - w0 - w1
                if min(w0, w1, w2) < 0:
                    continue
                z = w0 * az + w1 * bz + w2 * qz
                if z > depth[y][x]:
                    depth[y][x] = z
                    colour[y][x] = lit

    def pixel(x, y):
        r = g = b = a = 0.0
        for sy in range(ss):
            for sx in range(ss):
                lit = colour[y * ss + sy][x * ss + sx]
                if lit is not None:
                    r += ICON_BASE[0] * lit
                    g += ICON_BASE[1] * lit
                    b += ICON_BASE[2] * lit
                    a += 1
        if a == 0:
            return bytes((0, 0, 0, 0))
        return bytes((clamp8(255 * r / a), clamp8(255 * g / a), clamp8(255 * b / a), clamp8(255 * a / ss / ss)))
    return png(ICON, ICON, pixel, 4)


# ---------------------------------------------------------------- sounds

RATE = 44100
sound_rng = random.Random(20260930)


def noise(seconds):
    return [sound_rng.uniform(-1, 1) for _ in range(int(seconds * RATE))]


def lowpass(samples, cutoff):
    a = 1.0 - math.exp(-2 * math.pi * cutoff / RATE)
    out, y = [], 0.0
    for x in samples:
        y += a * (x - y)
        out.append(y)
    return out


def highpass(samples, cutoff):
    return [x - y for x, y in zip(samples, lowpass(samples, cutoff))]


def decay(samples, seconds):
    return [x * math.exp(-i / RATE / seconds) for i, x in enumerate(samples)]


def tone(seconds, f0, f1):
    out, phase = [], 0.0
    n = int(seconds * RATE)
    for i in range(n):
        phase += 2 * math.pi * (f0 + (f1 - f0) * i / n) / RATE
        out.append(math.sin(phase))
    return out


def at(seconds, samples):
    return [0.0] * int(seconds * RATE) + samples


def mix(*layers):
    n = max(len(layer) for layer, _ in layers)
    out = [0.0] * n
    for layer, gain in layers:
        for i, x in enumerate(layer):
            out[i] += x * gain
    return out


def click(pitch, seconds=0.03, ring=0.4):
    body = highpass(noise(seconds), pitch)
    return decay(mix((body, 0.8), (tone(seconds, pitch * 1.4, pitch), ring)), seconds / 4)


def slide(seconds, f0, f1):
    """Metal on metal: filtered noise swept in pitch, swelling and fading."""
    raw = noise(seconds)
    n = len(raw)
    out, lo, band = [], 0.0, 0.0
    for i, x in enumerate(raw):
        f = f0 + (f1 - f0) * i / n
        a = 1.0 - math.exp(-2 * math.pi * f / RATE)
        lo += a * (x - lo)
        band += 0.5 * (lo - band)
        out.append((lo - band) * math.sin(math.pi * i / n))
    return out


def shot():
    crack = decay(highpass(noise(0.03), 4000), 0.004)
    report = decay(mix((lowpass(noise(0.5), 1400), 1.0), (tone(0.5, 95, 38), 0.9)), 0.07)
    body = decay(lowpass(noise(1.0), 500), 0.22)
    # The echo: the report thrown back from far off, twice, softer and duller.
    echo1 = at(0.28, decay(lowpass(noise(0.8), 600), 0.25))
    echo2 = at(0.62, decay(lowpass(noise(0.9), 380), 0.35))
    return mix((crack, 1.0), (report, 1.0), (body, 0.7), (echo1, 0.28), (echo2, 0.16))


def bolt_sound():
    # Timed with the Bolt animation: lift, draw, eject, push, lock.
    t = BOLT_SECONDS
    return mix(
        (at(0.02 * t, click(1800, 0.05)), 0.9),
        (at(0.18 * t, slide(0.24 * t, 900, 2600)), 0.55),
        (at(0.44 * t, click(3200, 0.03, 0.7)), 0.6),
        (at(0.47 * t, decay(tone(0.12, 5200, 4700), 0.03)), 0.12),
        (at(0.45 * t, slide(0.24 * t, 2600, 1000)), 0.5),
        (at(0.70 * t, click(1500, 0.05)), 0.9),
        (at(0.86 * t, click(2200, 0.04)), 0.8),
    )


def wav(samples):
    import io
    peak = max(abs(x) for x in samples) or 1.0
    buffer = io.BytesIO()
    with wave.open(buffer, 'wb') as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(b''.join(struct.pack('<h', int(x / peak * 0.9 * 32767)) for x in samples))
    return buffer.getvalue()


# ---------------------------------------------------------------- data

def ticks(seconds):
    return round(seconds * 120)


def weapons():
    item, image, round_ = f'{NS}:weapon/sniperrifle', f'{NS}:image/sniperrifle', f'{NS}:projectile/round'
    ring = {
        # Kaje's blue ring trail, a faint ring left every millimetre of the
        # round's flight that grows and fades.
        'id': f'{NS}:particle/trail', 'texture': 'base/data/particles/thinRing', 'alpha_blend': False,
        'lifetime': 0.625, 'lifetime_variance': 0.055, 'drag': 3.0, 'wind': 0.0, 'gravity': 0.0,
        'inherited_velocity': 0.0, 'acceleration': 0.0, 'spin_degrees': 10.0, 'random_spin': [-500.0, 500.0],
        'keys': [{'time': 0.0, 'color': [0.3, 0.3, 0.9, 0.4], 'size': 0.15},
                 {'time': 1.0, 'color': [0.5, 0.5, 0.5, 0.0], 'size': 0.25}],
    }
    smoke = {
        # The muzzle smoke that curls up after each shot.
        'id': f'{NS}:particle/smoke', 'texture': 'base/data/particles/cloud', 'alpha_blend': False,
        'lifetime': 1.525, 'lifetime_variance': 0.055, 'drag': 3.0, 'wind': 0.0, 'gravity': -0.5,
        'inherited_velocity': 0.2, 'acceleration': 0.0, 'spin_degrees': 10.0, 'random_spin': [-500.0, 500.0],
        'keys': [{'time': 0.0, 'color': [0.5, 0.5, 0.5, 0.9], 'size': 0.15},
                 {'time': 1.0, 'color': [0.5, 0.5, 0.5, 0.0], 'size': 0.25}],
    }

    def emitter(id_, particle, period, speed, variance, theta):
        return {'id': id_, 'name': '', 'particles': [particle], 'period': period, 'period_variance': 0.0,
                'speed': speed, 'speed_variance': variance, 'offset': 0.0, 'offset_variance': 0.0,
                'theta_degrees': theta, 'phi_rate_degrees': 0.0, 'phi_variance_degrees': 360.0,
                'lifetime': 0.0, 'lifetime_variance': 0.0, 'orient': False, 'orient_on_velocity': True,
                'override_advance': False, 'use_emitter_colors': False, 'use_emitter_sizes': False,
                'use_placement_velocity': False, 'node_time_scale': 1.0, 'point_node_time_scale': 1.0}
    zoom = {
        'fov': 22.0, 'on_jet': True, 'jets': False, 'crosshair': False, 'first_person': True,
        'levels': [10.0],
        'overlay': 'scope/scope',
        'sway': {'degrees': 0.35, 'seconds': 4.5, 'crouched': 0.3, 'moving': 2.5},
    }
    states = [
        {'name': 'Activate', 'ticks': ticks(0.15), 'timeout': 1, 'sound': 'weaponSwitchSound'},
        {'name': 'Ready', 'down': 2},
        {'name': 'Fire', 'ticks': ticks(0.14), 'timeout': 3, 'allow_change': False, 'script': 'onFire',
         'sound': f'{NS}:shot', 'emitter': 'GunFlashEmitter', 'emitter_node': 'muzzlePoint',
         'emitter_seconds': 0.05},
        {'name': 'Smoke', 'ticks': ticks(0.35), 'timeout': 4, 'allow_change': False,
         'emitter': f'{NS}:emitter/smoke', 'emitter_node': 'muzzlePoint', 'emitter_seconds': 1.6},
        {'name': 'Bolt', 'ticks': ticks(BOLT_SECONDS + 0.1), 'timeout': 5, 'allow_change': False,
         'sequence': 'Bolt', 'sound': f'{NS}:bolt', 'eject_shell': True},
        {'name': 'Reload', 'up': 1},
    ]
    return {
        'schema_version': 3,
        'id': NS,
        'items': {
            item: {'ui_name': 'Sniper Rifle', 'image': image, 'model': MODEL_KEY,
                   'icon': 'icons/sniper_rifle', 'can_drop': True},
        },
        'images': {
            image: {
                'name': 'SniperRifleImage', 'model': MODEL_KEY, 'projectile': round_,
                'mount_point': 0, 'eye_offset': [0.42, -0.45, -1.0], 'correct_muzzle': True,
                'color': [1.0, 1.0, 1.0, 1.0], 'arm_ready': True, 'casing': 'GunShellDebris',
                'min_shot_ticks': ticks(1.5), 'fire_animation': 'shiftAway',
                'zoom': zoom, 'states': states,
            },
        },
        'projectiles': {
            round_: {
                'name': 'SniperRifleProjectile', 'speed': 2000.0, 'inherit': 1.0, 'gravity': 0.0,
                'lifetime_ticks': ticks(4.0), 'fade_ticks': ticks(3.5), 'damage': 150.0,
                'damage_type': '$DamageType::SniperRifle', 'radius_damage_type': '$DamageType::SniperRifle',
                'impulse': 1200.0, 'vertical': 1400.0, 'elasticity': 0.5, 'friction': 0.2,
                'explosion': {'effect': 'gunExplosion', 'damage': 0.0, 'radius': 0.0, 'impulse': 0.0,
                              'impulse_radius': 0.0, 'impulse_vertical': 0.0, 'burn_seconds': 0.0},
                'brick': {'radius': 0.0, 'direct': True, 'force': 25.0, 'max_volume': 200.0,
                          'max_floating_volume': 200.0},
                'trail': f'{NS}:emitter/trail',
            },
        },
        'damage_types': {
            'sniperrifle': {'name': 'SniperRifle', 'suicide_message': '%1 shot themselves with a sniper rifle',
                            'murder_message': '%2 sniped %1', 'vehicle_scale': 0.5, 'direct': True},
        },
        'sounds': {
            f'{NS}:shot': {'file': 'sounds/shot.wav', 'volume': 1.0},
            f'{NS}:bolt': {'file': 'sounds/bolt.wav', 'volume': 0.7},
        },
        'effects': {
            'particles': [ring, smoke],
            'emitters': [
                emitter(f'{NS}:emitter/trail', ring['id'], 0.001, 0.0, 0.0, [0.0, 90.0]),
                emitter(f'{NS}:emitter/smoke', smoke['id'], 0.01, 1.0, 1.0, [0.0, 90.0]),
            ],
        },
    }


def sha(data):
    return hashlib.sha256(data).hexdigest()


def write(rel, data):
    path = ASSETS / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return data


def dump(value):
    return (json.dumps(value, indent=2) + '\n').encode()


def main():
    model = shape()
    textures = {
        'walnut': walnut(),
        'steel': flat((236, 240, 248)),
        'black': flat((24, 24, 26)),
        'scope': flat((250, 250, 250)),
        'lens': flat((255, 255, 255)),
        'steel-detail': steel_detail(),
    }
    texture_keys = []
    texture_resources = {}
    for name in MATERIALS:
        data = write(f'textures/{name}.png', textures[name])
        key = f'{NS}:texture/{name}'
        texture_keys.append(key)
        w, h = struct.unpack('>II', data[16:24])
        texture_resources[key] = {'file': f'textures/{name}.png', 'sha256': sha(data), 'width': w,
                                  'height': h, 'source': f'{NS}/textures/{name}.png'}
    model_bytes = write('models/sniper-rifle.shape.json', json.dumps(model, separators=(',', ':')).encode())
    points = [p for m in model['meshes'] for p in m['positions']]
    # The bolt hangs off its node: its points are offset for the bounds.
    bolt_points = [[p[k] + BOLT[k] for k in range(3)] for p in model['meshes'][1]['positions']]
    points = model['meshes'][0]['positions'] + bolt_points
    lo = [round(min(p[k] for p in points), 4) for k in range(3)]
    hi = [round(max(p[k] for p in points), 4) for k in range(3)]
    write('scope/scope.png', scope_overlay())
    write('icons/sniper_rifle.png', icon(model))
    write('icons/sniper_rifle.render.json', dump({
        'schema_version': 1, 'pose_like': 'v20.weapon.rocketlauncheritem',
        'look': {'base': list(ICON_BASE)}}))
    write('sounds/shot.wav', wav(shot()))
    write('sounds/bolt.wav', wav(bolt_sound()))
    weapons_bytes = write('weapons.json', dump(weapons()))
    physics = {'schema_version': 1, 'items': {f'{NS}:weapon/sniperrifle': {'min': lo, 'max': hi}}}
    physics_bytes = write('item-physics.json', dump(physics))
    write('presentation.json', dump({
        'schema_version': 2,
        'id': f'{NS}:item-presentation/main',
        'weapons_sha256': sha(weapons_bytes),
        'item_physics_sha256': sha(physics_bytes),
        'models': {MODEL_KEY: {'file': 'models/sniper-rifle.shape.json', 'sha256': sha(model_bytes),
                               'source': f'{NS}/models/sniper-rifle.shape.json',
                               'source_sha256': sha(model_bytes), 'textures': texture_keys,
                               'bounds_min': lo, 'bounds_max': hi}},
        'textures': texture_resources,
        # The items and images present themselves from weapons.json, which
        # names this model: that way the icon is drawn from the model and
        # the scope finds its picture, as any Add-On's do.
        'items': {}, 'images': {}, 'projectiles': {},
        'diagnostics': [],
    }))


if __name__ == '__main__':
    main()
