"""Write the Adventure Pack: 23 guns, their ammo and the files the rules
read, as our own models, icons requests and sounds.

The pack is an original remake of the guns in Bushido's Adventure Pack for
Blockland (re-released by Conan): the same line-up, each gun filling its
niche, with magazines, reloads, ammo pickups, headshots and a taser. None of
Bushido's or Conan's files are used or needed; the models are built here from
boxes and cylinders, the sounds are synthesized, and every number is ours.

Outputs, under packages/adventure/:
  adventure-pack/assets/weapons.json        items, images, rounds, sounds
  adventure-pack/assets/models/*.shape.json  one model per gun and ammo box
  adventure-pack/assets/models/adv_palette.png  the models' one texture
  adventure-pack/assets/icons/*.render.json  icons drawn from the models
  adventure-pack/assets/sounds/*.wav        shots, reloads, the taser
  adventure-pack-rules/adventure.rhai       the table between its GENERATED
                                            markers (the rest is hand-written)

Run it again after changing the tables below. Only the Python standard
library is used, with fixed seeds, so the output is the same every run.
"""
import json
import math
import random
import struct
import wave
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / 'packages' / 'adventure'
PACK = ROOT / 'adventure-pack'
ASSETS = PACK / 'assets'
RULES = ROOT / 'adventure-pack-rules' / 'adventure.rhai'
NS = 'adventure-pack'
TICKS = 120  # host ticks a second

# ---------------------------------------------------------------------------
# The guns. Times are in seconds, damage in health (a player has 100).
#
# mode: semi (one shot a pull), auto (held), burst3 (three a pull)
# ammo: which reserve feeds it; mag: rounds it holds
# every: seconds between shots (for burst3, between rounds of a burst)
# reload: seconds for a whole magazine, or for one shell with `shells`
# hitscan: the rules trace a ray instead of firing a round
# head: 'kill' or a multiplier for a hit in the head
# ---------------------------------------------------------------------------
GUNS = [
    # Automatic and burst rifles
    dict(key='assaultrifle', name='Assault Rifle', mode='auto', ammo='machinerifle', mag=30, every=0.085,
         reload=2.2, damage=16, speed=220, spread=0.0010, sound='rifle', look='assault'),
    dict(key='automaticrifle', name='Automatic Rifle', mode='auto', ammo='machinerifle', mag=20, every=0.11,
         reload=2.4, damage=22, speed=240, spread=0.0008, sound='heavyrifle', look='battle'),
    dict(key='burstrifle', name='Burst Rifle', mode='burst3', ammo='rifle', mag=30, every=0.06,
         reload=2.2, damage=20, speed=230, spread=0.0006, rest=0.3, sound='rifle', look='burst'),
    dict(key='servicerifle', name='Service Rifle', mode='semi', ammo='rifle', mag=15, every=0.14,
         reload=2.0, damage=32, speed=260, spread=0.0003, sound='heavyrifle', look='service'),
    # Automatic small arms
    dict(key='submachinegun', name='Submachine Gun', mode='auto', ammo='machinepistol', mag=32, every=0.07,
         reload=1.9, damage=12, speed=180, spread=0.0020, head=2.5, sound='smg', look='smg'),
    dict(key='machinepistol', name='Machine Pistol', mode='auto', ammo='machinepistol', mag=20, every=0.055,
         reload=1.5, damage=10, speed=170, spread=0.0030, head=2.5, sound='smg', look='machinepistol'),
    # Pistols
    dict(key='revolver', name='Revolver', mode='semi', ammo='revolver', mag=6, every=0.3,
         reload=2.6, damage=48, hitscan=300, sound='magnum', look='revolver'),
    dict(key='brushpistol', name='Brush Pistol', mode='semi', ammo='revolver', mag=2, every=0.2,
         reload=1.8, damage=40, speed=150, spread=0.0015, sound='magnum', look='brush'),
    dict(key='servicepistol', name='Service Pistol', mode='semi', ammo='pistol', mag=12, every=0.12,
         reload=1.4, damage=20, speed=180, spread=0.0006, sound='pistol', look='pistol'),
    dict(key='huntingmagnum', name='Hunting Magnum', mode='semi', ammo='revolver', mag=5, every=0.35,
         reload=2.4, damage=60, speed=260, zoom=45, sound='magnum', look='magnum'),
    dict(key='automaticpistol', name='Automatic Pistol', mode='auto', ammo='pistol', mag=18, every=0.075,
         reload=1.5, damage=13, speed=180, spread=0.0025, head=2.5, sound='pistol', look='autopistol'),
    # Shotguns
    dict(key='doubleshotgun', name='Double Shotgun', mode='semi', ammo='shotgun', mag=2, every=0.12,
         reload=2.4, damage=9, pellets=8, speed=160, spread=0.0040, recoil=6, head=2, sound='shotgun',
         look='double'),
    dict(key='pairedshotgun', name='Paired Shotgun', mode='semi', ammo='shotgun', mag=2, every=0.2, uses=2,
         reload=2.6, damage=8, pellets=16, speed=160, spread=0.0055, recoil=12, head=2, sound='bigshotgun',
         look='paired'),
    dict(key='singleshotgun', name='Single Shotgun', mode='semi', ammo='shotgun', mag=1, every=0.2,
         reload=1.1, damage=10, pellets=9, speed=170, spread=0.0030, recoil=5, head=2, sound='shotgun',
         look='single'),
    dict(key='levershotgun', name='Lever Shotgun', mode='semi', ammo='shotgun', mag=5, every=0.28,
         reload=0.5, shells=True, damage=9, pellets=8, speed=160, spread=0.0038, recoil=5, head=2,
         sound='shotgun', cycle='lever', look='levershotgun'),
    dict(key='huntingshotgun', name='Hunting Shotgun', mode='semi', ammo='shotgun', mag=6, every=0.36,
         reload=0.55, shells=True, damage=12, pellets=6, speed=190, spread=0.0022, recoil=6, head=2,
         sound='shotgun', cycle='pump', look='pump'),
    dict(key='automaticshotgun', name='Automatic Shotgun', mode='auto', ammo='shotgun', mag=8, every=0.18,
         reload=2.8, damage=7, pellets=7, speed=160, spread=0.0045, recoil=3, head=2, sound='shotgun',
         look='autoshotgun'),
    # Rifles
    dict(key='fieldrifle', name='Field Rifle', mode='semi', ammo='rifle', mag=5, every=0.42,
         reload=2.4, damage=55, speed=300, zoom=40, sound='heavyrifle', cycle='bolt', look='field'),
    dict(key='leverrifle', name='Lever Rifle', mode='semi', ammo='rifle', mag=8, every=0.27,
         reload=0.45, shells=True, damage=38, speed=260, sound='heavyrifle', cycle='lever', look='leverrifle'),
    # Sniper weapons
    dict(key='sniperrifle', name='Sniper Rifle', mode='semi', ammo='sniper', mag=5, every=0.85,
         reload=3.0, damage=95, hitscan=600, zoom=12, scope=True, sound='sniper', cycle='bolt', look='sniper'),
    dict(key='sniperrepeater', name='Sniper Repeater', mode='semi', ammo='sniper', mag=10, every=0.3,
         reload=2.8, damage=50, speed=400, zoom=25, scope=True, sound='sniper', look='repeater'),
    dict(key='huntingrifle', name='Hunting Rifle', mode='semi', ammo='sniper', mag=4, every=0.6,
         reload=0.6, shells=True, damage=75, speed=340, zoom=30, scope=True, sound='heavyrifle', cycle='bolt',
         look='hunting'),
    # Special
    dict(key='taser', name='Taser', mode='semi', ammo='battery', mag=1, every=0.2,
         reload=2.5, damage=5, hitscan=24, head=1, taser=True, sound='taser', look='taser'),
]

