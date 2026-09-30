"""Write the Steel Ball Add-On's generated art and its vehicle entry.

Outputs, under packages/showcase/steel-ball-kit/assets/:
  models/steel-ball.shape.json  a smooth UV sphere with a bare-steel metal
                                material (the engine's physically based
                                metal: live reflections, GGX highlights)
  textures/steel.png            the steel's tint: faint warm and cool
                                patches of alloy
  textures/steel-detail.png     its fine surface detail, in linear channels:
                                red roughness, green grime, blue and alpha
                                the surface's tilt along u and v, from fine
                                brushing, fingerprints, dull patches, scuffs
                                and scratches; it tiles seamlessly
  vehicles.json                 the ball's vehicle definition, pointing at
                                all three by their SHA-256

Everything here is original and generated; run it again after changing the
numbers below. Only the Python standard library is used, and the output is
the same on every run.
"""
import hashlib
import json
import math
import random
import struct
import zlib
from pathlib import Path

KIT = Path(__file__).resolve().parent.parent / 'packages' / 'showcase' / 'steel-ball-kit' / 'assets'
RADIUS = 1.25
MASS = 900.0
RINGS, SEGMENTS = 32, 64
# Polished steel's reflectance, and how rough the polish is (0 a mirror).
STEEL = [0.62, 0.63, 0.65]
ROUGHNESS = 0.16
# The detail repeats this often round the ball's equator.
DETAIL_REPEATS = 3.0
DETAIL_SIZE = 256


def sphere():
    positions, normals, uv = [], [], []
    for r in range(RINGS + 1):
        theta = math.pi * r / RINGS
        for s in range(SEGMENTS + 1):
            phi = 2 * math.pi * s / SEGMENTS
            n = (math.sin(theta) * math.cos(phi), math.cos(theta), math.sin(theta) * math.sin(phi))
            normals.append([round(c, 6) for c in n])
            positions.append([round(c * RADIUS, 6) for c in n])
            # v runs half as far as u, as the ball is half as long pole to
            # pole as round: the detail keeps square texels.
            uv.append([s / SEGMENTS, 0.5 * r / RINGS])
    triangles = []
    row = SEGMENTS + 1
    for r in range(RINGS):
        for s in range(SEGMENTS):
            a, b = r * row + s, r * row + s + 1
            c, d = a + row, b + row
            # Counter-clockwise seen from outside, as converted shapes are.
            if r != 0:
                triangles.append([a, b, c])
            if r != RINGS - 1:
                triangles.append([b, d, c])
    plain = {'wrap_u': True, 'wrap_v': True, 'blend': 'opaque', 'unlit': False,
             'environment': False, 'mipmaps': True, 'detail_map': None, 'bump_map': None,
             'reflectance_map': None, 'detail_scale': 1.0, 'reflectance': 1.0}
    return {
        'schema_version': 1,
        'id': 'steel-ball-kit:file/models/steel-ball.shape.json',
        'nodes': [{'name': 'main', 'parent': None, 'translation': [0.0, 0.0, 0.0], 'rotation': [0.0, 0.0, 0.0, 1.0]}],
        'objects': [{'name': 'ball', 'node': 0, 'meshes': [0], 'visibility': 1.0, 'frame': 0, 'material_frame': 0}],
        'details': [{'name': 'detail100', 'pixel_threshold': 100.0, 'object_start': 0, 'object_count': 1,
                     'mesh_offset': 0, 'collision': False}],
        'meshes': [{'frame_vertices': len(positions), 'positions': positions, 'normals': normals, 'uv': uv,
                    'primitives': [{'material': 0, 'triangles': triangles}], 'skin': None,
                    'billboard': False, 'billboard_y': False}],
        'materials': [
            dict(name='steel', **plain, metal={
                'color': STEEL, 'roughness': ROUGHNESS, 'detail': 1,
                'detail_scale': DETAIL_REPEATS, 'detail_strength': 1.0}),
            # Only the metal's detail texture: no face uses it.
            dict(name='steel-detail', **plain),
        ],
        'animations': [],
    }


class Noise:
    """Smooth value noise that wraps every `period` cells, so the texture
    it paints tiles."""

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


def fbm(layers, x, y):
    total, weight = 0.0, 0.0
    for i, noise in enumerate(layers):
        scale = 2 ** i
        total += noise.at(x * scale, y * scale) / scale
        weight += 1 / scale
    return total / weight


