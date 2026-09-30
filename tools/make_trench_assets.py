"""Write the Trench Warfare Add-Ons' sounds and the Trench Pick's fallback
icon: original, made here, so they carry no one else's rights.

Outputs, under packages/trench-warfare/trench-kit/assets/:
  sounds/dig.wav      the pick biting into soil: a gritty scrape over a
                      dull knock (16-bit mono, 22050 Hz)
  sounds/place.wav    a cube of dirt patted down: a soft low thump
  sounds/whistle.wav  the officer's whistle that ends the ceasefire: a
                      trilling pea whistle, two short blasts and a long one
  models/trench_pick.shape.json  the Trench Pick: an ash handle with a
                      leather grip, an iron head with a pick point in front
                      and an adze blade behind, and a first-person swing
  models/pick_wood.png, pick_grip.png, pick_iron.png  its textures
  icons/trench_pick.png  a 128x128 pick drawn in the stock icons' manner
                      (a small shaded model on a clear background, pointing
                      up and to the right), shown only when the game cannot
                      draw the icon from the tool's own model
                      (trench_pick.render.json)

Run it again after changing a recipe below; the output is the same every
run (fixed seed, Python standard library only).
"""
import json
import math
import random
import struct
import wave
import zlib
from pathlib import Path

RATE = 22050
ROOT = Path(__file__).resolve().parent.parent / 'packages' / 'trench-warfare' / 'trench-kit' / 'assets'
rng = random.Random(19160701)


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
    low = lowpass(samples, cutoff)
    return [x - l for x, l in zip(samples, low)]


def sweep(seconds, f0, f1):
    out, phase = [], 0.0
    n = int(seconds * RATE)
    for i in range(n):
        f = f0 * (f1 / f0) ** (i / n)
        phase += 2 * math.pi * f / RATE
        out.append(math.sin(phase))
    return out


def envelope(samples, attack, decay):
    out = []
    for i, x in enumerate(samples):
        t = i / RATE
        a = min(1.0, t / attack) if attack > 0 else 1.0
        out.append(x * a * math.exp(-t / decay))
    return out


def mix(*tracks):
    n = max(len(t) for t, _ in tracks)
    out = [0.0] * n
    for track, gain in tracks:
        for i, x in enumerate(track):
            out[i] += x * gain
    return out


def fade_out(samples, seconds=0.03):
    n = int(seconds * RATE)
    for i in range(min(n, len(samples))):
        samples[-1 - i] *= i / n
    return samples


def write(path, samples, peak=0.85):
    top = max(1e-6, max(abs(x) for x in samples))
    scale = peak / top
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), 'wb') as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(b''.join(struct.pack('<h', int(max(-1, min(1, x * scale)) * 32767)) for x in samples))


def dig():
    # Grit: noise in the soil's band, grains made by gating it unevenly.
    grit = highpass(lowpass(noise(0.32), 3200), 400)
    gate = lowpass([1.0 if rng.random() < 0.35 else 0.15 for _ in grit], 900)
    scrape = envelope([g * x for g, x in zip(gate, grit)], 0.004, 0.09)
    knock = envelope(sweep(0.32, 150, 70), 0.002, 0.045)
    crumble = envelope(lowpass(noise(0.32), 500), 0.02, 0.12)
    return fade_out(mix((scrape, 1.0), (knock, 0.8), (crumble, 0.5)))


def place():
    body = envelope(sweep(0.3, 95, 50), 0.003, 0.07)
    pat = envelope(lowpass(noise(0.3), 900), 0.002, 0.035)
    settle = envelope(highpass(lowpass(noise(0.3), 2500), 600), 0.03, 0.06)
    return fade_out(mix((body, 1.0), (pat, 0.6), (settle, 0.15)))


def whistle():
    out = []
    # Two short blasts and a long one, as an officer's whistle.
    for length, gap in ((0.16, 0.08), (0.16, 0.1), (0.75, 0.0)):
        n = int(length * RATE)
        phase = 0.0
        breath = highpass(lowpass(noise(length), 6000), 1500)
        for i in range(n):
            t = i / RATE
            # The pea rattles: the pitch and loudness warble about 28 times a second.
            trill = math.sin(2 * math.pi * 28 * t)
            f = 2850 + 110 * trill
            phase += 2 * math.pi * f / RATE
            tone = math.sin(phase) + 0.25 * math.sin(2 * phase)
            level = (0.7 + 0.3 * trill) * min(1.0, t / 0.01) * min(1.0, (length - t) / 0.03)
            out.append((tone * 0.8 + breath[i] * 0.35) * level)
        out.extend([0.0] * int(gap * RATE))
    return out