# Reserves: what a life starts with, the most carried, and what one pickup
# gives (a dropped gun of that kind; an ammo box of that kind gives double).
AMMO = {
    'pistol': dict(name='Pistol Ammo', start=36, most=96, gives=24, look=(0.72, 0.6, 0.25)),
    'revolver': dict(name='Revolver Ammo', start=18, most=48, gives=12, look=(0.7, 0.45, 0.2)),
    'machinepistol': dict(name='Machine Pistol Ammo', start=64, most=192, gives=40, look=(0.35, 0.45, 0.6)),
    'rifle': dict(name='Rifle Ammo', start=45, most=120, gives=30, look=(0.35, 0.42, 0.22)),
    'machinerifle': dict(name='Machine Rifle Ammo', start=60, most=180, gives=40, look=(0.3, 0.33, 0.3)),
    'shotgun': dict(name='Shotgun Shells', start=16, most=48, gives=10, look=(0.62, 0.12, 0.1)),
    'sniper': dict(name='Sniper Ammo', start=10, most=30, gives=6, look=(0.22, 0.24, 0.3)),
}

# ---------------------------------------------------------------------------
# Models: boxes and cylinders in Torque's axes (x right, y forward, z up),
# the hand's grip at the origin, written in the game's axes (x, z, -y).
# ---------------------------------------------------------------------------
# Warm, light colours in the style of Blockland's own add-on guns: grey
# metal, orange and brown wood, cream pistol slides, yellow brass.
PALETTE = {
    'black': (0.17, 0.17, 0.18), 'gunmetal': (0.34, 0.34, 0.36), 'steel': (0.52, 0.52, 0.54),
    'bright': (0.76, 0.76, 0.78), 'wood': (0.82, 0.44, 0.2), 'lightwood': (0.93, 0.6, 0.33),
    'olive': (0.42, 0.45, 0.26), 'tan': (0.88, 0.83, 0.7), 'brass': (0.96, 0.8, 0.16),
    'yellow': (0.95, 0.78, 0.1), 'lens': (0.25, 0.45, 0.65), 'red': (0.75, 0.12, 0.08),
    'blue': (0.2, 0.3, 0.55), 'white': (0.93, 0.92, 0.88), 'rubber': (0.2, 0.18, 0.17),
    'green': (0.25, 0.4, 0.2), 'darkwood': (0.48, 0.22, 0.09), 'card': (0.8, 0.62, 0.38),
    'tip': (0.86, 0.86, 0.82), 'shell': (0.82, 0.16, 0.1),
}
PALETTE.update({f'ammo_{key}': a['look'] for key, a in AMMO.items()})
SWATCH = 16
COLUMNS = 8


def swatch_uv(colour):
    i = list(PALETTE).index(colour)
    col, row = i % COLUMNS, i // COLUMNS
    rows = (len(PALETTE) + COLUMNS - 1) // COLUMNS
    return [(col + 0.5) / COLUMNS, (row + 0.5) / rows]