def detail_maps(size):
    """Per texel: roughness scale, grime and tilt (u, v), all 0..1."""
    rng = random.Random(1337)
    rough = [[0.5] * size for _ in range(size)]
    grime = [[1.0] * size for _ in range(size)]
    height = [[0.0] * size for _ in range(size)]
    patches = [Noise(11 + i, 4 * 2 ** i) for i in range(3)]
    smudges = [Noise(29 + i, 8 * 2 ** i) for i in range(2)]
    for y in range(size):
        # Fine brushing along u: every row its own faint groove depth.
        brush = rng.uniform(-1.0, 1.0)
        for x in range(size):
            u, v = x / size, y / size
            patch = fbm(patches, u * 4, v * 4)
            smudge = fbm(smudges, u * 8, v * 8)
            streak = brush * 0.6 + rng.uniform(-0.4, 0.4)
            # Dull patches where it has been handled; polish elsewhere.
            dull = max(0.0, smudge - 0.58) * 2.2
            rough[y][x] = 0.42 + 0.14 * patch + 0.35 * dull + 0.03 * streak
            grime[y][x] = 1.0 - 0.10 * max(0.0, patch - 0.55) * 2 - 0.05 * dull
            height[y][x] = 0.004 * streak
    # A few fine scuffs: short, shallow grooves that catch the light at a
    # glance without covering the polish.
    for _ in range(36):
        x0, y0 = rng.uniform(0, size), rng.uniform(0, size)
        angle = rng.gauss(0.0, 0.5) if rng.random() < 0.7 else rng.uniform(0, math.pi)
        length = rng.uniform(4, 30) if rng.random() < 0.9 else rng.uniform(30, 80)
        depth = rng.uniform(0.006, 0.02)
        width = rng.uniform(0.4, 0.9)
        steps = int(length * 2)
        for k in range(steps):
            t = k / steps
            fade = math.sin(math.pi * t) ** 0.5
            cx = x0 + math.cos(angle) * length * t
            cy = y0 + math.sin(angle) * length * t
            for dy in range(-2, 3):
                for dx in range(-2, 3):
                    px, py = int(cx) + dx, int(cy) + dy
                    d = math.hypot(px + 0.5 - cx, py + 0.5 - cy)
                    if d > width + 1:
                        continue
                    w = max(0.0, 1 - d / (width + 1)) * fade
                    px, py = px % size, py % size
                    height[py][px] -= depth * w
                    rough[py][px] = max(rough[py][px], 0.5 + 0.2 * w)
                    grime[py][px] = min(grime[py][px], 1.0 - 0.12 * w)
    # Tilt: the slope of the height field, both ways round (it tiles).
    out = []
    for y in range(size):
        for x in range(size):
            du = height[y][(x + 1) % size] - height[y][(x - 1) % size]
            dv = height[(y + 1) % size][x] - height[(y - 1) % size][x]
            out.append((
                min(1.0, max(0.0, rough[y][x])),
                min(1.0, max(0.0, grime[y][x])),
                min(1.0, max(0.0, 0.5 - du * 4)),
                min(1.0, max(0.0, 0.5 - dv * 4)),
            ))
    return out


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


TINT = [Noise(5 + i, 2 * 2 ** i) for i in range(3)]


def steel(x, y):
    # Faint warm and cool patches of alloy over near-white.
    n = fbm(TINT, x / 64 * 2, y / 64 * 2) - 0.5
    return bytes((int(246 + 14 * n), int(247 + 6 * n), int(250 - 10 * n)))


def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return hashlib.sha256(data).hexdigest()


