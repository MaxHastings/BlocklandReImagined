"""Write the Steel Ball Add-On's generated art and its vehicle entry.

Outputs, under packages/showcase/steel-ball-kit/assets/:
  models/steel-ball.shape.json  a UV sphere, the ball's plain model (what
                                players see without the steel-ball-fx code)
  textures/steel.png            a brushed-steel paint for that model
  vehicles.json                 the ball's vehicle definition, pointing at
                                both by their SHA-256

Everything here is original and generated; run it again after changing the
numbers below. Only the Python standard library is used.
"""
import hashlib
import json
import math
import struct
import zlib
from pathlib import Path

KIT = Path(__file__).resolve().parent.parent / 'packages' / 'showcase' / 'steel-ball-kit' / 'assets'
RADIUS = 1.25
MASS = 900.0
RINGS, SEGMENTS = 16, 32


def sphere():
    positions, normals, uv = [], [], []
    for r in range(RINGS + 1):
        theta = math.pi * r / RINGS
        for s in range(SEGMENTS + 1):
            phi = 2 * math.pi * s / SEGMENTS
            n = (math.sin(theta) * math.cos(phi), math.cos(theta), math.sin(theta) * math.sin(phi))
            normals.append([round(c, 6) for c in n])
            positions.append([round(c * RADIUS, 6) for c in n])
            uv.append([s / SEGMENTS, r / RINGS])
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
        'materials': [{'name': 'steel', 'wrap_u': True, 'wrap_v': True, 'blend': 'opaque', 'unlit': False,
                       'environment': False, 'mipmaps': True, 'detail_map': None, 'bump_map': None,
                       'reflectance_map': None, 'detail_scale': 1.0, 'reflectance': 1.0}],
        'animations': [],
    }


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


def steel(x, y):
    # Brushed along u: fine streaks from a fixed hash, a soft band of sheen.
    h = (x * 73856093 ^ (y // 2) * 19349663) & 0xFFFF
    streak = (h % 23) - 11
    sheen = 18 * math.cos(y / 64 * math.pi * 2)
    v = int(max(0, min(255, 168 + streak + sheen)))
    return bytes((v, min(255, v + 4), min(255, v + 10)))


def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return hashlib.sha256(data).hexdigest()


def main():
    model_bytes = (json.dumps(sphere(), separators=(',', ':')) + '\n').encode()
    model_sha = write(KIT / 'models' / 'steel-ball.shape.json', model_bytes)
    texture_sha = write(KIT / 'textures' / 'steel.png', png(64, 64, steel))
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
        'runover_speed': 3.0, 'runover_damage': 4.0, 'runover_push': 1.0,
        'protect_direct': False, 'protect_radius': False, 'protect_burn': False,
        'smash': {'speed': 10.0, 'radius': 1.2, 'max_volume': 30.0, 'force': 15.0},
        'shove': True,
        'authored': {'category': 'Vehicles', 'uiname': 'Steel Ball'},
        'adaptations': [],
    }
    pack = {
        'schema_version': 5,
        'definitions': [definition],
        'assets': [
            {'virtual_path': 'Add-Ons/Vehicle_SteelBall/steel-ball.dts', 'path': 'models/steel-ball.shape.json',
             'sha256': model_sha, 'kind': 'model', 'source_sha256': model_sha},
            {'virtual_path': 'Add-Ons/Vehicle_SteelBall/steel.png', 'path': 'textures/steel.png',
             'sha256': texture_sha, 'kind': 'texture', 'source_sha256': texture_sha},
        ],
        'evidence': [], 'unresolved': [], 'animation_aliases': {},
    }
    (KIT / 'vehicles.json').write_text(json.dumps(pack, indent=2) + '\n', encoding='utf-8', newline='\n')
    print(f'model {model_sha}\ntexture {texture_sha}')


if __name__ == '__main__':
    main()