class Model:
    def __init__(self):
        self.positions, self.normals, self.uv, self.triangles = [], [], [], []
        self.nodes = {}

    def quad(self, corners, normal, colour):
        # Corners in Torque axes, counter-clockwise seen from outside.
        base = len(self.positions)
        uv = swatch_uv(colour)
        for c in corners:
            self.positions.append(native(c))
            self.normals.append(native(normal))
            self.uv.append(uv)
        self.triangles += [[base, base + 1, base + 2], [base, base + 2, base + 3]]

    def box(self, centre, size, colour):
        cx, cy, cz = centre
        hx, hy, hz = (s / 2 for s in size)
        for axis in range(3):
            for sign in (1, -1):
                n = [0, 0, 0]
                n[axis] = sign
                u = [0, 0, 0]
                v = [0, 0, 0]
                u[(axis + 1) % 3] = 1
                v[(axis + 2) % 3] = 1
                half = (hx, hy, hz)
                corners = []
                for a, b in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
                    p = [centre[k] + n[k] * half[k] + u[k] * a * half[k] + v[k] * b * half[k] for k in range(3)]
                    corners.append(p)
                if sign < 0:
                    corners.reverse()
                self.quad(corners, n, colour)

    def box_tilted(self, centre, size, colour, angle, hinge):
        # `box`, turned `angle` radians about the x axis through the hinge
        # line at (y, z) = `hinge`.
        start = len(self.positions)
        self.box(centre, size, colour)
        c, s_ = math.cos(angle), math.sin(angle)
        hy, hz = hinge
        for i in range(start, len(self.positions)):
            x, y, z = self.positions[i][0], -self.positions[i][2], self.positions[i][1]
            y, z = y - hy, z - hz
            y, z = c * y - s_ * z, s_ * y + c * z
            self.positions[i] = native([x, y + hy, z + hz])
            nx, ny, nz = self.normals[i][0], -self.normals[i][2], self.normals[i][1]
            self.normals[i] = native([nx, c * ny - s_ * nz, s_ * ny + c * nz])

    def cylinder(self, start, length, radius, colour, sides=8, axis='y'):
        # Along +y (or +z/+x) from `start`, capped.
        def at(t, angle):
            a, b = radius * math.cos(angle), radius * math.sin(angle)
            if axis == 'y':
                return [start[0] + a, start[1] + t, start[2] + b]
            if axis == 'z':
                return [start[0] + a, start[1] + b, start[2] + t]
            return [start[0] + t, start[1] + a, start[2] + b]

        def dirn(angle):
            a, b = math.cos(angle), math.sin(angle)
            return {'y': [a, 0, b], 'z': [a, b, 0], 'x': [0, a, b]}[axis]
        step = 2 * math.pi / sides
        for i in range(sides):
            a0, a1 = i * step, (i + 1) * step
            mid = (a0 + a1) / 2
            corners = [at(0, a0), at(0, a1), at(length, a1), at(length, a0)]
            self.oriented(corners, dirn(mid), colour)
        along = {'y': [0, 1, 0], 'z': [0, 0, 1], 'x': [1, 0, 0]}[axis]
        for t, sign in ((0, -1), (length, 1)):
            ring = [at(t, i * step) for i in range(sides)]
            centre = [sum(p[k] for p in ring) / sides for k in range(3)]
            n = [c * sign for c in along]
            for i in range(sides):
                self.oriented([centre, ring[i], ring[(i + 1) % sides], centre], n, colour, tri=True)

    def oriented(self, corners, normal, colour, tri=False):
        # Wind to face `normal`.
        a, b, c = corners[0], corners[1], corners[2]
        u = [b[k] - a[k] for k in range(3)]
        v = [c[k] - a[k] for k in range(3)]
        cross = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
        if sum(cross[k] * normal[k] for k in range(3)) < 0:
            corners = list(reversed(corners))
        if tri:
            base = len(self.positions)
            uv = swatch_uv(colour)
            for p in corners[:3]:
                self.positions.append(native(p))
                self.normals.append(native(normal))
                self.uv.append(uv)
            self.triangles.append([base, base + 1, base + 2])
        else:
            self.quad(corners, normal, colour)

    def node(self, name, at):
        self.nodes[name] = at

    def shape(self, name):
        nodes = [{'name': 'start', 'parent': None, 'translation': [0.0, 0.0, 0.0],
                  'rotation': [0.0, 0.0, 0.0, 1.0]}]
        for n, at in self.nodes.items():
            nodes.append({'name': n, 'parent': 0, 'translation': [round(v, 5) for v in native(at)],
                          'rotation': [0.0, 0.0, 0.0, 1.0]})
        return {
            'schema_version': 1,
            'id': f'{NS}:file/models/{name}.shape.json',
            'nodes': nodes,
            'objects': [{'name': name, 'node': 0, 'meshes': [0], 'visibility': 1.0, 'frame': 0,
                         'material_frame': 0}],
            'details': [{'name': 'detail100', 'pixel_threshold': 100.0, 'object_start': 0, 'object_count': 1,
                         'mesh_offset': 0, 'collision': False}],
            'meshes': [{
                'frame_vertices': len(self.positions),
                'positions': [[round(v, 5) for v in p] for p in self.positions],
                'normals': [[round(v, 5) for v in n] for n in self.normals],
                'uv': [[round(v, 5) for v in t] for t in self.uv],
                'primitives': [{'material': 0, 'triangles': self.triangles}],
                'skin': None, 'billboard': False, 'billboard_y': False,
            }],
            'materials': [{'name': 'adv_palette', 'wrap_u': False, 'wrap_v': False, 'blend': 'opaque',
                           'unlit': False, 'environment': False, 'mipmaps': True, 'detail_map': None,
                           'bump_map': None, 'reflectance_map': None, 'detail_scale': 1.0,
                           'reflectance': 1.0}],
            'animations': [],
        }


def native(p):
    return [float(p[0]), float(p[2]), float(-p[1])]


# Pieces most guns share. `length` runs forward from the receiver's front.
def grip(m, colour='wood', rake=0.12, height=0.42):
    m.box([0, -0.05 - rake / 2, -height / 2], [0.15, 0.17, height], colour)


def receiver(m, back, front, top=0.17, bottom=-0.03, colour='gunmetal', width=0.18):
    m.box([0, (back + front) / 2, (top + bottom) / 2], [width, front - back, top - bottom], colour)


def barrel(m, start, length, radius=0.04, z=0.09, colour='steel', x=0.0):
    m.cylinder([x, start, z], length, radius, colour)


def stock(m, back, front, colour='wood', drop=0.12, height=0.22, butt='darkwood'):
    m.box([0, (back + front) / 2, 0.06 - drop / 2], [0.14, front - back, height], colour)
    m.box([0, back + 0.03, 0.02 - drop / 2], [0.15, 0.06, height + 0.06], butt)


def magazine(m, y, depth=0.3, colour='gunmetal', rake=0.0, width=0.11, length=0.15):
    m.box([0, y + rake / 2, -depth / 2], [width, length, depth], colour)


def scope(m, back, front, z=0.3):
    m.cylinder([0, back, z], front - back, 0.065, 'black')
    m.cylinder([0, back - 0.05, z], 0.05, 0.08, 'black')
    m.cylinder([0, front, z], 0.06, 0.085, 'black')
    m.box([0, front + 0.061, z], [0.11, 0.002, 0.11], 'lens')
    m.box([0, (back + front) / 2, z - 0.1], [0.06, 0.22, 0.1], 'gunmetal')


def sight(m, y, z=0.2):
    m.box([0, y, z], [0.04, 0.04, 0.06], 'black')


