"""Write the classic weapon Add-Ons: our own Butterfly Knife and HE-Grenade.

Both are remakes of v20-era Add-Ons, built from nothing of theirs: the
models, animations, textures and sounds are made here, by code, and the
weapons packs say how they play. Their designs are credited in each
package.json.

  packages/classic/butterfly-knife/assets/
    weapons.json        the knife: a click jabs, holding for 0.7 s then
                        letting go stabs
    models/butterfly-knife.shape.json
                        a balisong: a blade between two handles on their
                        own pivot pins. `activate` flips it open (the blade
                        swings over, one handle whips a full turn round
                        and lands beside the other); `ready` holds it open
    textures/*.png      flat colours: handles, blade, pins
    sounds/flip.wav     the flip's rattle and the final clack
    sounds/swing.wav    the swish of a jab or a stab
    icons/butterfly_knife.render.json
                        its icon, drawn from the model like the Sword's
  packages/classic/he-grenade/assets/
    weapons.json        the grenade: the first click pulls the pin, the
                        second throws it on a 2.5 s fuse; the blast breaks
                        bricks like a rocket's
    models/he-grenade.shape.json        held: segmented body, fuze, spoon,
                                        pin and ring; `pinpull` hides the pin
    models/he-grenade-thrown.shape.json in flight: no pin, tumbling
                                        (`activate`, looping)
    models/he-grenade-pin.shape.json    the pin and ring that fly off
    sounds/pin.wav, sounds/bounce.wav
    icons/he_grenade.render.json

Each gets `presentation.json` and `item-physics.json` (the item
presentation format Import Add-On writes), which bind the models and
textures by SHA-256 and must match `weapons.json`; run this again after
changing anything here. Only the Python standard library is used, with
fixed seeds, so the output is the same on every run.

Axes are the game's: x right, y up, -z forward (where a held weapon
points).
"""
import hashlib
import json
import math
import random
import struct
import wave
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / 'packages' / 'classic'
RATE = 22050


# --- files -------------------------------------------------------------

def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return hashlib.sha256(data).hexdigest()


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


def flat_texture(rgb, seed, grain=0.0):
    """A 16 pixel square of one colour, with a faint grain."""
    rng = random.Random(seed)
    noise = [rng.uniform(-grain, grain) for _ in range(256)]

    def pixel(x, y):
        n = noise[y * 16 + x]
        return bytes(max(0, min(255, round((c + n) * 255))) for c in rgb)
    return png(16, 16, pixel)


def wav(samples):
    peak = max(1e-9, max(abs(s) for s in samples))
    scale = 0.9 / peak
    frames = b''.join(struct.pack('<h', int(max(-1, min(1, s * scale)) * 32767)) for s in samples)
    import io
    out = io.BytesIO()
    with wave.open(out, 'wb') as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(frames)
    return out.getvalue()


def dump(value):
    return (json.dumps(value, indent=2) + '\n').encode()


# --- geometry ----------------------------------------------------------

def v_add(a, b):
    return [a[i] + b[i] for i in range(3)]


def v_sub(a, b):
    return [a[i] - b[i] for i in range(3)]


def v_scale(a, s):
    return [c * s for c in a]


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def normalize(a):
    length = math.sqrt(sum(c * c for c in a))
    return [c / length for c in a] if length > 1e-12 else [0.0, 1.0, 0.0]


def round3(v):
    return [round(c, 6) for c in v]


class Mesh:
    """Flat-shaded polygons by material: every face has its own vertices,
    so each reads as a crisp facet, as Blockland's models do."""

    def __init__(self):
        self.positions, self.normals, self.uv = [], [], []
        self.triangles = {}

    def face(self, points, material, normal=None):
        """A convex polygon, its points counter-clockwise seen from outside."""
        if normal is None:
            # Newell's method: robust for slightly non-planar quads.
            n = [0.0, 0.0, 0.0]
            for i, a in enumerate(points):
                b = points[(i + 1) % len(points)]
                n[0] += (a[1] - b[1]) * (a[2] + b[2])
                n[1] += (a[2] - b[2]) * (a[0] + b[0])
                n[2] += (a[0] - b[0]) * (a[1] + b[1])
            if sum(c * c for c in n) < 1e-18:
                return
            normal = normalize(n)
        base = len(self.positions)
        for p in points:
            self.positions.append(round3(p))
            self.normals.append(round3(normal))
            self.uv.append([0.5, 0.5])
        tris = self.triangles.setdefault(material, [])
        for i in range(1, len(points) - 1):
            tris.append([base, base + i, base + i + 1])

    def quad_strip(self, a, b, material):
        """Faces between two loops of points (a then b), outward when both
        loops run counter-clockwise seen from a's side looking toward b."""
        n = len(a)
        for i in range(n):
            j = (i + 1) % n
            self.face([a[i], a[j], b[j], b[i]], material)

    def bounds(self):
        lo = [min(p[i] for p in self.positions) for i in range(3)]
        hi = [max(p[i] for p in self.positions) for i in range(3)]
        return lo, hi

    def native(self):
        return {
            'frame_vertices': len(self.positions),
            'positions': self.positions,
            'normals': self.normals,
            'uv': self.uv,
            'primitives': [{'material': m, 'triangles': t} for m, t in sorted(self.triangles.items())],
            'skin': None,
            'billboard': False,
            'billboard_y': False,
        }


def prism(mesh, outline, x0, x1, material, caps=True):
    """`outline` ((z, y) points, counter-clockwise seen from +x) extruded
    from x = x0 to x = x1."""
    front = [[x1, y, z] for z, y in outline]
    back = [[x0, y, z] for z, y in outline]
    # Seen from +x, (z, y) counter-clockwise is (-z, y) clockwise, so the
    # +x cap takes the outline reversed.
    if caps:
        mesh.face(front[::-1], material)
        mesh.face(back, material)
    n = len(outline)
    for i in range(n):
        j = (i + 1) % n
        mesh.face([front[j], front[i], back[i], back[j]], material)