# ---- The model ----
#
# Native model axes (bri_content::shape): x right, y up, -z forward. The
# hand holds the grip at the origin (the `mountPoint` node); the handle
# stands up out of the fist and the head lies across its top, pick point
# forward, as the stock hammer is held. Faces are flat shaded, in the
# stock tools' low-poly manner.

def v_add(a, b):
    return [a[0] + b[0], a[1] + b[1], a[2] + b[2]]


def v_sub(a, b):
    return [a[0] - b[0], a[1] - b[1], a[2] - b[2]]


def v_scale(a, s):
    return [a[0] * s, a[1] * s, a[2] * s]


def v_cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def v_dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def v_unit(a):
    n = math.sqrt(v_dot(a, a))
    return [a[0] / n, a[1] / n, a[2] / n]


class Part:
    """Flat-shaded faces of one material, facing out from `inside`."""

    def __init__(self):
        self.positions, self.normals, self.uv, self.triangles = [], [], [], []

    def face(self, corners, inside, uvs):
        normal = v_unit(v_cross(v_sub(corners[1], corners[0]), v_sub(corners[2], corners[0])))
        centre = v_scale([sum(c[k] for c in corners) for k in range(3)], 1 / len(corners))
        if v_dot(normal, v_sub(centre, inside)) < 0:
            corners, uvs, normal = corners[::-1], uvs[::-1], v_scale(normal, -1)
        base = len(self.positions)
        for c, t in zip(corners, uvs):
            self.positions.append([round(x, 6) for x in c])
            self.normals.append([round(x, 6) for x in normal])
            self.uv.append([round(x, 6) for x in t])
        for i in range(1, len(corners) - 1):
            self.triangles.append([base, base + i, base + i + 1])


def prism(part, rings, sides, uv_scale=1.0):
    """A tube through `rings` [(centre, radius)] along y, capped both ends."""
    angles = [2 * math.pi * (k + 0.5) / sides for k in range(sides)]
    points = [[[c[0] + r * math.cos(a), c[1], c[2] + r * math.sin(a)] for a in angles] for c, r in rings]
    for i in range(len(rings) - 1):
        inside = v_scale(v_add(rings[i][0], rings[i + 1][0]), 0.5)
        for k in range(sides):
            j = (k + 1) % sides
            quad = [points[i][k], points[i][j], points[i + 1][j], points[i + 1][k]]
            u0, u1 = k / sides, (k + 1) / sides
            v0, v1 = rings[i][0][1] * uv_scale, rings[i + 1][0][1] * uv_scale
            part.face(quad, inside, [[u0, v0], [u1, v0], [u1, v1], [u0, v1]])
    for end, towards in ((0, 1), (len(rings) - 1, -1)):
        inside = rings[end + towards][0]
        cap = points[end]
        part.face(cap, v_add(rings[end][0], v_scale(v_sub(inside, rings[end][0]), 0.01)),
                  [[0.5 + 0.5 * math.cos(a), 0.5 + 0.5 * math.sin(a)] for a in angles])


