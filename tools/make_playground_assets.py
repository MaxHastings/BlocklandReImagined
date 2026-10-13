"""Write the Mixed Add-On Playground's generated art, sounds and data.

The Playground (packages/playground/) is several Add-Ons written as if by
different people: a character, a weapon, a vehicle and a game mode that
names them. None of the first three knows the others exist. This script
writes their generated files:

  playground-runner-look/models/runner.json   the Runner's box body
  playground-rifle/assets/models/carbine.shape.json
                                              the Pulse Carbine, with its
                                              mountPoint, muzzlePoint and
                                              ejectPoint nodes
  playground-rifle/assets/{carbine,pulse}.png its paint
  playground-rifle/assets/sounds/*.wav        shot, equip, reload, empty
  playground-buggy/assets/models/*.shape.json the Dune Buggy's chassis and tire
  playground-buggy/assets/textures/*.png      their paint
  playground-buggy/assets/vehicles.json       the buggy's vehicle entry,
                                              pointing at its files by SHA-256
  playground-runner-hud/textures/*.png        the Runner's HUD: a power meter
                                              strip, a sprite font, a cap
                                              icon and a reticle
  playground-stomper-look/models/stomper.json the Stomper's box body

Everything here is original and generated; run it again after changing the
numbers below. Only the Python standard library is used, and the output is
the same on every run.
"""
import hashlib
import json
import math
import random
import struct
import wave
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / 'packages' / 'playground'
RATE = 22050


# --- files -----------------------------------------------------------------

def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return hashlib.sha256(data).hexdigest()


def write_json(path, value, compact=False):
    text = json.dumps(value, separators=(',', ':')) if compact else json.dumps(value, indent=1)
    return write(path, (text + '\n').encode('utf-8'))


def png(width, height, pixel):
    raw = bytearray()
    for y in range(height):
        raw.append(0)
        for x in range(width):
            raw.extend(pixel(x, y))

    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data) & 0xFFFFFFFF)
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(bytes(raw), 9)) + chunk(b'IEND', b''))


def png_rgba(width, height, pixel):
    """A PNG with alpha: `pixel(x, y)` gives four bytes."""
    raw = bytearray()
    for y in range(height):
        raw.append(0)
        for x in range(width):
            raw.extend(pixel(x, y))

    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data) & 0xFFFFFFFF)
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(bytes(raw), 9)) + chunk(b'IEND', b''))


def flat(rgb, grain=6, seed=1):
    """A 16 px swatch of `rgb` with a little fixed grain, so it reads as paint."""
    rng = random.Random(seed)
    cells = [[rng.randint(-grain, grain) for _ in range(16)] for _ in range(16)]
    return png(16, 16, lambda x, y: bytes(max(0, min(255, c + cells[y][x])) for c in rgb))


# --- box meshes --------------------------------------------------------------

# Each face: its normal and the two axes spanning it, wound counter-clockwise
# seen from outside, as converted shapes are.
FACES = [
    ((1, 0, 0), (0, 0, -1), (0, 1, 0)),
    ((-1, 0, 0), (0, 0, 1), (0, 1, 0)),
    ((0, 1, 0), (1, 0, 0), (0, 0, -1)),
    ((0, -1, 0), (1, 0, 0), (0, 0, 1)),
    ((0, 0, 1), (1, 0, 0), (0, 1, 0)),
    ((0, 0, -1), (-1, 0, 0), (0, 1, 0)),
]


def box(center, size):
    """Positions, normals, uvs and triangles of one axis-aligned box."""
    half = [s / 2 for s in size]
    positions, normals, uv, triangles = [], [], [], []
    for n, u, v in FACES:
        base = len(positions)
        for su, sv in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
            p = [center[i] + half[i] * (n[i] + su * u[i] + sv * v[i]) for i in range(3)]
            positions.append([round(c, 5) for c in p])
            normals.append(list(n))
            uv.append([(su + 1) / 2, (sv + 1) / 2])
        triangles += [[base, base + 1, base + 2], [base, base + 2, base + 3]]
    return positions, normals, uv, triangles