def box(mesh, lo, hi, material):
    x0, y0, z0 = lo
    x1, y1, z1 = hi
    c = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0],
         [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]]
    for f in ([0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [3, 7, 6, 2], [0, 4, 7, 3], [1, 2, 6, 5]):
        mesh.face([c[i] for i in f], material)


def cylinder(mesh, centre, axis, radius, length, sides, material, caps=True, phase=0.0):
    """A cylinder from `centre` along unit `axis` ('x', 'y' or 'z')."""
    a = 'xyz'.index(axis)
    u, v = [i for i in range(3) if i != a]

    def ring(t):
        out = []
        for k in range(sides):
            ang = phase + 2 * math.pi * k / sides
            p = list(centre)
            p[a] += t
            p[u] += radius * math.cos(ang)
            p[v] += radius * math.sin(ang)
            out.append(p)
        return out
    r0, r1 = ring(0.0), ring(length)
    # Loops run counter-clockwise round +axis when (u, v, a) is right-handed.
    right = (a, u, v) in ((0, 1, 2), (1, 2, 0), (2, 0, 1))
    if not right:
        r0, r1 = r0[::-1], r1[::-1]
    for k in range(sides):
        j = (k + 1) % sides
        mesh.face([r0[k], r0[j], r1[j], r1[k]], material)
    if caps:
        mesh.face(r1, material)
        mesh.face(r0[::-1], material)


def torus(mesh, centre, axis, major, minor, segments, sides, material):
    a = 'xyz'.index(axis)
    u, v = [i for i in range(3) if i != a]
    right = (a, u, v) in ((0, 1, 2), (1, 2, 0), (2, 0, 1))

    def point(i, k):
        t = 2 * math.pi * i / segments
        s = 2 * math.pi * k / sides
        radial = major + minor * math.cos(s)
        p = list(centre)
        p[u] += radial * math.cos(t)
        p[v] += radial * math.sin(t)
        p[a] += minor * math.sin(s)
        return p
    for i in range(segments):
        for k in range(sides):
            quad = [point(i, k), point(i + 1, k), point(i + 1, k + 1), point(i, k + 1)]
            mesh.face(quad if right else quad[::-1], material)


# --- shapes -------------------------------------------------------------

def quat_x(degrees):
    h = math.radians(degrees) / 2
    return [round(math.sin(h), 7), 0.0, 0.0, round(math.cos(h), 7)]


def node(name, parent=None, t=(0.0, 0.0, 0.0), r=(0.0, 0.0, 0.0, 1.0)):
    return {'name': name, 'parent': parent, 'translation': list(t), 'rotation': list(r)}


def material(name):
    return {'name': name, 'wrap_u': True, 'wrap_v': True, 'blend': 'opaque', 'unlit': False,
            'environment': False, 'mipmaps': True, 'detail_map': None, 'bump_map': None,
            'reflectance_map': None, 'detail_scale': 1.0, 'reflectance': 1.0}


def shape(shape_id, nodes, parts, materials, animations):
    """`parts`: (object name, node index, Mesh)."""
    names = [n['name'] for n in nodes]
    return {
        'schema_version': 1,
        'id': shape_id,
        'nodes': nodes,
        'objects': [{'name': name, 'node': names.index(at), 'meshes': [i], 'visibility': 1.0, 'frame': 0,
                     'material_frame': 0} for i, (name, at, _) in enumerate(parts)],
        'details': [{'name': 'detail100', 'pixel_threshold': 100.0, 'object_start': 0,
                     'object_count': len(parts), 'mesh_offset': 0, 'collision': False}],
        'meshes': [m.native() for _, _, m in parts],
        'materials': [material(m) for m in materials],
        'animations': animations,
    }


def animation(name, frames, duration, looping, nodes=(), objects=()):
    return {'name': name, 'frames': frames, 'duration': duration, 'looping': looping, 'additive': False,
            'priority': 0, 'nodes': list(nodes), 'objects': list(objects), 'ground_translations': [],
            'ground_rotations': [], 'triggers': []}


def track(node_name, rotations):
    return {'node': node_name, 'rotations': rotations, 'translations': [], 'scales': [], 'scale_rotations': []}


def world_bounds(nodes, parts, rest_rotations=None):
    """The shape's rest-pose box, every part placed by its node chain
    (translations and rotations about x only, as these shapes use)."""
    def place(index, p):
        while index is not None:
            n = nodes[index]
            qx, _, _, qw = n['rotation']
            ang = 2 * math.atan2(qx, qw)
            c, s = math.cos(ang), math.sin(ang)
            p = [p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c]
            p = v_add(p, n['translation'])
            index = n['parent']
        return p
    names = [n['name'] for n in nodes]
    points = [place(names.index(at), p) for _, at, m in parts for p in m.positions]
    lo = [round(min(p[i] for p in points), 6) for i in range(3)]
    hi = [round(max(p[i] for p in points), 6) for i in range(3)]
    return lo, hi


# --- the butterfly knife ------------------------------------------------

PIVOT = 0.17            # above the hand
PIN = 0.021             # each handle's pin, either side of the blade's centre line
HANDLE_LENGTH = 0.50
HANDLE_WIDTH = 0.034    # across the blade (z)
HANDLE_THICK = 0.05     # through the blade's flat (x)


def knife_blade():
    """The blade hangs down from the pivot (closed); `activate` swings it
    up. Rows down its length: (y, spine z, edge z, full thickness?)."""
    m = Mesh()
    rows = [
        # tang: full thickness, holes for the pins
        (0.026, 0.024, -0.024, True),
        (-0.058, 0.024, -0.024, True),
        # ricasso and choil
        (-0.072, 0.017, -0.019, False),
        (-0.15, 0.017, -0.021, False),
        (-0.25, 0.017, -0.021, False),
        (-0.33, 0.017, -0.017, False),
        (-0.385, 0.009, -0.010, False),
        (-0.43, 0.0, 0.0, False),
    ]
    spine_x, edge_x = 0.0055, 0.0011

    def points(row):
        y, zs, ze, full = row
        zg = zs if full else ze + 0.45 * (zs - ze)
        ex = spine_x if full else edge_x
        if row[0] == -0.43:
            ex = 0.0004
        return {
            'sf': [spine_x, y, zs], 'gf': [spine_x, y, zg], 'ef': [ex, y, ze],
            'sb': [-spine_x, y, zs], 'gb': [-spine_x, y, zg], 'eb': [-ex, y, ze],
        }
    loops = [points(r) for r in rows]
    for a, b in zip(loops, loops[1:]):
        # +x side: flat (spine to grind) then bevel (grind to edge).
        m.face([a['sf'], b['sf'], b['gf'], a['gf']], 'blade')
        m.face([a['gf'], b['gf'], b['ef'], a['ef']], 'blade')
        m.face([a['sb'], a['gb'], b['gb'], b['sb']], 'blade')
        m.face([a['gb'], a['eb'], b['eb'], b['gb']], 'blade')
        m.face([a['sf'], a['sb'], b['sb'], b['sf']], 'blade')
        m.face([a['ef'], b['ef'], b['eb'], a['eb']], 'blade')
    top = loops[0]
    m.face([top['sf'], top['gf'], top['ef'], top['eb'], top['gb'], top['sb']], 'blade')
    # The pins through the tang.
    for z in (-PIN, PIN):
        cylinder(m, [-0.0075, 0.0, z], 'x', 0.007, 0.015, 8, 'pins')
    return m


def knife_handle(latch):
    """One handle, hanging down from its pin at the node's origin."""
    m = Mesh()
    w, t, c = HANDLE_WIDTH / 2, HANDLE_THICK / 2, 0.007
    top, bottom = 0.03, 0.03 - HANDLE_LENGTH
    # Octagonal section in (z, x): the handle's long edges chamfered.
    section = [(w, t - c), (w - c, t), (-w + c, t), (-w, t - c), (-w, -t + c), (-w + c, -t), (w - c, -t), (w, -t + c)]
    upper = [[x, top, z] for z, x in section]
    lower = [[x, bottom, z] for z, x in section]
    # Loops in (z, x) counter-clockwise are clockwise round +y.
    m.quad_strip(lower[::-1], upper[::-1], 'handle')
    m.face(upper[::-1], 'handle')
    m.face(lower, 'handle')
    # Rivets and the pin's head on both flats.
    for side in (1, -1):
        x = side * t
        for y in (-0.11, -0.24, -0.37):
            lo = [min(x, x + side * 0.003), y - 0.007, -0.007]
            hi = [max(x, x + side * 0.003), y + 0.007, 0.007]
            box(m, lo, hi, 'pins')
        cylinder(m, [x if side > 0 else x - 0.004, 0.0, 0.0], 'x', 0.011, 0.004, 6, 'pins')
    if latch:
        # The latch that holds the handles shut, at the end.
        box(m, [-0.006, bottom - 0.012, -w - 0.004], [0.006, bottom + 0.05, -w + 0.004], 'pins')
        box(m, [-0.006, bottom - 0.012, -w - 0.004], [0.006, bottom, w], 'pins')
    return m


def ease(t):
    """The flip: fast off the mark, slowing into the catch."""
    return 1 - (1 - t) ** 2.4


def butterfly_knife():
    nodes = [
        node('Root'),
        node('mountPoint'),
        node('muzzlePoint'),
        node('knife', 0, (0.0, PIVOT, 0.0)),
        node('blade', 3),
        node('handleSafe', 3, (0.0, 0.0, -PIN)),
        node('handleBite', 3, (0.0, 0.0, PIN)),
    ]
    parts = [
        ('blade', 'blade', knife_blade()),
        ('handlesafe', 'handleSafe', knife_handle(False)),
        ('handlebite', 'handleBite', knife_handle(True)),
    ]
    frames = 15
    knife_turn, safe_turn, bite_turn = [], [], []
    for i in range(frames):
        a = 180 * ease(i / (frames - 1))
        knife_turn.append(quat_x(a))
        # The safe handle stays in the hand while the knife turns over it;
        # the bite handle goes the whole way round.
        safe_turn.append(quat_x(-a))
        bite_turn.append(quat_x(a))
    open_pose = [track('knife', [quat_x(180)]), track('handleSafe', [quat_x(-180)]),
                 track('handleBite', [quat_x(180)])]
    animations = [
        animation('activate', frames, 0.4667, False,
                  [track('knife', knife_turn), track('handleSafe', safe_turn), track('handleBite', bite_turn)]),
        animation('ready', 1, 0.0, False, open_pose),
    ]
    model = shape('butterfly-knife:file/models/butterfly-knife.shape.json', nodes, parts,
                  ['handle', 'blade', 'pins'], animations)
    # Material indices are by name above; fix them to list positions.
    return model, world_bounds(nodes, parts)


# --- the HE grenade -----------------------------------------------------

BODY_CENTRE = 0.05
BODY_R = 0.165          # round the middle
BODY_H = 0.15           # top to centre


def grenade_body():
    """A segmented body: raised blocks with grooves between, the classic
    fragmentation grenade's look, and plain caps top and bottom."""
    m = Mesh()
    bands, segments = 7, 12

    def at(lat, lon, scale):
        return [BODY_R * scale * math.sin(lat) * math.cos(lon),
                BODY_CENTRE + BODY_H * scale * math.cos(lat),
                -BODY_R * scale * math.sin(lat) * math.sin(lon)]
    lats = [math.pi * (0.14 + 0.72 * i / bands) for i in range(bands + 1)]
    groove, inset = 0.93, 0.18
    for b in range(bands):
        for s in range(segments):
            l0, l1 = lats[b], lats[b + 1]
            o0, o1 = 2 * math.pi * s / segments, 2 * math.pi * (s + 1) / segments
            dl, do = (l1 - l0) * inset, (o1 - o0) * inset
            outer = [at(l0 + dl, o0 + do, 1.0), at(l1 - dl, o0 + do, 1.0),
                     at(l1 - dl, o1 - do, 1.0), at(l0 + dl, o1 - do, 1.0)]
            corner = [at(l0, o0, groove), at(l1, o0, groove), at(l1, o1, groove), at(l0, o1, groove)]
            m.face(outer, 'body')
            for k in range(4):
                j = (k + 1) % 4
                m.face([corner[k], corner[j], outer[j], outer[k]], 'body')
    # Caps: a dome of the groove radius over each pole.
    for lat_edge, pole in ((lats[0], 0.0), (lats[-1], math.pi)):
        ring = [at(lat_edge, 2 * math.pi * s / segments, groove) for s in range(segments)]
        tip = at(pole, 0.0, groove)
        for s in range(segments):
            a, b = ring[s], ring[(s + 1) % segments]
            m.face([tip, a, b] if pole == 0.0 else [tip, b, a], 'body')
    return m


def grenade_fuze():
    m = Mesh()
    top = BODY_CENTRE + BODY_H
    cylinder(m, [0.0, top - 0.03, 0.0], 'y', 0.058, 0.075, 10, 'metal', phase=math.pi / 10)
    cylinder(m, [0.0, top + 0.045, 0.0], 'y', 0.064, 0.016, 10, 'metal', phase=math.pi / 10)
    return m


def grenade_spoon():
    """The lever, from the fuze's top down the body's back (+z)."""
    m = Mesh()
    w, thick = 0.023, 0.006
    top = BODY_CENTRE + BODY_H + 0.061
    path = [(top, 0.0), (top + 0.004, 0.045), (top - 0.02, 0.08)]
    for deg in (35, 55, 75, 95, 112):
        a = math.radians(deg)
        path.append((BODY_CENTRE + (BODY_H + 0.016) * math.cos(a), (BODY_R + 0.016) * math.sin(a)))
    # Each path point gets an outward normal in (y, z), and the strip has
    # thickness along it.
    for (y0, z0), (y1, z1) in zip(path, path[1:]):
        d = normalize([0.0, y1 - y0, z1 - z0])
        n = [0.0, -d[2], d[1]]  # outward: up and back
        if n[1] * 0.5 + n[2] < 0:
            n = v_scale(n, -1)
        a, b = [0.0, y0, z0], [0.0, y1, z1]
        corners = []
        for p in (a, b):
            for side in (-w, w):
                for out in (0.0, thick):
                    corners.append(v_add([side, p[1], p[2]], v_scale(n, out)))
        # corners: index = point*4 + side*2 + out
        def c(pi, si, oi):
            return corners[pi * 4 + si * 2 + oi]
        m.face([c(0, 0, 1), c(1, 0, 1), c(1, 1, 1), c(0, 1, 1)], 'metal')   # outer
        m.face([c(0, 0, 0), c(0, 1, 0), c(1, 1, 0), c(1, 0, 0)], 'metal')   # inner
        m.face([c(0, 0, 0), c(1, 0, 0), c(1, 0, 1), c(0, 0, 1)], 'metal')   # side
        m.face([c(0, 1, 0), c(0, 1, 1), c(1, 1, 1), c(1, 1, 0)], 'metal')   # side
    return m


def grenade_pin():
    """The pin through the fuze and its pull ring, at the node's origin
    (the fuze's left side)."""
    m = Mesh()
    cylinder(m, [-0.004, 0.0, 0.0], 'x', 0.0055, 0.13, 6, 'ring')
    torus(m, [-0.01, -0.034, 0.0], 'x', 0.032, 0.0055, 14, 5, 'ring')
    return m


PIN_AT = (-0.068, BODY_CENTRE + BODY_H + 0.012, 0.0)


def he_grenade():
    nodes = [
        node('Root'),
        node('mountPoint', None, (0.05, 0.0, 0.13)),
        node('muzzlePoint', None, (0.0, BODY_CENTRE + 0.02, 0.0)),
        node('ejectPoint', None, PIN_AT),
        node('grenade', 0),
        node('pin', 0, PIN_AT),
    ]
    parts = [
        ('body', 'grenade', grenade_body()),
        ('fuze', 'grenade', grenade_fuze()),
        ('spoon', 'grenade', grenade_spoon()),
        ('pin', 'pin', grenade_pin()),
    ]
    pin_object = 3
    animations = [
        animation('pinpull', 1, 0.0, False, objects=[
            {'object': pin_object, 'visibility': [0.0], 'frames': [], 'material_frames': []}]),
        animation('ready', 1, 0.0, False, objects=[
            {'object': pin_object, 'visibility': [1.0], 'frames': [], 'material_frames': []}]),
        # Out of the hand: the arm follows through empty before the
        # grenade is put away.
        animation('thrown', 1, 0.0, False, objects=[
            {'object': i, 'visibility': [0.0], 'frames': [], 'material_frames': []} for i in range(len(parts))]),
    ]
    held = shape('he-grenade:file/models/he-grenade.shape.json', nodes, parts, ['body', 'metal', 'ring'],
                 animations)
    held_bounds = world_bounds(nodes, parts)

    # In flight: the same body without its pin, turning about its centre.
    thrown_nodes = [node('Root'), node('grenade', 0)]
    shift = [0.0, -BODY_CENTRE, 0.0]
    thrown_parts = []
    for name, _, mesh in parts[:3]:
        moved = Mesh()
        moved.normals = mesh.normals
        moved.uv = mesh.uv
        moved.triangles = mesh.triangles
        moved.positions = [round3(v_add(p, shift)) for p in mesh.positions]
        thrown_parts.append((name, 'grenade', moved))
    frames = 19
    tumble = [quat_x(-360 * i / frames) for i in range(frames)]
    thrown = shape('he-grenade:file/models/he-grenade-thrown.shape.json', thrown_nodes, thrown_parts,
                   ['body', 'metal'], [animation('activate', frames, 0.6333, True, [track('grenade', tumble)])])
    thrown_bounds = world_bounds(thrown_nodes, thrown_parts)

    pin_parts = [('pin', 'Root', grenade_pin())]
    pin = shape('he-grenade:file/models/he-grenade-pin.shape.json', [node('Root')], pin_parts, ['ring'], [])
    pin_bounds = world_bounds([node('Root')], pin_parts)
    return (held, held_bounds), (thrown, thrown_bounds), (pin, pin_bounds)


def bind_materials(model):
    """Primitives were keyed by material name; the format wants indices."""
    names = [m['name'] for m in model['materials']]
    for mesh in model['meshes']:
        for p in mesh['primitives']:
            p['material'] = names.index(p['material'])
        mesh['primitives'].sort(key=lambda p: p['material'])
    return model


# --- sounds -------------------------------------------------------------

def click(rng, seconds, pitch, decay, level=1.0):
    """A small metal part striking another: a sharp tick of noise ringing
    on a few inharmonic partials."""
    n = int(seconds * RATE)
    partials = [(pitch * r, 1.0 / (k + 1)) for k, r in enumerate((1.0, 1.47, 2.09, 2.76))]
    out = []
    for i in range(n):
        t = i / RATE
        env = math.exp(-t / decay)
        tone = sum(a * math.sin(2 * math.pi * f * t) for f, a in partials)
        tick = rng.uniform(-1, 1) * math.exp(-t / 0.0015)
        out.append(level * env * (0.55 * tone + tick))
    return out


def mix(length, parts):
    out = [0.0] * int(length * RATE)
    for start, samples in parts:
        s = int(start * RATE)
        for i, v in enumerate(samples):
            if s + i < len(out):
                out[s + i] += v
    return out


def flip_sound():
    rng = random.Random(1707)
    parts = []
    # The handles rattling round the pins as the blade swings over...
    t = 0.012
    while t < 0.33:
        parts.append((t, click(rng, 0.05, rng.uniform(3600, 5600), 0.008, rng.uniform(0.25, 0.55))))
        t += rng.uniform(0.018, 0.04)
    # ...the pins' rasp under it...
    rasp = [rng.uniform(-1, 1) * 0.12 * math.sin(math.pi * i / (0.3 * RATE)) for i in range(int(0.3 * RATE))]
    parts.append((0.03, rasp))
    # ...and the handles clacking shut on the catch.
    parts.append((0.36, click(rng, 0.25, 2900, 0.03, 1.0)))
    parts.append((0.372, click(rng, 0.2, 4100, 0.02, 0.6)))
    return wav(lowpass(mix(0.62, parts), 9000))


def lowpass(samples, cutoff):
    a = 1.0 - math.exp(-2 * math.pi * cutoff / RATE)
    out, y = [], 0.0
    for x in samples:
        y += a * (x - y)
        out.append(y)
    return out


def swing_sound():
    """Air parted by a blade: noise through a band that rises and falls."""
    rng = random.Random(2009)
    n = int(0.22 * RATE)
    out, lo, band = [], 0.0, 0.0
    for i in range(n):
        p = i / n
        centre = 700 + 2600 * math.sin(math.pi * p) ** 1.5
        a = 1.0 - math.exp(-2 * math.pi * centre / RATE)
        x = rng.uniform(-1, 1)
        lo += a * (x - lo)
        band += 0.5 * (lo - band)
        env = math.sin(math.pi * min(1.0, p * 1.15)) ** 2
        out.append((lo - band) * env)
    return wav(out)


def pin_sound():
    rng = random.Random(97)
    parts = [
        (0.0, click(rng, 0.06, 5200, 0.006, 0.7)),   # the pin sliding out
        (0.09, click(rng, 0.25, 3300, 0.05, 1.0)),   # the ring's clink
        (0.10, click(rng, 0.2, 4700, 0.03, 0.4)),
    ]
    return wav(mix(0.4, parts))


def bounce_sound():
    """A heavy steel body knocking on the ground, twice."""
    rng = random.Random(2008)

    def knock(level):
        n = int(0.3 * RATE)
        out = []
        for i in range(n):
            t = i / RATE
            body = (math.sin(2 * math.pi * 420 * t) + 0.6 * math.sin(2 * math.pi * 655 * t)
                    + 0.3 * math.sin(2 * math.pi * 1130 * t)) * math.exp(-t / 0.045)
            thud = rng.uniform(-1, 1) * math.exp(-t / 0.004)
            out.append(level * (0.7 * body + thud))
        return lowpass(out, 3500)
    return wav(mix(0.42, [(0.0, knock(1.0)), (0.045, knock(0.6))]))


# --- packs --------------------------------------------------------------

def state(name, **fields):
    return {'name': name, **fields}


def butterfly_knife_pack():
    return {
        'schema_version': 3,
        'id': 'butterfly-knife',
        'items': {
            'butterfly-knife:weapon/butterflyknife': {
                'name': 'ButterflyKnifeItem',
                'ui_name': 'Butterfly Knife',
                'image': 'butterfly-knife:image/butterflyknife',
                'model': MODEL_KNIFE,
                'icon': 'icons/butterfly_knife',
            }
        },
        'images': {
            'butterfly-knife:image/butterflyknife': {
                'name': 'ButterflyKnifeImage',
                'model': MODEL_KNIFE,
                'projectile': 'butterfly-knife:projectile/stab',
                'correct_muzzle': True,
                'arm_ready': True,
                'color': [1.0, 1.0, 1.0, 1.0],
                'states': [
                    state('Activate', ticks=60, timeout=1, sequence='activate', sound='butterfly-knife:flip'),
                    state('Ready', down=2, sequence='ready'),
                    state('Charge', ticks=84, wait=False, timeout=5, up=3, allow_change=False,
                          sequence='ready', holder_sequence='spearReady'),
                    state('Jab', ticks=24, timeout=4, script='onFire', sequence='ready',
                          projectile='butterfly-knife:projectile/jab', holder_sequence='armattack',
                          sound='butterfly-knife:swing'),
                    state('StopFire', ticks=24, timeout=1, allow_change=False, holder_sequence='root'),
                    state('Armed', up=6, allow_change=False),
                    state('Stab', ticks=24, timeout=1, script='onFire', sequence='ready', allow_change=False,
                          holder_sequence='spearThrow', sound='butterfly-knife:swing'),
                ],
            }
        },
        'projectiles': {
            'butterfly-knife:projectile/jab': knife_projectile('ButterflyKnifeJabProjectile', 30.0),
            'butterfly-knife:projectile/stab': knife_projectile('ButterflyKnifeStabProjectile', 100.0),
        },
        'damage_types': {
            'butterflyknife': {
                'name': 'ButterflyKnife',
                'suicide_message': '%1 cut themselves',
                'murder_message': '%2 knifed %1',
                'vehicle_scale': 1.0,
                'direct': True,
            }
        },
        'sounds': {
            'butterfly-knife:flip': {'file': 'sounds/flip.wav', 'volume': 0.8},
            'butterfly-knife:swing': {'file': 'sounds/swing.wav', 'volume': 0.5},
        },
    }


def knife_projectile(name, damage):
    """An unseen cut a few units long (50 u/s for 0.1 s) that strikes with
    the Sword's hit: its sound, shake and sparks."""
    return {'name': name, 'speed': 50.0, 'inherit': 1.0, 'gravity': 0.0, 'lifetime_ticks': 12,
            'fade_ticks': 8, 'elasticity': 0.0, 'friction': 0.0, 'damage': damage,
            'damage_type': '$DamageType::ButterflyKnife', 'radius_damage_type': '$DamageType::ButterflyKnife',
            'explosion': {'effect': 'swordExplosion'}}


def he_grenade_pack():
    return {
        'schema_version': 3,
        'id': 'he-grenade',
        'items': {
            'he-grenade:weapon/hegrenade': {
                'name': 'HEGrenadeItem',
                'ui_name': 'HE-Grenade',
                'image': 'he-grenade:image/hegrenade',
                'model': MODEL_GRENADE,
                'icon': 'icons/he_grenade',
            }
        },
        'images': {
            'he-grenade:image/hegrenade': {
                'name': 'HEGrenadeImage',
                'model': MODEL_GRENADE,
                'projectile': 'he-grenade:projectile/hegrenade',
                'correct_muzzle': True,
                'arm_ready': True,
                'color': [1.0, 1.0, 1.0, 1.0],
                'casing': 'HEGrenadePinDebris',
                'states': [
                    state('Activate', ticks=12, timeout=1, sequence='ready', sound='weaponSwitchSound'),
                    state('Ready', down=2),
                    state('PinPull', ticks=24, timeout=3, allow_change=False, sequence='pinpull',
                          sound='he-grenade:pin', eject_shell=True),
                    state('PinOut', down=4, allow_change=False),
                    state('Charge', ticks=84, wait=False, timeout=6, up=5, allow_change=False,
                          holder_sequence='spearReady'),
                    state('AbortCharge', ticks=36, timeout=3, allow_change=False, holder_sequence='root'),
                    state('Armed', up=7, allow_change=False),
                    # The arm follows through with an empty hand, then the
                    # grenade is used up (putting the image away sooner
                    # would cut the throw short).
                    state('Throw', ticks=30, timeout=8, script='onFire', allow_change=False,
                          sequence='thrown', holder_sequence='spearThrow'),
                    state('Thrown', use_up=True),
                ],
            }
        },
        'projectiles': {
            'he-grenade:projectile/hegrenade': {
                'name': 'HEGrenadeProjectile',
                'model': MODEL_GRENADE_THROWN,
                'speed': 30.0, 'inherit': 0.0, 'gravity': 1.0,
                # The fuse: 2.5 s from the throw, whatever it hits.
                'lifetime_ticks': 300, 'fade_ticks': 360, 'arm_ticks': 300,
                'ballistic': True, 'elasticity': 0.4, 'friction': 0.3,
                'explode_death': True,
                'damage': 0.0, 'impulse': 200.0, 'vertical': 200.0,
                'damage_type': '$DamageType::HEGrenade', 'radius_damage_type': '$DamageType::HEGrenade',
                'explosion': {'effect': 'heGrenadeExplosion', 'damage': 250.0, 'radius': 17.0,
                              'impulse': 4000.0, 'impulse_radius': 20.0},
                'brick': {'radius': 10.0, 'direct': False, 'force': 25.0, 'max_volume': 100.0,
                          'max_floating_volume': 60.0},
                'bounce_effect': 'heGrenadeBounce',
            }
        },
        'damage_types': {
            'hegrenade': {
                'name': 'HEGrenade',
                'suicide_message': '%1 held on too long',
                'murder_message': '%2 blew up %1',
                'vehicle_scale': 1.0,
                'direct': False,
            }
        },
        'explosions': {
            'hegrenadeexplosion': {
                'name': 'heGrenadeExplosion',
                # The base game's vehicle explosion and the Rocket
                # Launcher's fireball, both by reference.
                'sound': 'vehicleExplosionSound',
                'shake': {'frequency': [7.0, 8.0, 7.0], 'amplitude': [1.0, 1.0, 1.0], 'seconds': 0.5,
                          'radius': 15.0, 'falloff': 10.0},
                'shape': 'Add-Ons/Weapon_Rocket_Launcher/explosionSphere1.dts',
                'seconds': 0.15, 'play_speed': 1.0, 'face_viewer': True, 'scale': [1.6, 1.6, 1.6],
                'sizes': [],
            },
            'hegrenadebounce': {
                'name': 'heGrenadeBounce', 'sound': 'he-grenade:bounce', 'shake': None, 'shape': '',
                'seconds': 0.3, 'play_speed': 1.0, 'face_viewer': True, 'scale': [1.0, 1.0, 1.0], 'sizes': [],
            },
        },
        'sounds': {
            'he-grenade:pin': {'file': 'sounds/pin.wav', 'volume': 0.7},
            'he-grenade:bounce': {'file': 'sounds/bounce.wav', 'volume': 0.8},
        },
        'effects': grenade_effects(),
        # The pin that flies off as it is pulled: a casing, in the
        # `DebrisData` fields Torque images eject (`stateEjectShell`).
        'definitions': [
            {'name': 'HEGrenadePinDebris', 'class': 'DebrisData', 'parent': None,
             'source': {'path': 'he-grenade/weapons.json', 'sha256': '', 'line': 0},
             'fields': {'shapefile': MODEL_GRENADE_PIN, 'lifetime': '4.0', 'minspinspeed': '-400',
                        'maxspinspeed': '200', 'elasticity': '0.5', 'friction': '0.2', 'numbounces': '3',
                        'staticonmaxbounce': '1', 'fade': '1', 'gravmodifier': '2'}},
            {'name': 'HEGrenadeImage', 'class': 'ShapeBaseImageData', 'parent': None,
             'source': {'path': 'he-grenade/weapons.json', 'sha256': '', 'line': 0},
             'fields': {'shellexitdir': '-2.0 1.0 1.0', 'shellexitvariance': '15.0', 'shellvelocity': '7.0'}},
        ],
    }


def particle(pid, texture, alpha, lifetime, variance, keys, drag=0.0, gravity=0.0, spin=(0.0, 0.0)):
    return {'id': pid, 'texture': texture, 'alpha_blend': alpha, 'lifetime': lifetime,
            'lifetime_variance': variance, 'drag': drag, 'wind': 0.0, 'gravity': gravity,
            'inherited_velocity': 0.0, 'acceleration': 0.0, 'spin_degrees': 0.0, 'random_spin': list(spin),
            'keys': [{'time': t, 'color': c, 'size': s} for t, c, s in keys]}


def emitter(eid, particles, period, speed, speed_variance, theta, lifetime, offset=0.0):
    return {'id': eid, 'name': '', 'particles': particles, 'period': period, 'period_variance': 0.0,
            'speed': speed, 'speed_variance': speed_variance, 'offset': offset, 'offset_variance': 0.0,
            'theta_degrees': list(theta), 'phi_rate_degrees': 0.0, 'phi_variance_degrees': 360.0,
            'lifetime': lifetime, 'lifetime_variance': 0.0, 'orient': False, 'orient_on_velocity': True,
            'override_advance': False, 'use_emitter_colors': False, 'use_emitter_sizes': False,
            'use_placement_velocity': False, 'node_time_scale': 1.0, 'point_node_time_scale': 1.0}


def grenade_effects():
    """Our own blast: a fireball, a ring of smoke hugging the ground and a
    column rising, and clods of dirt thrown up; a puff of dust on each
    bounce. Textures are the base game's particle sprites."""
    cloud, chunk = 'base/data/particles/cloud', 'base/data/particles/chunk'
    p = 'he-grenade:particle/'
    e = 'he-grenade:emitter/'
    return {
        'particles': [
            particle(p + 'fire', cloud, False, 0.45, 0.2, [
                (0.0, [1.0, 0.9, 0.5, 1.0], 2.0), (0.35, [1.0, 0.5, 0.12, 0.7], 4.5),
                (1.0, [0.35, 0.1, 0.03, 0.0], 6.0)], drag=3.0, gravity=-0.4, spin=(-90.0, 90.0)),
            particle(p + 'smoke', cloud, True, 3.0, 1.5, [
                (0.0, [0.12, 0.12, 0.12, 0.0], 2.5), (0.06, [0.2, 0.2, 0.19, 0.85], 6.0),
                (1.0, [0.42, 0.41, 0.4, 0.0], 12.0)], drag=1.6, gravity=-0.15, spin=(-40.0, 40.0)),
            particle(p + 'dirt', chunk, True, 1.1, 0.5, [
                (0.0, [0.13, 0.11, 0.09, 1.0], 0.45), (1.0, [0.2, 0.18, 0.15, 0.0], 0.15)],
                drag=0.1, gravity=4.0, spin=(-300.0, 300.0)),
            particle(p + 'dust', cloud, True, 0.6, 0.25, [
                (0.0, [0.5, 0.47, 0.42, 0.45], 0.25), (1.0, [0.6, 0.58, 0.52, 0.0], 1.0)],
                drag=3.0, gravity=-0.05, spin=(-60.0, 60.0)),
        ],
        'emitters': [
            emitter(e + 'fire', [p + 'fire'], 0.004, 9.0, 4.0, (0.0, 180.0), 0.07, offset=0.6),
            emitter(e + 'smokering', [p + 'smoke'], 0.012, 16.0, 5.0, (80.0, 90.0), 0.14),
            emitter(e + 'smokecolumn', [p + 'smoke'], 0.025, 6.0, 2.0, (0.0, 35.0), 0.2),
            emitter(e + 'dirt', [p + 'dirt'], 0.002, 24.0, 8.0, (0.0, 70.0), 0.03),
            emitter(e + 'dust', [p + 'dust'], 0.012, 1.6, 0.8, (20.0, 90.0), 0.05),
        ],
        'explosions': [
            {'id': 'he-grenade:explosion/heGrenadeExplosion', 'lifetime': 0.5,
             'emitters': [e + 'fire', e + 'smokering', e + 'smokecolumn', e + 'dirt'],
             'burst': [e + 'smokecolumn', 14, 1.2]},
            {'id': 'he-grenade:explosion/heGrenadeBounce', 'lifetime': 0.2, 'emitters': [e + 'dust']},
        ],
    }


MODEL_KNIFE = 'butterfly-knife/models/butterfly-knife.shape.json'
MODEL_GRENADE = 'he-grenade/models/he-grenade.shape.json'
MODEL_GRENADE_THROWN = 'he-grenade/models/he-grenade-thrown.shape.json'
MODEL_GRENADE_PIN = 'he-grenade/models/he-grenade-pin.shape.json'


def presentation(folder, pack_id, weapons, models, textures, item_ids, image_ids, projectiles):
    """`models`: key -> (file, shape, bounds, texture keys); `textures`:
    key -> (file, png bytes)."""
    weapons_bytes = dump(weapons)
    weapons_sha = write(folder / 'weapons.json', weapons_bytes)
    model_entries = {}
    for key, (file, model, bounds, bound_textures) in models.items():
        data = (json.dumps(model, separators=(',', ':')) + '\n').encode()
        sha = write(folder / file, data)
        model_entries[key] = {'file': file, 'sha256': sha, 'source': key, 'source_sha256': sha,
                              'textures': bound_textures, 'bounds_min': bounds[0], 'bounds_max': bounds[1]}
    texture_entries = {}
    for key, (file, data) in textures.items():
        texture_entries[key] = {'file': file, 'sha256': write(folder / file, data), 'width': 16,
                                'height': 16, 'source': key}
    evidence = {'path': f'{pack_id}/weapons.json', 'sha256': weapons_sha, 'line': 0}
    items, physics = {}, {}
    for item_id in item_ids:
        item = weapons['items'][item_id]
        items[item_id] = {'model': item['model'].lower(), 'image': item['image'], 'tint': [1.0, 1.0, 1.0, 1.0],
                          'icon': None, 'evidence': evidence}
        b = model_entries[item['model'].lower()]
        physics[item_id] = {'min': b['bounds_min'], 'max': b['bounds_max']}
    images = {}
    for image_id in image_ids:
        image = weapons['images'][image_id]
        images[image_id] = {'model': image['model'].lower(), 'mount_point': 0, 'offset': [0.0, 0.0, 0.0],
                            'eye_offset': [0.0, 0.0, 0.0], 'source_rotation_degrees': [0.0, 0.0, 0.0],
                            'eye_rotation_degrees': [0.0, 0.0, 0.0], 'tint': [1.0, 1.0, 1.0, 1.0],
                            'evidence': evidence}
    projectile_entries = {pid: {'model': (weapons['projectiles'][pid].get('model') or '').lower() or None,
                                'tint': [1.0, 1.0, 1.0, 1.0]} for pid in projectiles}
    physics_bytes = dump({'schema_version': 1, 'items': physics})
    physics_sha = write(folder / 'item-physics.json', physics_bytes)
    manifest = {
        'schema_version': 2, 'id': f'{pack_id}:item-presentation/main',
        'weapons_sha256': weapons_sha, 'item_physics_sha256': physics_sha,
        'models': model_entries, 'textures': texture_entries, 'items': items, 'images': images,
        'projectiles': projectile_entries, 'diagnostics': [],
    }
    write(folder / 'presentation.json', dump(manifest))


def main():
    knife_dir = ROOT / 'butterfly-knife' / 'assets'
    model, bounds = butterfly_knife()
    model = bind_materials(model)
    knife_textures = {
        'butterfly-knife/textures/handle.png': ('textures/handle.png', flat_texture([0.16, 0.16, 0.17], 1, 0.015)),
        'butterfly-knife/textures/blade.png': ('textures/blade.png', flat_texture([0.62, 0.63, 0.65], 2, 0.03)),
        'butterfly-knife/textures/pins.png': ('textures/pins.png', flat_texture([0.74, 0.74, 0.72], 3, 0.0)),
    }
    pack = butterfly_knife_pack()
    presentation(knife_dir, 'butterfly-knife', pack,
                 {MODEL_KNIFE: ('models/butterfly-knife.shape.json', model, bounds,
                                list(knife_textures))},
                 knife_textures, list(pack['items']), list(pack['images']), list(pack['projectiles']))
    write(knife_dir / 'sounds' / 'flip.wav', flip_sound())
    write(knife_dir / 'sounds' / 'swing.wav', swing_sound())
    write(knife_dir / 'icons' / 'butterfly_knife.render.json', dump(
        {'schema_version': 1, 'pose_like': 'v20.weapon.sworditem', 'look': {'base': [1.0, 1.0, 1.0],
                                                                              'materials': True}}))

    grenade_dir = ROOT / 'he-grenade' / 'assets'
    (held, held_bounds), (thrown, thrown_bounds), (pin, pin_bounds) = he_grenade()
    held, thrown, pin = bind_materials(held), bind_materials(thrown), bind_materials(pin)
    green = 'he-grenade/textures/body.png'
    metal = 'he-grenade/textures/metal.png'
    ring = 'he-grenade/textures/ring.png'
    grenade_textures = {
        green: ('textures/body.png', flat_texture([0.12, 0.33, 0.17], 4, 0.02)),
        metal: ('textures/metal.png', flat_texture([0.70, 0.69, 0.62], 5, 0.015)),
        ring: ('textures/ring.png', flat_texture([0.55, 0.55, 0.56], 6, 0.0)),
    }
    pack = he_grenade_pack()
    presentation(grenade_dir, 'he-grenade', pack, {
        MODEL_GRENADE: ('models/he-grenade.shape.json', held, held_bounds, [green, metal, ring]),
        MODEL_GRENADE_THROWN: ('models/he-grenade-thrown.shape.json', thrown, thrown_bounds, [green, metal]),
        MODEL_GRENADE_PIN: ('models/he-grenade-pin.shape.json', pin, pin_bounds, [ring]),
    }, grenade_textures, list(pack['items']), list(pack['images']), list(pack['projectiles']))
    write(grenade_dir / 'sounds' / 'pin.wav', pin_sound())
    write(grenade_dir / 'sounds' / 'bounce.wav', bounce_sound())
    write(grenade_dir / 'icons' / 'he_grenade.render.json', dump(
        {'schema_version': 1, 'pose_like': 'v20.weapon.gunitem', 'look': {'base': [1.0, 1.0, 1.0],
                                                                            'materials': True}}))
    print('knife', bounds, 'grenade', held_bounds, thrown_bounds, pin_bounds)


if __name__ == '__main__':
    main()