def build(look):
    m = Model()
    muzzle = None
    eject = [0.1, 0.1, 0.12]
    if look in ('pistol', 'autopistol', 'machinepistol'):
        # A cream slide over a grey frame, like a service pistol.
        slide = {'pistol': 'tan', 'autopistol': 'tan', 'machinepistol': 'black'}[look]
        grip(m, 'black' if look == 'machinepistol' else 'gunmetal')
        m.box([0, -0.03, -0.2], [0.16, 0.12, 0.3], 'wood' if look == 'pistol' else 'black')  # grip panels
        receiver(m, -0.12, 0.55, top=0.17, bottom=0.03, colour=slide)
        m.box([0, 0.2, -0.0], [0.13, 0.4, 0.07], 'gunmetal')  # frame
        m.box([0, 0.08, -0.08], [0.05, 0.14, 0.03], 'gunmetal')  # trigger guard
        m.box([0, 0.08, -0.03], [0.03, 0.03, 0.08], 'black')  # trigger
        m.box([0.091, 0.0, 0.11], [0.004, 0.18, 0.05], 'gunmetal')  # slide serrations
        sight(m, 0.5, 0.2)
        sight(m, -0.08, 0.2)
        muzzle = [0, 0.58, 0.1]
        if look == 'autopistol':
            m.box([0, -0.05, -0.45], [0.13, 0.15, 0.12], 'gunmetal')  # long magazine
            m.box([0.095, 0.0, 0.05], [0.02, 0.08, 0.04], 'red')  # selector
        if look == 'machinepistol':
            magazine(m, -0.05, depth=0.64, colour='gunmetal')
            barrel(m, 0.55, 0.14, 0.045, 0.09, 'black')
            m.box([0, 0.36, -0.13], [0.09, 0.09, 0.2], 'wood')  # fore grip
            muzzle = [0, 0.69, 0.09]
    elif look in ('revolver', 'magnum', 'brush'):
        m.box([0, -0.1, -0.2], [0.15, 0.17, 0.4], 'wood' if look != 'magnum' else 'darkwood')
        receiver(m, -0.12, 0.2, top=0.17, bottom=-0.03, colour='steel' if look != 'brush' else 'gunmetal')
        if look != 'brush':
            m.cylinder([0, 0.0, 0.07], 0.2, 0.11, 'bright' if look == 'revolver' else 'steel')  # cylinder
        length = {'revolver': 0.45, 'magnum': 0.65, 'brush': 0.5}[look]
        if look == 'brush':
            barrel(m, 0.2, length, 0.05, 0.1, 'gunmetal', x=-0.045)
            barrel(m, 0.2, length, 0.05, 0.1, 'gunmetal', x=0.045)
            m.box([0, 0.2 + length / 2, 0.03], [0.15, length * 0.9, 0.05], 'lightwood')
        else:
            barrel(m, 0.2, length, 0.045, 0.1, 'steel')
            m.box([0, 0.2 + length / 2, 0.145], [0.04, length, 0.04], 'steel')  # rib
        sight(m, 0.2 + length - 0.03, 0.19)
        if look == 'magnum':
            scope(m, -0.05, 0.35, z=0.29)
        m.box([0, 0.02, -0.08], [0.05, 0.12, 0.03], 'gunmetal')
        muzzle = [0, 0.2 + length, 0.1]
        eject = [0.08, 0.05, 0.08]
    elif look in ('smg',):
        grip(m, 'gunmetal')
        receiver(m, -0.3, 0.45, top=0.17, bottom=-0.03, colour='gunmetal')
        m.box([0, 0.2, 0.0], [0.15, 0.4, 0.1], 'wood')  # wooden forend
        barrel(m, 0.45, 0.24, 0.04, 0.08, 'black')
        magazine(m, 0.3, depth=0.52, colour='black', length=0.12)
        stock(m, -0.95, -0.3, colour='wood', height=0.18)
        sight(m, 0.4, 0.21)
        muzzle = [0, 0.69, 0.08]
    elif look in ('assault', 'battle', 'burst', 'service'):
        # Grey metal with orange or brown wood furniture.
        metal = {'assault': 'gunmetal', 'battle': 'gunmetal', 'burst': 'steel', 'service': 'gunmetal'}[look]
        wood = {'assault': 'wood', 'battle': 'darkwood', 'burst': 'wood', 'service': 'darkwood'}[look]
        grip(m, wood)
        receiver(m, -0.25, 0.6, top=0.19, bottom=-0.03, colour=metal)
        stock(m, -1.0, -0.25, colour=wood)
        m.box([0, 0.85, 0.07], [0.15, 0.5, 0.16], wood)  # handguard
        barrel(m, 1.1, 0.4 if look != 'service' else 0.55, 0.04, 0.08, 'steel')
        rake = 0.1 if look in ('assault', 'battle') else 0.0
        magazine(m, 0.38, depth=0.44 if look != 'battle' else 0.34, colour='gunmetal' if look != 'burst' else 'black',
                 rake=rake)
        if look == 'assault':
            m.box([0, 0.1, 0.28], [0.08, 0.45, 0.07], 'gunmetal')  # carry handle
            m.box([0, -0.1, 0.22], [0.08, 0.06, 0.08], 'gunmetal')
            sight(m, 1.0, 0.25)
        elif look == 'burst':
            m.box([0, 0.15, 0.26], [0.11, 0.25, 0.09], 'black')
            m.box([0, 0.28, 0.26], [0.09, 0.01, 0.07], 'lens')
        else:
            sight(m, 1.0, 0.21)
            sight(m, -0.1, 0.25)
        muzzle = [0, 1.5 if look != 'service' else 1.65, 0.08]
        eject = [0.1, 0.2, 0.12]
    elif look in ('field', 'leverrifle', 'hunting', 'sniper', 'repeater'):
        wood = {'field': 'wood', 'leverrifle': 'lightwood', 'hunting': 'darkwood', 'sniper': 'darkwood',
                'repeater': 'wood'}[look]
        m.box([0, -0.08, -0.14], [0.14, 0.17, 0.3], wood)  # wrist
        receiver(m, -0.2, 0.45, top=0.17, bottom=0.0, colour='gunmetal' if look != 'leverrifle' else 'brass')
        stock(m, -1.05, -0.15, colour=wood)
        m.box([0, 0.9, 0.04], [0.14, 0.9, 0.13], wood)  # forend
        length = {'field': 0.85, 'leverrifle': 0.75, 'hunting': 0.95, 'sniper': 1.25, 'repeater': 1.05}[look]
        barrel(m, 0.45, length, 0.04 if look != 'sniper' else 0.05, 0.1, 'steel' if look != 'sniper' else 'gunmetal')
        if look == 'leverrifle':
            m.box([0, 0.02, -0.16], [0.04, 0.26, 0.04], 'steel')  # lever loop
            m.box([0, -0.1, -0.1], [0.04, 0.04, 0.12], 'steel')
            m.cylinder([0, 0.45, 0.02], 0.7, 0.03, 'steel')  # tube
            sight(m, 1.15, 0.18)
        else:
            m.box([0.11, -0.02, 0.13], [0.09, 0.04, 0.04], 'steel')  # bolt handle
            scope(m, -0.15, 0.45 if look != 'sniper' else 0.55, z=0.31)
        if look in ('sniper', 'repeater'):
            magazine(m, 0.2, depth=0.22, colour='gunmetal')
        if look == 'sniper':
            m.box([0.05, 1.3, -0.12], [0.03, 0.03, 0.24], 'black')  # bipod
            m.box([-0.05, 1.3, -0.12], [0.03, 0.03, 0.24], 'black')
        muzzle = [0, 0.45 + length, 0.1]
        eject = [0.1, 0.15, 0.12]
    elif look in ('double', 'paired', 'single', 'levershotgun', 'pump', 'autoshotgun'):
        wood = {'double': 'darkwood', 'paired': 'lightwood', 'single': 'wood', 'levershotgun': 'wood',
                'pump': 'darkwood', 'autoshotgun': 'wood'}[look]
        m.box([0, -0.08, -0.14], [0.14, 0.17, 0.3], wood)
        receiver(m, -0.2, 0.35, top=0.19, bottom=-0.03,
                 colour='steel' if look in ('double', 'paired') else 'gunmetal')
        stock(m, -1.0, -0.15, colour=wood)
        length = {'double': 1.0, 'paired': 0.85, 'single': 0.95, 'levershotgun': 0.9, 'pump': 1.05,
                  'autoshotgun': 0.9}[look]
        if look in ('double', 'paired'):
            barrel(m, 0.35, length, 0.055, 0.1, 'steel', x=-0.055)
            barrel(m, 0.35, length, 0.055, 0.1, 'steel', x=0.055)
            m.box([0, 0.35 + 0.35, 0.02], [0.16, 0.7, 0.09], wood)
            if look == 'paired':
                m.box([0, 0.3, 0.21], [0.21, 0.08, 0.05], 'brass')  # joined triggers' latch
        else:
            barrel(m, 0.35, length, 0.055, 0.11, 'steel' if look != 'autoshotgun' else 'gunmetal')
            m.cylinder([0, 0.35, 0.02], length * 0.8, 0.045, 'gunmetal')  # tube
            if look == 'pump':
                m.box([0, 0.75, 0.03], [0.15, 0.35, 0.13], 'wood')
            if look == 'levershotgun':
                m.box([0, 0.02, -0.16], [0.04, 0.26, 0.04], 'steel')
                m.box([0, -0.1, -0.1], [0.04, 0.04, 0.12], 'steel')
            if look == 'autoshotgun':
                magazine(m, 0.25, depth=0.36, colour='gunmetal', length=0.21, width=0.15)
                m.box([0, 0.7, 0.03], [0.15, 0.4, 0.13], 'wood')
        sight(m, 0.35 + length - 0.04, 0.19)
        muzzle = [0, 0.35 + length, 0.1]
        eject = [0.1, 0.1, 0.12]
    elif look == 'taser':
        m.box([0, -0.05, -0.18], [0.15, 0.17, 0.36], 'black')
        receiver(m, -0.12, 0.35, top=0.17, bottom=-0.03, colour='yellow')
        m.box([0, 0.4, 0.07], [0.17, 0.12, 0.17], 'black')  # cartridge
        m.box([0.05, 0.47, 0.1], [0.04, 0.02, 0.04], 'blue')
        m.box([-0.05, 0.47, 0.1], [0.04, 0.02, 0.04], 'blue')
        m.box([0, 0.1, 0.19], [0.11, 0.18, 0.05], 'black')
        muzzle = [0, 0.48, 0.1]
    else:
        raise ValueError(look)
    m.node('mountPoint', [0, 0, 0])
    m.node('muzzlePoint', muzzle)
    m.node('ejectPoint', eject)
    return m