def prism(center, radius, width, sides):
    """A wheel: a `sides`-sided prism with its axle along X."""
    positions, normals, uv, triangles = [], [], [], []
    cx, cy, cz = center
    ring = [(math.cos(2 * math.pi * (i + 0.5) / sides), math.sin(2 * math.pi * (i + 0.5) / sides))
            for i in range(sides)]
    for i in range(sides):
        (y0, z0), (y1, z1) = ring[i], ring[(i + 1) % sides]
        mid = ((y0 + y1) / 2, (z0 + z1) / 2)
        length = math.hypot(*mid)
        n = [0.0, round(mid[0] / length, 5), round(mid[1] / length, 5)]
        base = len(positions)
        for x, (y, z) in ((-1, (y0, z0)), (1, (y0, z0)), (1, (y1, z1)), (-1, (y1, z1))):
            positions.append([round(cx + x * width / 2, 5), round(cy + y * radius, 5), round(cz + z * radius, 5)])
            normals.append(n)
            uv.append([(x + 1) / 2, i / sides])
        triangles += [[base, base + 2, base + 1], [base, base + 3, base + 2]]
    for side in (-1, 1):
        base = len(positions)
        positions.append([cx + side * width / 2, cy, cz])
        normals.append([side, 0, 0])
        uv.append([0.5, 0.5])
        for y, z in ring:
            positions.append([round(cx + side * width / 2, 5), round(cy + y * radius, 5), round(cz + z * radius, 5)])
            normals.append([side, 0, 0])
            uv.append([0.5 + y / 2, 0.5 + z / 2])
        for i in range(sides):
            a, b = base + 1 + i, base + 1 + (i + 1) % sides
            triangles.append([base, a, b] if side > 0 else [base, b, a])
    return positions, normals, uv, triangles


def mesh(parts):
    """One mesh from `(material index, (positions, normals, uv, triangles))`."""
    positions, normals, uv, by_material = [], [], [], {}
    for material, (p, n, t, tris) in parts:
        base = len(positions)
        positions += p
        normals += n
        uv += t
        by_material.setdefault(material, []).extend([[base + i for i in tri] for tri in tris])
    return {
        'frame_vertices': len(positions), 'positions': positions, 'normals': normals, 'uv': uv,
        'primitives': [{'material': m, 'triangles': tris} for m, tris in sorted(by_material.items())],
        'skin': None, 'billboard': False, 'billboard_y': False,
    }


def material(name):
    return {'name': name, 'wrap_u': True, 'wrap_v': True, 'blend': 'opaque', 'unlit': False,
            'environment': False, 'mipmaps': True, 'detail_map': None, 'bump_map': None,
            'reflectance_map': None, 'detail_scale': 1.0, 'reflectance': 0.0}


def node(name, parent, at):
    return {'name': name, 'parent': parent, 'translation': list(at), 'rotation': [0.0, 0.0, 0.0, 1.0]}


def shape(shape_id, nodes, materials, parts):
    return {
        'schema_version': 1,
        'id': shape_id,
        'nodes': nodes,
        'objects': [{'name': 'body', 'node': 0, 'meshes': [0], 'visibility': 1.0, 'frame': 0, 'material_frame': 0}],
        'details': [{'name': 'detail1', 'pixel_threshold': 0.0, 'object_start': 0, 'object_count': 1,
                     'mesh_offset': 0, 'collision': False}],
        'meshes': [mesh(parts)],
        'materials': [material(m) for m in materials],
        'animations': [],
    }


# --- sounds ------------------------------------------------------------------

rng = random.Random(20261012)


def noise(seconds):
    return [rng.uniform(-1, 1) for _ in range(int(seconds * RATE))]


def tone(seconds, f0, f1):
    out, phase, n = [], 0.0, int(seconds * RATE)
    for i in range(n):
        phase += 2 * math.pi * (f0 + (f1 - f0) * i / n) / RATE
        out.append(math.sin(phase))
    return out


def decay(samples, seconds):
    return [x * math.exp(-i / RATE / seconds) for i, x in enumerate(samples)]


def mix(*layers):
    out = [0.0] * max(len(l) for l in layers)
    for layer in layers:
        for i, x in enumerate(layer):
            out[i] += x
    return out