def bar(part, sections):
    """A bar through `sections` [(centre, half width x, half height y)]
    along z, capped at both ends (a zero-size end comes to a point)."""
    def corners(c, hw, hh):
        return [[c[0] - hw, c[1] - hh, c[2]], [c[0] + hw, c[1] - hh, c[2]],
                [c[0] + hw, c[1] + hh, c[2]], [c[0] - hw, c[1] + hh, c[2]]]
    rings = [corners(*s) for s in sections]
    for i in range(len(rings) - 1):
        inside = v_scale(v_add(sections[i][0], sections[i + 1][0]), 0.5)
        for k in range(4):
            j = (k + 1) % 4
            quad = [rings[i][k], rings[i][j], rings[i + 1][j], rings[i + 1][k]]
            z0, z1 = sections[i][0][2] * 2, sections[i + 1][0][2] * 2
            uvs = [[k * 0.25, z0], [(k + 1) * 0.25, z0], [(k + 1) * 0.25, z1], [k * 0.25, z1]]
            if all(v_dot(v_sub(a, b), v_sub(a, b)) < 1e-12 for a, b in ((quad[0], quad[1]), (quad[2], quad[3]))):
                continue
            if v_dot(v_sub(quad[2], quad[3]), v_sub(quad[2], quad[3])) < 1e-12:
                part.face(quad[:3], inside, uvs[:3])
            elif v_dot(v_sub(quad[0], quad[1]), v_sub(quad[0], quad[1])) < 1e-12:
                part.face(quad[1:], inside, uvs[1:])
            else:
                part.face(quad, inside, uvs)
    for end, towards in ((0, 1), (len(rings) - 1, -1)):
        _, hw, hh = sections[end]
        if hw > 1e-6 and hh > 1e-6:
            c = sections[end][0]
            inside = v_add(c, v_scale(v_sub(sections[end + towards][0], c), 0.01))
            part.face(rings[end], inside, [[0, 0], [1, 0], [1, 1], [0, 1]])


def box(part, lo, hi):
    c = v_scale(v_add(lo, hi), 0.5)
    x0, y0, z0 = lo
    x1, y1, z1 = hi
    for quad in ([[x0, y0, z0], [x0, y1, z0], [x0, y1, z1], [x0, y0, z1]],
                 [[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]],
                 [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
                 [[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]],
                 [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0]],
                 [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]]):
        part.face(quad, c, [[0, 0], [1, 0], [1, 1], [0, 1]])


TOP = 0.78  # the head's middle, above the grip


def pick_parts():
    wood, grip, iron = Part(), Part(), Part()
    # The handle: ash, swelling a little toward the knob at the bottom.
    prism(wood, [([0, -0.3, 0], 0.062), ([0, -0.26, 0], 0.066), ([0, -0.2, 0], 0.052),
                 ([0, 0.3, 0], 0.047), ([0, TOP + 0.08, 0], 0.043)], 8, 1.5)
    # A leather grip where the hand closes.
    prism(grip, [([0, -0.17, 0], 0.056), ([0, 0.14, 0], 0.056)], 8, 3.0)
    # The head's eye: a collar round the top of the handle.
    box(iron, [-0.058, TOP - 0.1, -0.075], [0.058, TOP + 0.1, 0.075])
    # The pick point, forward (-z), drooping to a sharp tip.
    n = 8
    front = []
    for i in range(n + 1):
        t = i / n
        z = -0.075 - 0.47 * t
        y = TOP - 0.2 * t ** 1.8
        taper = (1 - t) ** 0.8
        front.append(([0, y, z], 0.042 * taper, 0.06 * taper))
    bar(iron, front)
    # The adze behind (+z): flattening and widening into a chisel edge.
    back = []
    for i in range(n + 1):
        t = i / n
        z = 0.075 + 0.36 * t
        y = TOP - 0.13 * t ** 1.6
        back.append(([0, y, z], 0.042 + 0.05 * t ** 1.3, 0.06 * (1 - 0.8 * t)))
    bar(iron, back)
    return [('pick_wood', wood), ('pick_grip', grip), ('pick_iron', iron)]


def mesh_of(parts):
    positions, normals, uv, primitives = [], [], [], []
    for material, (_, part) in enumerate(parts):
        base = len(positions)
        positions += part.positions
        normals += part.normals
        uv += part.uv
        primitives.append({'material': material,
                           'triangles': [[a + base, b + base, c + base] for a, b, c in part.triangles]})
    return {'frame_vertices': len(positions), 'positions': positions, 'normals': normals, 'uv': uv,
            'primitives': primitives, 'skin': None, 'billboard': False, 'billboard_y': False}


def quat_x(angle):
    return [round(math.sin(angle / 2), 6), 0.0, 0.0, round(math.cos(angle / 2), 6)]


