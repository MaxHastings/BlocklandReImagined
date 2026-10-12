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

TIRE = dict(static_friction=4.0, kinetic_friction=3.0, lateral_force=16000.0, lateral_damping=3500.0,
            lateral_relaxation=0.02, longitudinal_force=14000.0, longitudinal_damping=1800.0,
            longitudinal_relaxation=0.02)
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
    ]
    seats = [[-0.45, 0.9, 0.25], [0.45, 0.9, 0.25]]
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

    def wheel(x, z, steering, powered):
        return {'position': [x, 0.0, z], 'radius': WHEEL_RADIUS, 'rest_length': 0.35, 'spring': 3000.0,
                'damping': 650.0, 'anti_sway': 8.0, 'tire': TIRE, 'steering': steering, 'powered': powered,
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
             'pose': 'sit', 'controls': i == 0, 'weapon': False}
            for i, at in enumerate(seats)
        ],
        'wheels': [wheel(x, z, 1.0 if z < 0 else 0.0, z > 0)
                   for z in (-1.45, 1.45) for x in (-1.15, 1.15)],
        'weapon': None,
        'attachment_model': None,
        'attachment_collision_hulls': [],
        'attachment_mount': None,
        'attachment_fallback_seat': None,
        'collision_hulls': [hull],
        'bounds_min': [-1.15 - 0.22, -WHEEL_RADIUS, -2.0],
        'bounds_max': [1.15 + 0.22, 2.4, 2.0],
        'mass': 300.0,
        'mass_center': [0.0, 0.2, 0.0],
        'inertia_box': [2.6, 1.6, 4.0],
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
        'engine_force': 12000.0,
        'engine_brake': 1500.0,
        'brake_force': 4000.0,
        'max_speed': 30.0,
        'reverse_speed': 12.0,
        'max_steering': 0.6,
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


def main():
    runner(ROOT)
    carbine(ROOT)
    buggy(ROOT)


if __name__ == '__main__':
    main()