def wav(samples, gain):
    peak = max(abs(x) for x in samples) or 1.0
    frames = b''.join(struct.pack('<h', int(32767 * gain * x / peak)) for x in samples)
    import io
    buffer = io.BytesIO()
    with wave.open(buffer, 'wb') as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(frames)
    return buffer.getvalue()


def sounds(out):
    # A rising electric zap over a short thump: the Pulse Carbine firing.
    shot = mix(decay(tone(0.22, 1400, 380), 0.06), decay(tone(0.25, 120, 60), 0.08),
               [0.25 * x for x in decay(noise(0.2), 0.03)])
    write(out / 'shot.wav', wav(shot, 0.8))
    # Two quick rising chirps: raising it.
    equip = tone(0.06, 600, 900) + [0.0] * 600 + tone(0.06, 800, 1200)
    write(out / 'equip.wav', wav(decay(equip, 0.2), 0.5))
    # A falling whine then a clack: a new cell going in.
    reload = decay(tone(0.35, 900, 300), 0.25) + [0.0] * 2000 + decay(noise(0.05), 0.01)
    write(out / 'reload.wav', wav(reload, 0.6))
    # A dry tick: out of charge.
    write(out / 'empty.wav', wav(decay(noise(0.04), 0.006), 0.5))


# --- the Runner --------------------------------------------------------------

def runner(root):
    """A short, round-headed runner in a red cap and blue overalls: an
    original body 1.9 units tall, under the Blockhead's 2.65."""
    red, blue, skin, brown, white = [0.82, 0.12, 0.1, 1], [0.15, 0.25, 0.75, 1], [0.98, 0.8, 0.62, 1], \
        [0.35, 0.2, 0.1, 1], [0.95, 0.95, 0.95, 1]
    boxes = [
        ([-0.2, 0.08, -0.05], [0.3, 0.16, 0.45], brown),   # boots
        ([0.2, 0.08, -0.05], [0.3, 0.16, 0.45], brown),
        ([-0.18, 0.42, 0.0], [0.28, 0.52, 0.32], blue),    # legs
        ([0.18, 0.42, 0.0], [0.28, 0.52, 0.32], blue),
        ([0.0, 0.95, 0.0], [0.78, 0.6, 0.5], blue),        # overalls
        ([0.0, 1.12, 0.0], [0.8, 0.22, 0.52], red),        # shirt
        ([-0.5, 1.0, 0.0], [0.22, 0.5, 0.26], red),        # arms
        ([0.5, 1.0, 0.0], [0.22, 0.5, 0.26], red),
        ([-0.5, 0.7, 0.0], [0.24, 0.2, 0.26], white),      # gloves
        ([0.5, 0.7, 0.0], [0.24, 0.2, 0.26], white),
        ([0.0, 1.5, 0.0], [0.62, 0.56, 0.56], skin),       # head
        ([0.0, 1.45, -0.32], [0.18, 0.16, 0.12], skin),    # nose
        ([0.0, 1.36, -0.29], [0.4, 0.08, 0.04], brown),    # moustache
        ([0.0, 1.82, 0.02], [0.68, 0.16, 0.62], red),      # cap
        ([0.0, 1.76, -0.36], [0.6, 0.06, 0.24], red),      # peak
    ]
    model = {'schema_version': 1, 'boxes': [
        {'center': c, 'size': s, 'color': col} for c, s, col in boxes]}
    write_json(root / 'playground-runner-look' / 'models' / 'runner.json', model)


# --- the Stomper -------------------------------------------------------------