def model():
    parts = pick_parts()
    material = lambda name: {
        'name': name, 'wrap_u': True, 'wrap_v': True, 'blend': 'opaque', 'unlit': False,
        'environment': False, 'mipmaps': True, 'detail_map': None, 'bump_map': None,
        'reflectance_map': None, 'detail_scale': 1.0, 'reflectance': 1.0}
    mesh = mesh_of(parts)
    # The first-person swing, on the `swing` node the held copy hangs from:
    # a short wind-up back, a hard chop forward and down, and back to rest.
    swing = [0.0, 0.3, 0.45, 0.2, -0.55, -1.1, -1.2, -0.75, -0.3, 0.0]
    return {
        'schema_version': 1,
        'id': 'trench-kit:file/models/trench_pick.shape.json',
        'nodes': [
            {'name': 'root', 'parent': None, 'translation': [0.0, 0.0, 0.0], 'rotation': [0.0, 0.0, 0.0, 1.0]},
            {'name': 'mountPoint', 'parent': 0, 'translation': [0.0, 0.0, 0.0], 'rotation': [0.0, 0.0, 0.0, 1.0]},
            {'name': 'swing', 'parent': 0, 'translation': [0.0, 0.0, 0.0], 'rotation': [0.0, 0.0, 0.0, 1.0]},
        ],
        # Others see the still copy; the holder sees the one that swings.
        'objects': [
            {'name': 'pick', 'node': 0, 'meshes': [0], 'visibility': 1.0, 'frame': 0, 'material_frame': 0},
            {'name': 'pickheld', 'node': 2, 'meshes': [2, 1], 'visibility': 1.0, 'frame': 0, 'material_frame': 0},
        ],
        'details': [
            {'name': 'detail32', 'pixel_threshold': 32.0, 'object_start': 0, 'object_count': 1,
             'mesh_offset': 0, 'collision': False},
            {'name': 'detail9999', 'pixel_threshold': 9999.0, 'object_start': 1, 'object_count': 1,
             'mesh_offset': 1, 'collision': False},
        ],
        'meshes': [mesh, mesh, None],
        'materials': [material(name) for name, _ in parts],
        'animations': [{
            'name': 'fire', 'frames': len(swing), 'duration': 0.3, 'looping': False, 'additive': False,
            'priority': 0,
            'nodes': [{'node': 'swing', 'rotations': [quat_x(a) for a in swing], 'translations': [],
                       'scales': [], 'scale_rotations': []}],
            'objects': [], 'ground_translations': [], 'ground_rotations': [], 'triggers': [],
        }],
    }


def texture(size, colour):
    """A `size` square RGBA image: colour(x, y) gives (r, g, b) from 0 to 1."""
    pixels = bytearray()
    for y in range(size):
        pixels.append(0)
        for x in range(size):
            r, g, b = colour(x, y)
            pixels += bytes([int(max(0, min(1, c)) * 255) for c in (r, g, b)] + [255])
    return png(bytes(pixels), size, size)


def wood(x, y):
    # Ash: pale brown with long dark grain running up the handle (v).
    grain = math.sin(x * 0.9 + math.sin(y * 0.19) * 1.7 + rng.uniform(-0.25, 0.25))
    streak = 0.08 * grain + rng.uniform(-0.03, 0.03)
    return (0.6 + streak, 0.43 + streak * 0.9, 0.26 + streak * 0.7)


def grip(x, y):
    # Dark leather wound round: a darker line every eighth of the way up.
    band = 0.72 if (y % 8) == 0 else 1.0
    n = rng.uniform(-0.04, 0.04)
    return ((0.3 + n) * band, (0.19 + n) * band, (0.11 + n) * band)


def iron(x, y):
    # Forged iron: dark grey, mottled, with a few bright scratches.
    n = rng.uniform(-0.05, 0.05)
    scratch = 0.12 if rng.random() < 0.02 else 0.0
    return (0.42 + n + scratch, 0.43 + n + scratch, 0.45 + n + scratch)


# ---- The icon ----

SIZE = 128
SAMPLES = 3


def seg(px, py, ax, ay, bx, by):
    """Distance from (px, py) to segment a-b, and how far along it (0 to 1)."""
    dx, dy = bx - ax, by - ay
    t = max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)))
    qx, qy = ax + dx * t - px, ay + dy * t - py
    return math.hypot(qx, qy), t