def round_model(length, radius):
    # A round in flight, pointing along +y: a brass case behind a lead tip.
    m = Model()
    m.cylinder([0, -length / 2, 0], length * 0.6, radius, 'brass', sides=6)
    m.cylinder([0, length * 0.1, 0], length * 0.4, radius * 0.7, 'tip', sides=6)
    m.node('mountPoint', [0, 0, 0])
    return m


# How each kind of ammo is packed: the box, its lid, and the rounds in it
# (length, radius, the case's colour, the tip's colour).
PACKING = {
    'pistol': dict(box='gunmetal', lid='gunmetal', size=(0.42, 0.26, 0.2), round=(0.12, 0.028, 'brass', 'tip')),
    'revolver': dict(box='card', lid='card', size=(0.32, 0.24, 0.14), round=(0.13, 0.03, 'brass', 'gunmetal')),
    'machinepistol': dict(box='olive', lid='olive', size=(0.44, 0.26, 0.22), round=(0.12, 0.028, 'brass', 'tip')),
    'rifle': dict(box='wood', lid='lightwood', size=(0.5, 0.3, 0.2), round=(0.2, 0.03, 'brass', 'tip')),
    'machinerifle': dict(box='gunmetal', lid='olive', size=(0.5, 0.28, 0.24), round=(0.2, 0.03, 'brass', 'brass')),
    'shotgun': dict(box='wood', lid='wood', size=(0.46, 0.3, 0.18), round=(0.17, 0.045, 'shell', 'brass')),
    'sniper': dict(box='darkwood', lid='darkwood', size=(0.56, 0.24, 0.18), round=(0.28, 0.034, 'brass', 'tip')),
}


def cartridge(m, at, length, radius, case, tip, axis='z'):
    # Standing up (`axis` z) or lying along x/y; a shotgun shell's `tip` is
    # its brass base, so it goes at the bottom.
    x, y, z = at
    if case == 'shell':
        m.cylinder([x, y, z], length * 0.22, radius, tip, sides=6, axis=axis)
        step = [0, 0, 0]
        step['xyz'.index(axis)] = length * 0.22
        m.cylinder([x + step[0], y + step[1], z + step[2]], length * 0.78, radius * 0.95, case, sides=6, axis=axis)
        return
    m.cylinder([x, y, z], length * 0.7, radius, case, sides=6, axis=axis)
    step = [0, 0, 0]
    step['xyz'.index(axis)] = length * 0.7
    m.cylinder([x + step[0], y + step[1], z + step[2]], length * 0.3, radius * 0.7, tip, sides=6, axis=axis)