def stomper(root):
    """A squat walking mushroom with a broad brown cap, a pale face, angry
    brows and two dark feet: an original body 1.2 units tall."""
    cap, face, dark, white = [0.55, 0.3, 0.12, 1], [0.95, 0.85, 0.68, 1], [0.15, 0.08, 0.05, 1], \
        [0.97, 0.97, 0.97, 1]
    boxes = [
        ([-0.3, 0.1, -0.05], [0.42, 0.2, 0.55], dark),     # feet
        ([0.3, 0.1, -0.05], [0.42, 0.2, 0.55], dark),
        ([0.0, 0.42, 0.0], [0.7, 0.46, 0.62], face),       # stem
        ([0.0, 0.88, 0.0], [1.2, 0.5, 1.1], cap),          # cap
        ([0.0, 1.13, 0.0], [0.9, 0.1, 0.8], cap),          # its crown
        ([-0.17, 0.52, -0.32], [0.14, 0.22, 0.04], white),  # eyes
        ([0.17, 0.52, -0.32], [0.14, 0.22, 0.04], white),
        ([-0.15, 0.5, -0.345], [0.07, 0.12, 0.02], dark),  # pupils
        ([0.15, 0.5, -0.345], [0.07, 0.12, 0.02], dark),
        ([-0.2, 0.68, -0.33], [0.2, 0.05, 0.04], dark),    # brows
        ([0.2, 0.68, -0.33], [0.2, 0.05, 0.04], dark),
    ]
    model = {'schema_version': 1, 'boxes': [
        {'center': c, 'size': s, 'color': col} for c, s, col in boxes]}
    write_json(root / 'playground-stomper-look' / 'models' / 'stomper.json', model)


# --- the Pulse Carbine -------------------------------------------------------

def carbine(root):
    assets = root / 'playground-rifle' / 'assets'
    # Held at the grip (the origin, where the hand closes), barrel along -Z.
    body, glow = 0, 1
    parts = [
        (body, box([0.0, 0.0, 0.05], [0.08, 0.22, 0.12])),     # grip
        (body, box([0.0, 0.14, -0.15], [0.12, 0.14, 0.75])),   # receiver
        (body, box([0.0, 0.12, 0.38], [0.1, 0.16, 0.3])),      # stock
        (body, box([0.0, 0.14, -0.68], [0.07, 0.07, 0.36])),   # barrel
        (glow, box([0.0, 0.23, -0.2], [0.05, 0.04, 0.4])),     # charge strip
        (glow, box([0.0, 0.05, -0.18], [0.07, 0.16, 0.12])),   # cell
    ]
    nodes = [node('root', None, [0, 0, 0]), node('mountPoint', 0, [0, 0, 0]),
             node('muzzlePoint', 0, [0.0, 0.14, -0.88]), node('ejectPoint', 0, [0.07, 0.17, -0.1])]
    write_json(assets / 'models' / 'carbine.shape.json',
               shape('playground-rifle:file/models/carbine.shape.json', nodes, ['carbine', 'pulse'], parts),
               compact=True)
    write(assets / 'models' / 'carbine.png', flat([70, 78, 88], seed=2))
    write(assets / 'models' / 'pulse.png', flat([60, 220, 255], grain=2, seed=3))
    sounds(assets / 'sounds')


# --- the Dune Buggy ----------------------------------------------------------

# The buggy is the Playground's stand-in for Halo's Warthog, tuned from the
# Warthog's own tags (Halo PC `bloodgulch.map`, 4.12 units per Halo world
# unit; the numbers are in docs/audits/vehicle-handling.md): top speed 34
# forward and 14.8 back, reached at a steady pull in about 3.3 s, a hard
# stop in under a second, 30 degree lock, four-wheel drive, 0.62 of
# suspension travel, the weight a little forward, and per-axle grip along
# and across the tyre (front 0.75 / 0.45, rear 0.45 / 0.65, times 4 for this
# tyre model). Halo has no springs, engine torque or gravity in its tags;
# those, the rear steering and the look steering are this buggy's own.
# `crates/vehicles/tests/handling.rs` measures each.
TIRE = dict(lateral_force=42000.0, lateral_damping=9000.0, lateral_relaxation=0.02,
            longitudinal_force=40000.0, longitudinal_damping=5000.0, longitudinal_relaxation=0.02)
GRIP_SCALE = 4.0
# (along, across) per axle, from the Warthog's mass points 11 to 14.
FRONT_GRIP = (0.75, 0.45)
REAR_GRIP = (0.45, 0.65)
WHEEL_RADIUS = 0.55