def main():
    model_bytes = (json.dumps(sphere(), separators=(',', ':')) + '\n').encode()
    model_sha = write(KIT / 'models' / 'steel-ball.shape.json', model_bytes)
    texture_sha = write(KIT / 'textures' / 'steel.png', png(64, 64, steel))
    texels = detail_maps(DETAIL_SIZE)
    detail = png(DETAIL_SIZE, DETAIL_SIZE,
                 lambda x, y: bytes(round(c * 255) for c in texels[y * DETAIL_SIZE + x]), channels=4)
    detail_sha = write(KIT / 'textures' / 'steel-detail.png', detail)
    half = RADIUS
    # The inertia of a solid sphere, as the cube Torque-style mass boxes use.
    box = round(math.sqrt(0.4 * RADIUS * RADIUS * 6), 4)
    octahedron = [[half, 0, 0], [-half, 0, 0], [0, half, 0], [0, -half, 0], [0, 0, half], [0, 0, -half]]
    definition = {
        'id': 'steel-ball-kit:vehicle/steelball',
        'datablock': 'SteelBallVehicle',
        'name': 'Steel Ball',
        'family': 'Ball',
        'energy': {'maximum': 0.0, 'minimum_jet': 0.0, 'drain_per_32ms': 0.0, 'recharge_per_32ms': 0.0, 'jet_force': 0.0},
        'flight': None,
        'model': 'models/steel-ball.shape.json',
        'seats': [], 'wheels': [], 'weapon': None,
        'attachment_model': None, 'attachment_collision_hulls': [], 'attachment_mount': None,
        'attachment_fallback_seat': None,
        'collision_hulls': [octahedron],
        'bounds_min': [-half, -half, -half], 'bounds_max': [half, half, half],
        'mass': MASS, 'mass_center': [0.0, 0.0, 0.0], 'inertia_box': [box, box, box],
        'density': 8.0, 'drag': 0.4, 'friction': 0.8, 'restitution': 0.2,
        'max_damage': 999999.0, 'burn_ticks': 0, 'invulnerable_ticks': 0,
        'initial_explosion': None, 'final_explosion': None,
        'initial_explosion_offset': 0.0, 'final_explosion_offset': 0.0,
        'mount_distance': 0.0, 'engine_force': 0.0, 'engine_brake': 0.0, 'brake_force': 0.0,
        'max_speed': 0.0, 'reverse_speed': 0.0, 'max_steering': 0.0, 'thrust': 0.0, 'reverse_thrust': 0.0,
        'lift': 0.0, 'yaw_force': 0.0, 'pitch_force': 0.0, 'roll_force': 0.0, 'angular_drag': 0.3,
        'jump_speed': 0.0, 'max_side_speed': 0.0, 'run_surface_angle': 0.0,
        'impact_threshold': 0.0, 'impact_damage': 0.0, 'strafe_steering': False,
        'look_pitch': [-1.5707964, 1.5707964], 'underwater_speeds': [0.0, 0.0, 0.0],
        'camera': {'max_dist': 13.0, 'offset': 7.5, 'tilt': 0.4, 'lag': 0.0, 'decay': 0.75},
        'look_limits': [0.0, 1.0],
        # Run-overs count from 14 u/s (12, plus 2 with no driver): a roll
        # (11) only bumps, a hurl (24) does 120 and kills.
        'runover_speed': 12.0, 'runover_damage': 5.0, 'runover_push': 1.0,
        'protect_direct': False, 'protect_radius': False, 'protect_burn': False,
        # Smashing starts at 14 u/s into a surface. Each brick costs its
        # volume x 600 of the hit's kinetic energy (half mass x speed
        # squared), so a hurl (24 u/s, enough for about 430 studs x studs x
        # plates) punches through a one-brick wall and rolls on at about 20,
        # and a thick bunker stops it. A vehicle struck takes the square of
        # (speed - 14) / (26 - 14) of its health: a hurl takes 70%, a drop
        # or a hard throw at 26 or more wrecks it.
        'smash': {'speed': 14.0, 'radius': 1.3, 'max_volume': 64.0, 'force': 15.0,
                  'energy_per_volume': 600.0, 'wreck_speed': 26.0},
        'shove': True,
        'harms_only_in_minigames': True,
        'authored': {'category': 'Vehicles', 'uiname': 'Steel Ball'},
        'adaptations': [],
    }
    pack = {
        'schema_version': 7,
        'definitions': [definition],
        'assets': [
            {'virtual_path': 'Add-Ons/Vehicle_SteelBall/steel-ball.dts', 'path': 'models/steel-ball.shape.json',
             'sha256': model_sha, 'kind': 'model', 'source_sha256': model_sha},
            {'virtual_path': 'Add-Ons/Vehicle_SteelBall/steel.png', 'path': 'textures/steel.png',
             'sha256': texture_sha, 'kind': 'texture', 'source_sha256': texture_sha},
            {'virtual_path': 'Add-Ons/Vehicle_SteelBall/steel-detail.png', 'path': 'textures/steel-detail.png',
             'sha256': detail_sha, 'kind': 'texture', 'source_sha256': detail_sha},
        ],
        'evidence': [], 'unresolved': [], 'animation_aliases': {},
    }
    (KIT / 'vehicles.json').write_text(json.dumps(pack, indent=2) + '\n', encoding='utf-8', newline='\n')
    print(f'model {model_sha}\ntexture {texture_sha}\ndetail {detail_sha}')


if __name__ == '__main__':
    main()