def crate(m, kind, at=(0.0, 0.0), turn=False, lid=True):
    # An open box of `kind`'s rounds, standing in rows, with the lid
    # thrown back and the kind's colour on its label.
    p = PACKING[kind]
    w, d, h = p['size']
    if turn:
        w, d = d, w
    ox, oy = at
    t = 0.025
    m.box([ox, oy, t / 2], [w, d, t], p['box'])
    m.box([ox - w / 2 + t / 2, oy, h / 2], [t, d, h], p['box'])
    m.box([ox + w / 2 - t / 2, oy, h / 2], [t, d, h], p['box'])
    m.box([ox, oy - d / 2 + t / 2, h / 2], [w - 2 * t, t, h], p['box'])
    m.box([ox, oy + d / 2 - t / 2, h / 2], [w - 2 * t, t, h], p['box'])
    m.box([ox, oy - d / 2 - 0.002, h * 0.55], [w * 0.5, 0.004, h * 0.35], f'ammo_{kind}')  # label
    length, radius, case, tip = p['round']
    stand = min(length, h + 0.04)
    cols = max(1, int((w - 2 * t) / (radius * 2.3)))
    rows = max(1, int((d - 2 * t) / (radius * 2.3)))
    for i in range(cols):
        for j in range(rows):
            x = ox - w / 2 + t + (i + 0.5) * (w - 2 * t) / cols
            y = oy - d / 2 + t + (j + 0.5) * (d - 2 * t) / rows
            cartridge(m, (x, y, t + max(0.0, h - t - stand + 0.05)), stand, radius, case, tip)
    if lid:
        # Hinged at the back edge, thrown back past upright.
        m.box_tilted([ox, oy + d / 2 + 0.012, h + d / 2 - 0.02], [w, t, d], p['lid'],
                     angle=-0.35, hinge=(oy + d / 2, h))


def loose(m, kind, x, y, angle_axis='x'):
    length, radius, case, tip = PACKING[kind]['round']
    cartridge(m, (x, y, radius), length, radius, case, tip, axis=angle_axis)


def ammo_model(key):
    m = Model()
    if key == 'pile':
        # A heap of open boxes of every kind, with rounds spilled round it.
        crate(m, 'rifle', (-0.22, 0.05))
        crate(m, 'shotgun', (0.28, -0.05), turn=True)
        crate(m, 'pistol', (0.0, -0.32), lid=False)
        crate(m, 'sniper', (0.02, 0.36), lid=False)
        for kind, x, y, axis in [('rifle', -0.5, -0.25, 'x'), ('shotgun', 0.52, 0.25, 'y'),
                                 ('pistol', -0.32, -0.45, 'y'), ('sniper', 0.4, 0.42, 'x')]:
            loose(m, kind, x, y, axis)
    else:
        crate(m, key)
        w, d, _ = PACKING[key]['size']
        loose(m, key, w / 2 + 0.06, -d / 4, 'y')
        loose(m, key, -w / 2 - 0.02, -d / 2 - 0.06, 'x')
    m.node('mountPoint', [0, 0, 0])
    return m


# ---------------------------------------------------------------------------
# Image states. Every gun counts its rounds through the rules:
#   onActivate -> equip     sets whether the gun is loaded as it comes out
#   onFire     -> fired     spends rounds (and traces a hitscan gun's shot)
#   onCheck    -> check     may it reload? (ammo on: yes; off: it is empty)
#   onReloaded -> reloaded  fills the magazine, or loads one shell
#   onInterrupt-> interrupt fire stops a shell-by-shell reload
# and the light key reloads (`reload`).
# ---------------------------------------------------------------------------
def ticks(seconds):
    return max(1, round(seconds * TICKS))


def states(g):
    sound = f'{NS}:{g["sound"]}'
    flash = dict(emitter='gunFlashEmitter', emitter_node='muzzlePoint', emitter_seconds=0.05)
    hitscan = 'hitscan' in g
    fire = dict(script='onFire', sound=sound, eject_shell=not hitscan and not g.get('cycle'), **(flash if not g.get('taser') else {}))
    s = [
        dict(name='Activate', ticks=ticks(0.25), script='onActivate', sound='weaponSwitchSound', timeout='Ready',),
        dict(name='Ready', no_ammo='Check', down='Fire'),
    ]
    if g['mode'] == 'burst3':
        s += [
            dict(name='Fire', ticks=ticks(g['every']), no_ammo='Check', timeout='Burst2', wait=True,
                 allow_change=False, **fire),
            dict(name='Burst2', ticks=ticks(g['every']), no_ammo='Check', timeout='Burst3', wait=True,
                 allow_change=False, **fire),
            dict(name='Burst3', ticks=ticks(g['rest']), no_ammo='Check', timeout='After', wait=True,
                 allow_change=False, **fire),
        ]
    else:
        s.append(dict(name='Fire', ticks=ticks(g['every']), no_ammo='Check', timeout='After' if not g.get('cycle')
                      else 'Cycle', wait=True, allow_change=False, **fire))
        if g.get('cycle'):
            s.append(dict(name='Cycle', ticks=ticks(0.25), no_ammo='Check', timeout='After', wait=True,
                          sound=f'{NS}:{g["cycle"]}', eject_shell=not hitscan, allow_change=False))
    if g['mode'] == 'auto':
        s.append(dict(name='After', ticks=0, no_ammo='Check', timeout='Ready'))
    else:
        s.append(dict(name='After', no_ammo='Check', up='Ready'))
    s += [
        dict(name='Check', ticks=1, script='onCheck', ammo='Reload', no_ammo='Empty'),
    ]
    if g.get('shells'):
        s += [
            dict(name='Reload', ticks=ticks(g['reload']), wait=False, down='Interrupt', timeout='Reloaded',
                 sound=f'{NS}:shell'),
            dict(name='Reloaded', ticks=1, script='onReloaded', ammo='Ready', no_ammo='Reload'),
            dict(name='Interrupt', ticks=1, script='onInterrupt', ammo='Ready', no_ammo='Reload'),
        ]
    else:
        s += [
            dict(name='Reload', ticks=ticks(g['reload']), timeout='Reloaded', allow_change=True,
                 sound=f'{NS}:{"charge" if g.get("taser") else "reload"}'),
            dict(name='Reloaded', ticks=1, script='onReloaded', ammo='Ready', no_ammo='Empty'),
        ]
    s += [
        dict(name='Empty', ammo='Check', down='Click'),
        dict(name='Click', ticks=ticks(0.25), timeout='Empty', sound=f'{NS}:click'),
    ]
    index = {state['name']: i for i, state in enumerate(s)}
    out = []
    for state in s:
        state = dict(state)
        for field in ('timeout', 'down', 'up', 'ammo', 'no_ammo'):
            if field in state:
                state[field] = index[state[field]]
        out.append(state)
    return out