def buggy(root):
    assets = root / 'playground-buggy' / 'assets'
    paint, frame, seat = 0, 1, 2
    # Facing -Z, the floor 0.3 above the chassis origin, two seats side by
    # side in an open roll cage.
    parts = [
        (frame, box([0.0, 0.05, 0.0], [1.6, 0.3, 3.6])),        # skid plate
        (paint, box([0.0, 0.35, 0.0], [2.0, 0.3, 4.0])),        # floor pan
        (paint, box([0.0, 0.6, -1.65], [1.9, 0.3, 0.7])),       # nose
        (paint, box([0.0, 0.65, 1.7], [1.9, 0.4, 0.6])),        # engine deck
        (seat, box([-0.45, 0.7, 0.25], [0.65, 0.4, 0.6])),      # seats
        (seat, box([0.45, 0.7, 0.25], [0.65, 0.4, 0.6])),
        (seat, box([-0.45, 1.05, 0.55], [0.65, 0.6, 0.15])),
        (seat, box([0.45, 1.05, 0.55], [0.65, 0.6, 0.15])),
        (frame, box([-0.9, 1.45, 0.65], [0.1, 1.9, 0.1])),      # roll cage
        (frame, box([0.9, 1.45, 0.65], [0.1, 1.9, 0.1])),
        (frame, box([0.0, 2.35, 0.65], [1.9, 0.1, 0.1])),
        (frame, box([-0.9, 2.35, -0.2], [0.1, 0.1, 1.8])),
        (frame, box([0.9, 2.35, -0.2], [0.1, 0.1, 1.8])),
        (frame, box([-0.9, 1.55, -1.1], [0.1, 1.6, 0.1])),
        (frame, box([0.9, 1.55, -1.1], [0.1, 1.6, 0.1])),
        (frame, box([0.0, 1.0, -0.6], [0.08, 0.5, 0.08])),      # steering column
        (frame, box([0.0, 1.25, -0.5], [0.4, 0.06, 0.06])),
        (frame, box([0.0, 1.45, 1.5], [0.12, 0.6, 0.12])),      # gun post
        (frame, box([0.0, 1.85, 1.35], [0.22, 0.2, 0.5])),      # gun
        (frame, box([0.0, 1.85, 0.9], [0.08, 0.08, 0.5])),      # barrel
    ]
    # Driver and passenger side by side; a gunner stands on the engine
    # deck behind them at the post.
    seats = [[-0.45, 0.9, 0.25], [0.45, 0.9, 0.25], [0.0, 0.85, 1.75]]
    nodes = [node('root', None, [0, 0, 0])] + [node(f'mount{i}', 0, at) for i, at in enumerate(seats)]
    chassis = write_json(assets / 'models' / 'buggy.shape.json',
                         shape('playground-buggy:file/models/buggy.shape.json', nodes,
                               ['buggy_paint', 'buggy_frame', 'buggy_seat'], parts), compact=True)
    tire = write_json(assets / 'models' / 'tire.shape.json',
                      shape('playground-buggy:file/models/tire.shape.json', [node('root', None, [0, 0, 0])],
                            ['buggy_tire'], [(0, prism([0, 0, 0], WHEEL_RADIUS, 0.45, 12))]), compact=True)
    textures = {
        'buggy_paint': write(assets / 'textures' / 'buggy_paint.png', flat([235, 150, 30], seed=4)),
        'buggy_frame': write(assets / 'textures' / 'buggy_frame.png', flat([40, 42, 46], seed=5)),
        'buggy_seat': write(assets / 'textures' / 'buggy_seat.png', flat([25, 25, 25], seed=6)),
        'buggy_tire': write(assets / 'textures' / 'buggy_tire.png', flat([18, 18, 18], seed=7)),
    }

    def wheel(x, z):
        rear = z > 0
        along, across = REAR_GRIP if rear else FRONT_GRIP
        tire = dict(static_friction=along * GRIP_SCALE, kinetic_friction=along * GRIP_SCALE * 0.75,
                    lateral_grip=round(across / along, 4), **TIRE)
        # A quarter of the weight holds each spring near half its travel.
        # The hubs mount high in the wheel wells, so the long travel hangs
        # below and the deck stays low enough to jump onto.
        # The springs share the forward weight (57% front) so the deck sits
        # level, and the hubs mount high enough that a dipping nose clears
        # the ground through the Warthog's shorter travel.
        return {'position': [x, 0.43, z], 'radius': WHEEL_RADIUS, 'rest_length': 0.62,
                'spring': 7700.0 if rear else 10300.0,
                'damping': 2600.0, 'rebound_damping': 2400.0, 'anti_sway': 6000.0, 'tire': tire,
                'steering': -0.35 if rear else 1.0, 'powered': True,
                'model': 'models/tire.shape.json', 'model_rotation': [0.0, 0.0, 0.0, 1.0]}

    hull = [[x, y, z] for x in (-1.0, 1.0) for y in (-0.1, 1.2) for z in (-2.0, 2.0)]
    definition = {
        'id': 'playground-buggy:vehicle/buggy',
        'datablock': 'PlaygroundBuggyVehicle',
        'name': 'Dune Buggy',
        'family': 'Wheeled',
        'energy': {'maximum': 0.0, 'minimum_jet': 0.0, 'drain_per_32ms': 0.0, 'recharge_per_32ms': 0.0,
                   'jet_force': 0.0},
        'flight': None,
        'model': 'models/buggy.shape.json',
        'seats': [
            {'node': f'mount{i}', 'transform': {'position': at, 'rotation': [0.0, 0.0, 0.0, 1.0]},
             'pose': 'sit' if i < 2 else 'root', 'controls': i == 0, 'weapon': i == 2}
            for i, at in enumerate(seats)
        ],
        'wheels': [wheel(x, z) for z in (-1.45, 1.45) for x in (-1.15, 1.15)],
        # The gunner's post fires bolts where they look, turning about the
        # post's top.
        'weapon': {'projectile': 'playground-buggy:projectile/bolt',
                   'muzzle': {'position': [0.0, 1.85, 0.6], 'rotation': [0.0, 0.0, 0.0, 1.0]},
                   'pivot': [0.0, 1.85, 1.5], 'cooldown_ticks': 12, 'speed': 140.0,
                   'charge_ticks': 0, 'charge_steps': 1, 'sound': 'playground-buggy:turret',
                   'effect': ''},
        'attachment_model': None,
        'attachment_collision_hulls': [],
        'attachment_mount': None,
        'attachment_fallback_seat': None,
        'collision_hulls': [hull],
        'bounds_min': [-1.15 - 0.22, -WHEEL_RADIUS, -2.0],
        'bounds_max': [1.15 + 0.22, 2.4, 2.0],
        'mass': 900.0,
        # 57% of the weight on the front axle, as the Warthog's centre of
        # mass sits between its axles.
        'mass_center': [0.0, 0.05, -0.2],
        'inertia_box': [2.8, 1.6, 4.4],
        'density': 4.0,
        'drag': 0.4,
        'friction': 0.8,
        'restitution': 0.2,
        'max_damage': 150.0,
        'burn_ticks': 240,
        'invulnerable_ticks': 60,
        'initial_explosion': None,
        'final_explosion': None,
        'initial_explosion_offset': 0.0,
        'final_explosion_offset': 0.0,
        'mount_distance': 3.0,
        'engine_force': 5400.0,
        'engine_curve': [[0.0, 1.0], [0.9, 1.0], [1.0, 0.0]],
        'engine_brake': 4500.0,
        'brake_force': 30000.0,
        'max_speed': 34.0,
        'reverse_speed': 14.8,
        'max_steering': 0.5236,
        'thrust': 0.0,
        'reverse_thrust': 0.0,
        'lift': 0.0,
        'yaw_force': 0.0,
        'pitch_force': 0.0,
        'roll_force': 0.0,
        'angular_drag': 0.6,
        'jump_speed': 0.0,
        'max_side_speed': 0.0,
        'run_surface_angle': 0.0,
        'impact_threshold': 15.0,
        'impact_damage': 0.05,
        'strafe_steering': False,
        'throttle_brakes': True,
        'reverse_limited': True,
        'steering': {'strafe_rate': 0.25, 'auto_return': True, 'auto_return_rate': 0.9,
                     'auto_return_max_speed': 10.0, 'toward_look': True, 'speed_lock': 0.6},
        'gravity_scale': 0.85,
        'look_pitch': [-1.5, 1.5],
        'underwater_speeds': [5.0, 3.0, 3.0],
        'camera': {'max_dist': 9.0, 'offset': 2.5, 'tilt': 0.1, 'lag': 0.1, 'decay': 0.5},
        'look_limits': [0.0, 1.0],
        'wheeled_flight': None,
        'runover_speed': 6.0,
        'runover_damage': 25.0,
        'runover_push': 6.0,
        'protect_direct': False,
        'protect_radius': False,
        'protect_burn': False,
        # Knocks bricks out it hits at speed, under the mini-game's brick
        # damage rule, as a rocket's hit does.
        'smash': {'speed': 12.0, 'radius': 1.2, 'max_volume': 8.0, 'force': 10.0},
        'harms_only_in_minigames': True,
        'trails': [],
        'authored': {},
        'adaptations': [],
    }
    base = 'Add-Ons/Vehicle_PlaygroundBuggy/'
    pack = {
        'schema_version': 7,
        'definitions': [definition],
        'assets': [
            {'virtual_path': base + 'buggy.dts', 'path': 'models/buggy.shape.json', 'sha256': chassis,
             'kind': 'model', 'source_sha256': chassis},
            {'virtual_path': base + 'tire.dts', 'path': 'models/tire.shape.json', 'sha256': tire,
             'kind': 'model', 'source_sha256': tire},
        ] + [
            {'virtual_path': base + f'{name}.png', 'path': f'textures/{name}.png', 'sha256': sha,
             'kind': 'texture', 'source_sha256': sha}
            for name, sha in textures.items()
        ],
        'evidence': [],
        'unresolved': [],
        'animation_aliases': {},
    }
    write_json(assets / 'vehicles.json', pack)
    # The gun's bolt and sound, in the buggy's own weapons file: a vehicle
    # brings what it fires.
    bolt = mix(decay(tone(0.12, 900, 250), 0.04), [0.3 * x for x in decay(noise(0.1), 0.02)])
    write(assets / 'sounds' / 'turret.wav', wav(bolt, 0.8))
    write_json(assets / 'weapons.json', {
        'schema_version': 5,
        'id': 'playground-buggy',
        'items': {},
        'images': {},
        'projectiles': {
            'playground-buggy:projectile/bolt': {
                'speed': 140.0, 'lifetime_ticks': 180, 'fade_ticks': 180, 'damage': 20.0,
                'damage_type': '$DamageType::BuggyGun', 'impulse': 150.0,
            },
        },
        'damage_types': {
            'buggygun': {'name': 'BuggyGun', 'suicide_message': '%1 shot themselves',
                         'murder_message': '%2 gunned down %1', 'vehicle_scale': 1.0, 'direct': True},
        },
        'sounds': {'playground-buggy:turret': {'file': 'sounds/turret.wav'}},
    })