HANDLE = ((0.22, 0.86), (0.64, 0.32))
_dx, _dy = HANDLE[1][0] - HANDLE[0][0], HANDLE[1][1] - HANDLE[0][1]
_n = math.hypot(_dx, _dy)
ALONG = (_dx / _n, _dy / _n)
ACROSS = (-ALONG[1], ALONG[0])
HEAD = [(HANDLE[1][0] + ACROSS[0] * s * 0.34 - ALONG[0] * 0.13 * s * s,
         HANDLE[1][1] + ACROSS[1] * s * 0.34 - ALONG[1] * 0.13 * s * s, s)
        for s in (i / 40 - 1 for i in range(81))]


def head_near(u, v):
    return abs(u - HANDLE[1][0]) < 0.4 and abs(v - HANDLE[1][1]) < 0.4


def head(u, v):
    """Distance to the head's spine, where along it (-1 to 1), and its half width there."""
    best = (9.0, 0.0)
    for x, y, s in HEAD:
        d = math.hypot(u - x, v - y)
        if d < best[0]:
            best = (d, s)
    d, s = best
    return d, s, 0.055 * (1 - abs(s) ** 1.6) + 0.007


def shade(base, light):
    return tuple(min(255, int(c * light)) for c in base)


def icon():
    wood = (150, 104, 60)
    steel = (150, 156, 164)
    pixels = bytearray()
    for y in range(SIZE):
        pixels.append(0)
        for x in range(SIZE):
            acc = [0.0, 0.0, 0.0, 0.0]
            for sy in range(SAMPLES):
                for sx in range(SAMPLES):
                    u = (x + (sx + 0.5) / SAMPLES) / SIZE
                    v = (y + (sy + 0.5) / SAMPLES) / SIZE
                    color = None
                    # The head: a blade across the top of the handle,
                    # both points curving back toward the hand.
                    if head_near(u, v):
                        d, s_at, width = head(u, v)
                        if d < width:
                            color = shade(steel, 1.2 - 0.4 * (d / width) - 0.25 * (s_at + 1) / 2)
                    # The handle: from the lower left up into the head.
                    d, t = seg(u, v, HANDLE[0][0], HANDLE[0][1], HANDLE[1][0], HANDLE[1][1])
                    if color is None and d < 0.045:
                        across = d / 0.045
                        color = shade(wood, 1.12 - 0.45 * across * across - 0.2 * t)
                    if color is not None:
                        acc[0] += color[0]
                        acc[1] += color[1]
                        acc[2] += color[2]
                        acc[3] += 255
            n = SAMPLES * SAMPLES
            a = acc[3] / n
            if a > 0:
                pixels += bytes([int(acc[0] / n * 255 / a), int(acc[1] / n * 255 / a), int(acc[2] / n * 255 / a), int(a)])
            else:
                pixels += bytes(4)
    return bytes(pixels)


def png(pixels, width=SIZE, height=SIZE):
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    header = struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0)
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', header) + chunk(b'IDAT', zlib.compress(pixels, 9)) + chunk(b'IEND', b'')


if __name__ == '__main__':
    write(ROOT / 'sounds' / 'dig.wav', dig(), peak=0.8)
    write(ROOT / 'sounds' / 'place.wav', place(), peak=0.75)
    write(ROOT / 'sounds' / 'whistle.wav', fade_out(whistle(), 0.02), peak=0.7)
    models = ROOT / 'models'
    models.mkdir(parents=True, exist_ok=True)
    (models / 'trench_pick.shape.json').write_text(json.dumps(model(), separators=(',', ':')) + '\n')
    (models / 'pick_wood.png').write_bytes(texture(32, wood))
    (models / 'pick_grip.png').write_bytes(texture(32, grip))
    (models / 'pick_iron.png').write_bytes(texture(32, iron))
    (ROOT / 'icons').mkdir(parents=True, exist_ok=True)
    (ROOT / 'icons' / 'trench_pick.png').write_bytes(png(icon()))
    for path in sorted(ROOT.rglob('*')):
        if path.suffix in ('.wav', '.png', '.json') and path.parent.name != 'assets':
            print(path.relative_to(ROOT), path.stat().st_size)