RULES_ID = 'adventure-pack-rules'


def weapons_json():
    items, images, projectiles, damage_types = {}, {}, {}, {}
    for g in GUNS:
        k = g['key']
        items[f'{NS}:weapon/{k}'] = {
            'name': f'Adv{k}Item', 'ui_name': g['name'], 'image': f'{NS}:image/{k}',
            'model': f'models/{k}', 'icon': f'icons/{k}',
        }
        image = {
            'name': f'Adv{k}Image', 'model': f'models/{k}', 'mount_point': 0, 'arm_ready': True,
            'correct_muzzle': True, 'casing': '' if 'hitscan' in g else 'gunShellDebris',
            'states': states(g),
            'commands': {
                'states': {
                    'onactivate': f'{RULES_ID}:equip', 'onfire': f'{RULES_ID}:fired',
                    'oncheck': f'{RULES_ID}:check', 'onreloaded': f'{RULES_ID}:reloaded',
                    **({'oninterrupt': f'{RULES_ID}:interrupt'} if g.get('shells') else {}),
                },
                'light': f'{RULES_ID}:reload',
            },
        }
        if 'hitscan' not in g:
            image['projectile'] = f'{NS}:projectile/{k}'
            if g.get('pellets', 1) > 1 or g.get('spread') or g.get('recoil'):
                image['shot'] = {'projectiles': g.get('pellets', 1), 'spread': g.get('spread', 0.0),
                                 'recoil': g.get('recoil', 0)}
            projectiles[f'{NS}:projectile/{k}'] = {
                'name': f'Adv{k}Projectile', 'model': 'models/round' if g.get('pellets', 1) == 1 else 'models/pellet',
                'speed': g['speed'], 'gravity': 0.0, 'lifetime_ticks': ticks(2.0), 'fade_ticks': ticks(2.0),
                'damage': g['damage'], 'damage_type': f'$DamageType::Adv{k}',
                'impulse': 60.0 if g.get('pellets', 1) == 1 else 20.0,
                'explosion': {'effect': 'gunExplosion'},
            }
        if 'zoom' in g:
            image['zoom'] = {'fov': g['zoom'], 'on_jet': True, 'crosshair': not g.get('scope', False),
                             'first_person': bool(g.get('scope'))}
        images[f'{NS}:image/{k}'] = image
        verb = {'taser': 'tased'}.get(k, 'shot')
        damage_types[f'adv{k}'] = {
            'name': f'Adv{k}', 'suicide_message': '%1 shot themselves',
            'murder_message': f'%2 {verb} %1 ({g["name"]})', 'vehicle_scale': 1.0, 'direct': True,
        }
    for key, a in AMMO.items():
        items[f'{NS}:weapon/ammo{key}'] = {
            'name': f'Adv{key}AmmoItem', 'ui_name': a['name'], 'model': f'models/ammo{key}',
            'icon': f'icons/ammo{key}',
        }
    items[f'{NS}:weapon/ammopile'] = {'name': 'AdvAmmoPileItem', 'ui_name': 'Ammo Pile',
                                       'model': 'models/ammopile', 'icon': 'icons/ammopile'}
    sounds = {f'{NS}:{name}': {'file': f'sounds/{name}.wav', **extra} for name, extra in SOUND_KEYS.items()}
    return {
        'schema_version': 3, 'id': NS, 'items': items, 'images': images, 'projectiles': projectiles,
        'damage_types': damage_types, 'sounds': sounds,
    }


SOUND_KEYS = {
    'pistol': {'volume': 0.8}, 'magnum': {}, 'smg': {'volume': 0.75}, 'rifle': {'volume': 0.85},
    'heavyrifle': {}, 'shotgun': {}, 'bigshotgun': {}, 'sniper': {}, 'taser': {'volume': 0.8},
    'reload': {'volume': 0.7}, 'shell': {'volume': 0.7}, 'charge': {'volume': 0.6},
    'lever': {'volume': 0.7}, 'pump': {'volume': 0.7}, 'bolt': {'volume': 0.7},
    'click': {'volume': 0.6, 'local': True}, 'headshot': {'volume': 0.7, 'local': True},
    'ammo': {'volume': 0.7, 'local': True},
}

# ---------------------------------------------------------------------------
# Sounds: 16-bit mono WAV at 22050 Hz.
# ---------------------------------------------------------------------------
RATE = 22050
rng = random.Random(20260930)


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
    return [x - y for x, y in zip(samples, lowpass(samples, cutoff))]


def envelope(samples, attack, seconds):
    out = []
    for i, x in enumerate(samples):
        t = i / RATE
        gain = min(1.0, t / attack) if attack > 0 else 1.0
        out.append(x * gain * math.exp(-t / seconds))
    return out


def tone(seconds, f0, f1, shape=math.sin):
    out, phase = [], 0.0
    n = int(seconds * RATE)
    for i in range(n):
        phase += 2 * math.pi * (f0 + (f1 - f0) * i / max(1, n)) / RATE
        out.append(shape(phase))
    return out


def mix(*layers):
    n = max(len(layer) for layer, _ in layers)
    out = [0.0] * n
    for layer, gain in layers:
        for i, x in enumerate(layer):
            out[i] += x * gain
    return out


def pad(samples, seconds):
    return [0.0] * int(seconds * RATE) + samples


def normalise(samples, peak=0.9):
    top = max(abs(x) for x in samples) or 1.0
    return [x * peak / top for x in samples]


def gunshot(crack, boom, tail, body_hz, length):
    return normalise(mix(
        (envelope(highpass(noise(0.06), 2500), 0.0005, 0.008 * crack), 1.0),
        (envelope(lowpass(noise(length), body_hz), 0.001, 0.05 * boom), 1.2),
        (envelope(tone(0.25, body_hz * 0.12, body_hz * 0.05), 0.001, 0.06 * boom), 0.6),
        (envelope(lowpass(noise(length), body_hz * 0.5), 0.02, tail), 0.35),
    ))


def clack(parts):
    out = []
    for at, hz, gain in parts:
        out = mix((out, 1.0), (pad(envelope(highpass(noise(0.04), hz), 0.0003, 0.006), at), gain))
    return normalise(out, 0.8)