# --- the Runner's HUD ---------------------------------------------------------

CLEAR = bytes([0, 0, 0, 0])
# Seven-segment digits, then `x` and `*`: which of a b c d e f g each lights.
SEGMENTS = {
    '0': 'abcdef', '1': 'bc', '2': 'abdeg', '3': 'abcdg', '4': 'bcfg',
    '5': 'acdfg', '6': 'acdefg', '7': 'abc', '8': 'abcdefg', '9': 'abcdfg',
}
GLYPHS = '0123456789x*'


def power_meter(frame, size=64):
    """One frame of the power meter: a gold-rimmed disc of eight wedges,
    `frame` of them lit (blue when healthy, yellow, then red)."""
    lit = (70, 150, 255) if frame >= 6 else (255, 210, 40) if frame >= 3 else (240, 60, 50)
    half = size / 2

    def pixel(x, y):
        dx, dy = x + 0.5 - half, y + 0.5 - half
        r = math.hypot(dx, dy) / half
        if r > 1.0:
            return CLEAR
        if r > 0.86:
            return bytes([230, 180, 40, 255])
        if r < 0.22:
            return bytes([250, 250, 240, 255])
        # Wedges clockwise from the top, a dark seam between each.
        turn = (math.atan2(dx, -dy) / math.tau) % 1.0
        wedge = int(turn * 8)
        if (turn * 8) % 1.0 < 0.06:
            return bytes([20, 20, 30, 255])
        rgb = lit if wedge < frame else (45, 45, 60)
        return bytes([*rgb, 255])
    return pixel


def glyph(char, size=16):
    """A white seven-segment character with a dark outline."""
    def on(x, y):
        if char == 'x':
            return abs((x - 7.5) - (y - 7.5)) < 1.6 or abs((x - 7.5) + (y - 7.5)) < 1.6 if 3 <= x <= 12 and 4 <= y <= 13 else False
        if char == '*':
            return (abs(x - 7.5) < 1.6 and 2 <= y <= 13) or (abs(y - 7.5) < 1.6 and 2 <= x <= 13) \
                or (abs(abs(x - 7.5) - abs(y - 7.5)) < 1.2 and 4 <= x <= 11 and 4 <= y <= 11)
        segs = SEGMENTS[char]
        left, right, top, mid, bottom = 3, 12, 1, 7.5, 14
        bar = 1.6
        hits = {
            'a': abs(y - top - 0.5) < bar and left <= x <= right,
            'g': abs(y - mid) < bar and left <= x <= right,
            'd': abs(y - bottom + 0.5) < bar and left <= x <= right,
            'f': abs(x - left - 0.5) < bar and top <= y <= mid,
            'b': abs(x - right + 0.5) < bar and top <= y <= mid,
            'e': abs(x - left - 0.5) < bar and mid <= y <= bottom,
            'c': abs(x - right + 0.5) < bar and mid <= y <= bottom,
        }
        return any(hits[s] for s in segs)

    def pixel(x, y):
        if on(x, y):
            return bytes([255, 255, 255, 255])
        near = any(on(x + dx, y + dy) for dx in (-1, 0, 1) for dy in (-1, 0, 1))
        return bytes([20, 20, 40, 255]) if near else CLEAR
    return pixel