def sounds():
    return {
        'pistol': gunshot(1.0, 0.8, 0.12, 1800, 0.45),
        'magnum': gunshot(1.4, 1.4, 0.25, 1300, 0.7),
        'smg': gunshot(0.8, 0.6, 0.08, 2200, 0.3),
        'rifle': gunshot(1.2, 1.0, 0.18, 1600, 0.55),
        'heavyrifle': gunshot(1.5, 1.4, 0.28, 1200, 0.8),
        'shotgun': gunshot(1.0, 1.8, 0.3, 900, 0.8),
        'bigshotgun': gunshot(1.2, 2.4, 0.4, 700, 1.0),
        'sniper': gunshot(2.0, 1.8, 0.5, 1000, 1.2),
        'taser': normalise(mix(
            (envelope([math.copysign(1, x) for x in tone(0.5, 55, 60)], 0.002, 0.2), 0.5),
            (envelope(highpass(noise(0.5), 3000), 0.002, 0.15), 0.6),
            (envelope(tone(0.5, 2400, 1800), 0.002, 0.1), 0.2),
        ), 0.7),
        'reload': clack([(0.0, 1500, 0.8), (0.35, 2500, 1.0), (0.8, 1800, 0.9), (0.95, 3000, 1.0)]),
        'shell': clack([(0.0, 1200, 0.8), (0.12, 2600, 0.6)]),
        'charge': normalise(envelope(tone(0.8, 400, 2400), 0.3, 0.8), 0.4),
        'lever': clack([(0.0, 2000, 0.9), (0.14, 1400, 1.0)]),
        'pump': clack([(0.0, 900, 1.0), (0.16, 1100, 1.0)]),
        'bolt': clack([(0.0, 2500, 0.8), (0.1, 1600, 1.0), (0.22, 2200, 0.8)]),
        'click': clack([(0.0, 4000, 1.0)]),
        'headshot': normalise(envelope(mix((tone(0.25, 1800, 1800), 0.6), (tone(0.25, 2700, 2700), 0.4)),
                                       0.001, 0.07), 0.6),
        'ammo': clack([(0.0, 1800, 0.7), (0.06, 2400, 0.7), (0.12, 2000, 0.7)]),
    }


def write_wav(path, samples):
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), 'wb') as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(b''.join(struct.pack('<h', int(max(-1, min(1, x)) * 32767)) for x in samples))


def png(width, height, pixel):
    raw = b''.join(b'\x00' + b''.join(bytes(pixel(x, y)) for x in range(width)) for y in range(height))

    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data) & 0xffffffff)
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b''))


def palette_png():
    names = list(PALETTE)
    rows = (len(names) + COLUMNS - 1) // COLUMNS

    def pixel(x, y):
        i = (y // SWATCH) * COLUMNS + x // SWATCH
        colour = PALETTE[names[i]] if i < len(names) else (1, 0, 1)
        return [round(c * 255) for c in colour]
    return png(COLUMNS * SWATCH, rows * SWATCH, pixel)


def dump(path, value, indent=None):
    path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(value, indent=indent, separators=None if indent else (',', ':'))
    path.write_text(text + '\n', encoding='utf-8', newline='\n')


def icon_request(pose_like='v20.weapon.gunitem', base=(0.92, 0.92, 0.9)):
    return {'schema_version': 1, 'pose_like': pose_like, 'look': {'base': list(base)}}


def rules_table():
    lines = ['// The guns and ammo, written by tools/make_adventure_pack.py.',
             '// The gun an image belongs to, or ().', 'fn gun(image) {', '    switch image {']
    for g in GUNS:
        fields = {
            'item': json.dumps(f'{NS}:weapon/{g["key"]}'), 'name': json.dumps(g['name']),
            'ammo': json.dumps(g['ammo']), 'mag': g['mag'], 'uses': g.get('uses', 1),
            'shells': 'true' if g.get('shells') else 'false', 'damage': float(g['damage']),
            'range': float(g.get('hitscan', 0)),
            'head': json.dumps('kill') if g.get('head', 'kill') == 'kill' else float(g['head']),
            'taser': 'true' if g.get('taser') else 'false', 'type': json.dumps(f'Adv{g["key"]}'),
        }
        body = ', '.join(f'{k}: {v}' for k, v in fields.items())
        lines.append(f'        "{NS}:image/{g["key"]}" => #{{ {body} }},')
    lines += ['        _ => ()', '    }', '}',
              '// What a hit in the head does for a damage type: "kill", a multiplier, or ().',
              'fn head_of(damage_type) {', '    switch damage_type {']
    for g in GUNS:
        head = json.dumps('kill') if g.get('head', 'kill') == 'kill' else float(g['head'])
        lines.append(f'        "Adv{g["key"]}" => {head},')
    lines += ['        _ => ()', '    }', '}',
              '// A reserve: what a life starts with, the most carried, what one pickup gives.',
              'fn reserve(kind) {', '    switch kind {']
    for key, a in AMMO.items():
        lines.append(f'        "{key}" => #{{ start: {a["start"]}, most: {a["most"]}, gives: {a["gives"]} }},')
    lines += ['        _ => ()', '    }', '}']
    return '\n'.join(lines)


def main():
    for g in GUNS:
        dump(ASSETS / 'models' / f'{g["key"]}.shape.json', build(g['look']).shape(g['key']))
        dump(ASSETS / 'icons' / f'{g["key"]}.render.json', icon_request())
    for key in list(AMMO) + ['pile']:
        dump(ASSETS / 'models' / f'ammo{key}.shape.json', ammo_model(key).shape(f'ammo{key}'))
        dump(ASSETS / 'icons' / f'ammo{key}.render.json', icon_request())
    dump(ASSETS / 'models' / 'round.shape.json', round_model(0.3, 0.035).shape('round'))
    dump(ASSETS / 'models' / 'pellet.shape.json', round_model(0.12, 0.03).shape('pellet'))
    (ASSETS / 'models' / 'adv_palette.png').write_bytes(palette_png())
    dump(ASSETS / 'weapons.json', weapons_json(), indent=2)
    for name, samples in sounds().items():
        write_wav(ASSETS / 'sounds' / f'{name}.wav', samples)
    script = RULES.read_text(encoding='utf-8')
    begin, end = '// BEGIN GENERATED', '// END GENERATED'
    head, rest = script.split(begin, 1)
    _, tail = rest.split(end, 1)
    RULES.write_text(f'{head}{begin}\n{rules_table()}\n{end}{tail}', encoding='utf-8', newline='\n')
    print(f'wrote {len(GUNS)} guns and {len(AMMO) + 1} ammo pickups to {ROOT}')


if __name__ == '__main__':
    main()