def strip(frames, width, height):
    """Frames side by side, left to right."""
    return png_rgba(width * len(frames), height, lambda x, y: frames[x // width](x % width, y))


def cap_icon(size=32):
    """The Runner's red cap seen from the front, on a gold disc."""
    half = size / 2

    def pixel(x, y):
        dx, dy = x + 0.5 - half, y + 0.5 - half
        r = math.hypot(dx, dy) / half
        if r > 1.0:
            return CLEAR
        if r > 0.85:
            return bytes([230, 180, 40, 255])
        if dy < -2 and abs(dx) < 10:
            return bytes([210, 30, 25, 255])
        if -2 <= dy < 2 and abs(dx) < 13:
            return bytes([170, 20, 18, 255])
        if dy >= 2 and abs(dx) < 9:
            return bytes([250, 205, 160, 255])
        return bytes([30, 40, 90, 255])
    return png_rgba(size, size, pixel)


def reticle(size=32):
    """A gold ring with four ticks, clear in the middle."""
    half = size / 2

    def pixel(x, y):
        dx, dy = x + 0.5 - half, y + 0.5 - half
        r = math.hypot(dx, dy)
        ring = abs(r - 11) < 1.3
        tick = (abs(dx) < 1.0 and 4 < abs(dy) < 8) or (abs(dy) < 1.0 and 4 < abs(dx) < 8)
        if ring or tick:
            return bytes([255, 215, 60, 255])
        if abs(r - 11) < 2.3:
            return bytes([30, 20, 0, 200])
        return CLEAR
    return png_rgba(size, size, pixel)


def runner_hud(root):
    out = root / 'playground-runner-hud' / 'textures'
    write(out / 'power.png', strip([power_meter(f) for f in range(9)], 64, 64))
    write(out / 'digits.png', strip([glyph(c) for c in GLYPHS], 16, 16))
    write(out / 'cap.png', cap_icon())
    write(out / 'reticle.png', reticle())


def main():
    runner(ROOT)
    carbine(ROOT)
    buggy(ROOT)
    runner_hud(ROOT)
    stomper(ROOT)


if __name__ == '__main__':
    main()
