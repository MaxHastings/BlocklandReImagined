"""Build the Gravity Gun's model in Blender and write it for the game.

Run with Blender's Python (Blender itself, or `pip install bpy`):

    blender -b -P tools/make_gravity_gun_model.py -- [--render DIR] [--blend FILE] [--icon]
    python tools/make_gravity_gun_model.py [--render DIR] [--blend FILE] [--icon]

Outputs, under packages/showcase/gravity-gun-tool/assets/models/:
  gravity-gun.shape.json   the model (native shape: x right, y up, -z
                           forward), with `mountPoint` at the grip and
                           `muzzlePoint` on the core's face
  gg_*.png                 one flat colour per material

`--render DIR` also renders preview sheets there, `--blend FILE` saves
the scene so it can be opened and changed in Blender, and `--icon [FILE]`
renders the item icon shown until the game has drawn one from the model
(by default icons/gravity_gun.png; Cycles' denoiser makes it differ by a
few bits from run to run).

The gun is built from chunky bevelled blocks. Every glowing line is a
groove cut into the shell (a boolean whose cut faces take the glowing
material), so the lines sit in the surface at any distance. Each
material's faces carry its slot in the texture's u (slot k at
(k + 0.5) / 8) and how far along the gun they are in v (0 at the back, 1
at the claw's tips), so the skin (skins/gravity.wgsl) knows what it is
drawing without the textures.
"""
import argparse
import hashlib
import json
import math
import os
import struct
import sys
import zlib
from pathlib import Path

import bpy  # noqa: I001 (bpy first: it provides bmesh and mathutils)
import bmesh
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'packages' / 'showcase' / 'gravity-gun-tool' / 'assets' / 'models'
ICON = ROOT / 'packages' / 'showcase' / 'gravity-gun-tool' / 'assets' / 'icons' / 'gravity_gun.png'
MODEL_ID = 'gravity-gun-tool:file/models/gravity-gun.shape.json'

# The model is designed in its own units with the barrel along +y and up
# +z (Blender's axes), then scaled by SCALE into game units.
SCALE = 0.72
# Where the image sits in first person (weapons.json's `eye_offset`, x
# right, y up, -z forward from the eye), for the first-person preview.
EYE_OFFSET = (0.85, -1.0, -1.6)
# And `eye_rotation`: Torque Euler degrees (x pitch, y roll, z yaw; z
# turns the muzzle right), applied as the game does.
EYE_ROTATION = (-2.0, -8.0, -6.0)

# Materials: (name, slot, colour (sRGB 0..255), unlit). The order is the
# slot each one's faces carry in u.
MATERIALS = [
    ('gg_shell', 0, (17, 20, 30), False),
    ('gg_seam', 1, (60, 235, 255), True),
    ('gg_core', 2, (80, 228, 255), True),
    ('gg_grip', 3, (255, 214, 0), False),
    ('gg_accent', 4, (190, 70, 255), True),
    ('gg_dark', 5, (8, 10, 16), False),
    ('gg_hot', 6, (235, 255, 255), True),
    # The claw's own seams: the same light, but they move as the claw
    # opens, which the skin (drawn at rest) cannot follow, so it leaves
    # them be.
    ('gg_clawseam', 7, (60, 235, 255), True),
]
SLOTS = len(MATERIALS)
UV_SLOTS = 8

# The barrel's axis, the grip and the claw, in design units.
AXIS_Z = 0.50
BACK_Y, FRONT_Y = -0.62, 1.80


def srgb_to_linear(c):
    c = c / 255
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


# ---------------------------------------------------------------- scene

def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    for name, _, colour, unlit in MATERIALS:
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        b = m.node_tree.nodes['Principled BSDF']
        lin = [srgb_to_linear(c) for c in colour] + [1.0]
        b.inputs['Base Color'].default_value = lin
        b.inputs['Roughness'].default_value = 0.45 if name == 'gg_shell' else 0.6
        if unlit:
            # Unlit in the game: exactly its colour, whatever the light.
            b.inputs['Base Color'].default_value = (0, 0, 0, 1)
            b.inputs['Emission Color'].default_value = lin
            b.inputs['Emission Strength'].default_value = 1.0
    bpy.data.collections.new('cutters')


def mat(name):
    return bpy.data.materials[name]


def new_object(name, bm, material):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    me.materials.append(mat(material))
    ob = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(ob)
    return ob


def frame(axis, up_hint):
    """Orthonormal (side, along, up) for a part running along `axis`."""
    along = Vector(axis).normalized()
    up = Vector(up_hint)
    up = (up - along * up.dot(along))
    if up.length < 1e-6:
        up = Vector((1, 0, 0)) - along * along.x
    up.normalize()
    side = along.cross(up).normalized()
    return side, along, up


def prism(name, p0, p1, poly, material, up=(0, 0, 1), poly1=None, bevel=0.0):
    """A closed prism from p0 to p1 whose cross-section is `poly` (2D, in
    the part's side and up), optionally tapering to `poly1`."""
    p0, p1 = Vector(p0), Vector(p1)
    side, along, upv = frame(p1 - p0, up)
    bm = bmesh.new()
    rings = []
    for p, pl in ((p0, poly), (p1, poly1 or poly)):
        rings.append([bm.verts.new(p + side * x + upv * y) for x, y in pl])
    n = len(poly)
    bm.faces.new(list(reversed(rings[0])))
    bm.faces.new(rings[1])
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new([rings[0][i], rings[0][j], rings[1][j], rings[1][i]])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    ob = new_object(name, bm, material)
    if bevel:
        mod = ob.modifiers.new('bevel', 'BEVEL')
        mod.width = bevel
        mod.segments = 1
        mod.limit_method = 'ANGLE'
        mod.angle_limit = math.radians(30)
    return ob


def rect(hx, hy, c=0.0):
    if c <= 0:
        return [(-hx, -hy), (hx, -hy), (hx, hy), (-hx, hy)]
    return [(-hx + c, -hy), (hx - c, -hy), (hx, -hy + c), (hx, hy - c),
            (hx - c, hy), (-hx + c, hy), (-hx, hy - c), (-hx, -hy + c)]


def octagon(apothem):
    r = apothem / math.cos(math.radians(22.5))
    return [(r * math.cos(math.radians(22.5 + 45 * i)), r * math.sin(math.radians(22.5 + 45 * i)))
            for i in range(8)]


def bar(name, p0, p1, w, h, material, up=(0, 0, 1), bevel=0.0, chamfer=0.0):
    return prism(name, p0, p1, rect(w / 2, h / 2, chamfer), material, up=up, bevel=bevel)


def block(name, lo, hi, material, bevel=0.0):
    lo, hi = Vector(lo), Vector(hi)
    c = (lo + hi) / 2
    return bar(name, (c.x, lo.y, c.z), (c.x, hi.y, c.z), hi.x - lo.x, hi.z - lo.z, material, bevel=bevel)


def cutter(target, ob):
    """Cut `ob` out of `target`; the cut faces take `ob`'s material."""
    coll_name = f'cut:{target.name}'
    coll = bpy.data.collections.get(coll_name)
    if coll is None:
        coll = bpy.data.collections.new(coll_name)
        bpy.data.collections['cutters'].children.link(coll)
        mod = target.modifiers.new('cuts', 'BOOLEAN')
        mod.operation = 'DIFFERENCE'
        mod.operand_type = 'COLLECTION'
        mod.collection = coll
        mod.solver = 'EXACT'
        mod.material_mode = 'TRANSFER'
    for c in ob.users_collection:
        c.objects.unlink(ob)
    coll.objects.link(ob)
    ob.hide_render = True
    ob.display_type = 'WIRE'
    if target.data.materials.find(ob.data.materials[0].name) < 0:
        target.data.materials.append(ob.data.materials[0])
    return ob


def groove(target, face_point, normal, a, b, width=0.032, depth=0.026, material='gg_seam'):
    """A glowing groove on the face of `target` through `face_point` with
    outward `normal`, from a to b (points on that face)."""
    n = Vector(normal).normalized()
    a, b = Vector(a), Vector(b)
    d = (b - a)
    length = d.length + width
    mid = (a + b) / 2
    # The cutter: `width` across, from `depth` under the face to well
    # outside it.
    outer = 0.3
    c0 = mid - d.normalized() * length / 2
    c1 = mid + d.normalized() * length / 2
    off = n * ((outer - depth) / 2)
    ob = prism('groove', c0 + off, c1 + off, rect(width / 2, (outer + depth) / 2), material, up=n)
    return cutter(target, ob)


def window(target, centre, normal, size_a, size_b, up, depth=0.04, material='gg_seam'):
    """A rectangular glowing inset on a face."""
    n = Vector(normal).normalized()
    upv = Vector(up)
    outer = 0.3
    c = Vector(centre) + n * ((outer - depth) / 2)
    side = upv.cross(n).normalized()
    ob = prism('window', c - side * size_a / 2, c + side * size_a / 2,
               rect(size_b / 2, (outer + depth) / 2), material, up=n)
    return cutter(target, ob)


def union(target, others):
    for o in others:
        mod = target.modifiers.new('join', 'BOOLEAN')
        mod.operation = 'UNION'
        mod.object = o
        mod.solver = 'EXACT'
        mod.material_mode = 'TRANSFER'
        o.hide_render = True
        o.hide_viewport = True
        for m in o.data.materials:
            if target.data.materials.find(m.name) < 0:
                target.data.materials.append(m)
    return target


def mirror_z(ob, name, axis):
    """A copy of `ob` (its modifiers applied) mirrored across z = axis."""
    deps = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(ob.evaluated_get(deps))
    me.transform(ob.matrix_world)
    me.transform(Matrix.Translation((0, 0, axis)) @ Matrix.Scale(-1, 4, (0, 0, 1))
                 @ Matrix.Translation((0, 0, -axis)))
    me.flip_normals()
    copy = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(copy)
    return copy


# ---------------------------------------------------------------- gun

def build():
    reset()
    z = AXIS_Z
    S = 0.30  # receiver half width

    # Receiver: a chunky bevelled block, heavier at the back, with a raised
    # spine and a back plate.
    rec = block('receiver', (-S, -0.52, 0.20), (S, 0.42, 0.80), 'gg_shell', bevel=0.07)
    spine = block('spine', (-0.20, -0.46, 0.70), (0.20, 0.26, 0.93), 'gg_shell', bevel=0.06)
    back = block('backplate', (-0.25, BACK_Y, 0.27), (0.25, -0.46, 0.73), 'gg_shell', bevel=0.05)

    # Panel lines on both sides: angular, like circuits, branching from a
    # node behind the side window.
    for s in (-1, 1):
        x = s * S
        n = (s, 0, 0)
        lines = [
            [(-0.52, 0.66), (-0.30, 0.66), (-0.20, 0.56), (0.02, 0.56)],
            [(-0.20, 0.56), (-0.20, 0.44), (-0.32, 0.32), (-0.32, 0.20)],
            [(0.02, 0.56), (0.02, 0.70), (0.10, 0.78)],
            [(0.02, 0.56), (0.02, 0.36), (0.14, 0.28), (0.42, 0.28)],
            [(-0.06, 0.20), (-0.06, 0.30), (0.02, 0.36)],
        ]
        for line in lines:
            for (ya, za), (yb, zb) in zip(line, line[1:]):
                groove(rec, (x, 0, 0), n, (x, ya, za), (x, yb, zb))
        # The side window: a framed glowing square near the front.
        window(rec, (x, 0.22, 0.56), n, 0.15, 0.15, (0, 0, 1), depth=0.05)
        # The spine's side seam.
        groove(spine, (s * 0.20, 0, 0), n, (s * 0.20, -0.40, 0.79), (s * 0.20, 0.22, 0.79), width=0.026)
    # A line down the spine's middle, and two violet vents on its top.
    groove(spine, (0, 0, 0.93), (0, 0, 1), (0, -0.05, 0.93), (0, 0.24, 0.93), width=0.03)
    for xv in (-0.08, 0.08):
        groove(spine, (0, 0, 0.93), (0, 0, 1), (xv, -0.36, 0.93), (xv, -0.16, 0.93), width=0.045,
               depth=0.03, material='gg_accent')
    # The back plate's glowing vents.
    for zv in (0.40, 0.50, 0.60):
        groove(back, (0, BACK_Y, 0), (0, -1, 0), (-0.13, BACK_Y, zv), (0.13, BACK_Y, zv), width=0.04)
    # Seams round the receiver where it meets the barrel and the back plate.
    groove(rec, (0, 0, 0.80), (0, 0, 1), (-S, 0.30, 0.80), (S, 0.30, 0.80), width=0.03)
    groove(rec, (0, 0, 0.20), (0, 0, -1), (-S, 0.30, 0.20), (S, 0.30, 0.20), width=0.03)

    # Barrel: an octagonal tube in a dark cage, glowing through windows on
    # its sides and diagonals, with a violet strip on top.
    barrel = prism('barrel', (0, 0.36, z), (0, 1.00, z), octagon(0.21), 'gg_shell', bevel=0.012)
    for i in range(8):
        ang = math.radians(45 * i)
        nrm = Vector((math.cos(ang), 0, math.sin(ang)))
        if i in (2, 6):
            continue  # top and bottom stay solid
        c = Vector((0, 0.69, z)) + nrm * 0.21
        window(barrel, c, nrm, 0.42, 0.11, nrm.cross(Vector((0, 1, 0))), depth=0.05)
    groove(barrel, (0, 0, z + 0.21), (0, 0, 1), (0, 0.48, z + 0.21), (0, 0.66, z + 0.21), width=0.05,
           depth=0.03, material='gg_accent')
    groove(barrel, (0, 0, z + 0.21), (0, 0, 1), (0, 0.74, z + 0.21), (0, 0.92, z + 0.21), width=0.03)

    # Collar: two stepped rings with a glowing band between, then the
    # core's socket and the core itself.
    ring_a = prism('collar_a', (0, 0.90, z), (0, 1.03, z), octagon(0.35), 'gg_shell', bevel=0.025)
    band = prism('collar_band', (0, 1.03, z), (0, 1.055, z), octagon(0.33), 'gg_seam')
    ring_b = prism('collar_b', (0, 1.055, z), (0, 1.15, z), octagon(0.31), 'gg_shell', bevel=0.02)
    socket = prism('socket', (0, 1.15, z), (0, 1.21, z), octagon(0.25), 'gg_dark', bevel=0.01)
    core = prism('core', (0, 1.19, z), (0, 1.34, z), octagon(0.20), 'gg_core')
    cap = prism('core_cap', (0, 1.34, z), (0, 1.43, z), octagon(0.20), 'gg_core', poly1=octagon(0.11))
    hot = prism('core_hot', (0, 1.42, z), (0, 1.445, z), octagon(0.09), 'gg_hot')

    # The claw: a C of two chunky arms, top and bottom, each rising from
    # the collar, running forward and hooking in toward the core.
    # The claw: a C of two chunky arms, top and bottom, each rising from
    # the collar, running forward and hooking in toward the core. The top
    # one is built and the bottom one is its mirror image.
    def p(y, dz):
        return (0, y, z + dz)
    w, t = 0.34, 0.19
    riser = bar('arm_riser', p(0.93, 0.24), p(1.17, 0.55), w, t, 'gg_shell', up=(0, -1, 0))
    reach = bar('arm_reach', p(1.10, 0.55), p(1.64, 0.50), w, t, 'gg_shell')
    hook = bar('arm_hook', p(1.60, 0.52), p(1.78, 0.28), w * 0.9, t, 'gg_shell', up=(0, 1, 0))
    knuckle = prism('arm_knuckle', (-w / 2 - 0.02, 1.13, z + 0.55), (w / 2 + 0.02, 1.13, z + 0.55),
                    octagon(0.125), 'gg_shell', up=(0, 1, 0))
    arm = union(riser, [reach, hook, knuckle])
    arm.name = 'arm_top'
    bev = arm.modifiers.new('bevel', 'BEVEL')
    bev.width = 0.03
    bev.segments = 1
    bev.limit_method = 'ANGLE'
    bev.angle_limit = math.radians(30)
    # Glowing lines along the arm's outer face and up its sides, and a
    # light either side of its tip.
    top = 0.55 + t / 2
    seam = 'gg_clawseam'
    groove(arm, (0, 0, z + top), (0, 0, 1), (0, 1.20, z + top - 0.004), (0, 1.58, z + 0.50 + t / 2), width=0.035,
           material=seam)
    for sx in (-1, 1):
        window(arm, (sx * w * 0.45, 1.69, z + 0.42), (sx, 0, 0), 0.07, 0.07, (0, 0, 1), depth=0.03, material=seam)
        groove(arm, (sx * w / 2, 0, 0), (sx, 0, 0), (sx * w / 2, 1.00, z + 0.36), (sx * w / 2, 1.10, z + 0.52),
               width=0.03, material=seam)
    mirror_z(arm, 'arm_bottom', z)

    # Grip: dark, raked back, with yellow finger blocks and trigger.
    g0, g1 = Vector((0, -0.10, 0.24)), Vector((0, -0.27, -0.42))
    grip = bar('grip', g0, g1, 0.21, 0.27, 'gg_shell', up=(0, 1, 0), bevel=0.035)
    pommel = block('pommel', (-0.135, -0.46, -0.50), (0.135, -0.08, -0.40), 'gg_shell', bevel=0.03)
    along = (g1 - g0).normalized()
    fwd = Vector((0, 1, 0))
    fwd = (fwd - along * fwd.dot(along)).normalized()
    pads = []
    for k in range(3):
        c = g0 + along * (0.26 + 0.17 * k) + fwd * 0.12
        pads.append(bar(f'finger{k}', c - along * 0.065, c + along * 0.065, 0.235, 0.09, 'gg_grip',
                        up=tuple(fwd), bevel=0.02))
    back_c = g0 + along * 0.18 - fwd * 0.14
    backstrap = bar('backstrap', back_c - along * 0.16, back_c + along * 0.16, 0.235, 0.06, 'gg_grip',
                    up=tuple(fwd), bevel=0.015)
    trigger = bar('trigger', (0, 0.10, 0.22), (0, 0.06, 0.06), 0.07, 0.08, 'gg_grip', up=(0, 1, 0), bevel=0.012)

    # Where the hand holds it and where the beam leaves.
    mount = g0 + along * 0.40
    muzzle = Vector((0, 1.445, z))
    return mount, muzzle


# The claw's arms each turn on a hinge where they leave the collar: open
# while the beam holds something (the image's Grab state plays `open`),
# shut again on letting go (`close`).
CLAW_HINGE = {'arm_top': Vector((0, 0.98, AXIS_Z + 0.30)), 'arm_bottom': Vector((0, 0.98, AXIS_Z - 0.30))}
CLAW_OPEN_DEGREES = 16.0


def pose_claw(opened):
    """Turn the arms' objects in the Blender scene (for previews)."""
    for name, hinge in CLAW_HINGE.items():
        ob = bpy.data.objects[name]
        sign = 1 if name == 'arm_top' else -1
        turn = Matrix.Rotation(math.radians(CLAW_OPEN_DEGREES * opened * sign), 4, 'X')
        ob.matrix_world = Matrix.Translation(hinge) @ turn @ Matrix.Translation(-hinge)


# ---------------------------------------------------------------- export

def evaluated_triangles():
    """Every visible part's triangles in world space, by material name:
    [(object, material, (p0, p1, p2))]."""
    deps = bpy.context.evaluated_depsgraph_get()
    out = []
    for ob in bpy.context.scene.objects:
        if ob.type != 'MESH' or ob.hide_render:
            continue
        ev = ob.evaluated_get(deps)
        me = ev.to_mesh()
        bm = bmesh.new()
        bm.from_mesh(me)
        bmesh.ops.triangulate(bm, faces=bm.faces)
        mw = ob.matrix_world
        for f in bm.faces:
            idx = f.material_index
            name = me.materials[idx].name if idx < len(me.materials) and me.materials[idx] else 'gg_shell'
            tri = tuple(mw @ v.co for v in f.verts)
            out.append((ob.name, name, tri))
        bm.free()
        ev.to_mesh_clear()
    return out


def to_game(v, mount):
    """Design (x right, y forward, z up) to native (x right, y up, -z
    forward), scaled, with the grip's mount point at the origin."""
    d = (Vector(v) - mount) * SCALE
    return [round(d.x, 5), round(d.z, 5), round(-d.y, 5)]


def shape(mount, muzzle):
    pose_claw(0.0)
    tris = evaluated_triangles()
    names = [m[0] for m in MATERIALS]
    span = FRONT_Y - BACK_Y
    parts = ['gun', 'arm_top', 'arm_bottom']
    origin = {'gun': mount, **CLAW_HINGE}
    meshes = {p: ([], [], [], {n: [] for n in names}) for p in parts}
    for ob_name, name, (a, b, c) in tris:
        part = ob_name if ob_name in CLAW_HINGE else 'gun'
        positions, normals, uv, prims = meshes[part]
        if name not in prims:
            name = 'gg_shell'
        n = (b - a).cross(c - a)
        if n.length < 1e-9:
            continue
        n.normalize()
        slot = names.index(name)
        base = len(positions)
        for v in (a, b, c):
            # Each part in its own node's frame.
            positions.append(to_game(v, origin[part]))
            normals.append([round(n.x, 5), round(n.z, 5), round(-n.y, 5)])
            uv.append([(slot + 0.5) / UV_SLOTS, round(min(max((v.y - BACK_Y) / span, 0.0), 1.0), 4)])
        prims[name].append([base, base + 1, base + 2])
    plain = {'wrap_u': True, 'wrap_v': True, 'blend': 'opaque', 'environment': False, 'mipmaps': False,
             'detail_map': None, 'bump_map': None, 'reflectance_map': None, 'detail_scale': 1.0,
             'reflectance': 1.0}

    def node(name, parent, at):
        return {'name': name, 'parent': parent, 'translation': to_game(at, mount),
                'rotation': [0.0, 0.0, 0.0, 1.0]}

    def mesh(part):
        positions, normals, uv, prims = meshes[part]
        return {'frame_vertices': len(positions), 'positions': positions, 'normals': normals, 'uv': uv,
                'primitives': [{'material': names.index(n), 'triangles': t} for n, t in prims.items() if t],
                'skin': None, 'billboard': False, 'billboard_y': False}

    def claw(opened):
        # Turning about x is the same in Blender's axes and the game's.
        tracks = []
        for name in CLAW_HINGE:
            sign = 1 if name == 'arm_top' else -1
            half = math.radians(CLAW_OPEN_DEGREES * sign) / 2
            tracks.append({'node': name, 'rotations': [
                [round(math.sin(half * o), 6), 0.0, 0.0, round(math.cos(half * o), 6)] for o in opened],
                'translations': [], 'scales': [], 'scale_rotations': []})
        return tracks

    def animation(name, seconds, opened):
        return {'name': name, 'frames': len(opened), 'duration': seconds, 'looping': False, 'additive': False,
                'priority': 0, 'nodes': claw(opened), 'objects': [], 'ground_translations': [],
                'ground_rotations': [], 'triggers': []}

    return {
        'schema_version': 1,
        'id': MODEL_ID,
        'nodes': [node('root', None, mount), node('mountPoint', 0, mount), node('muzzlePoint', 0, muzzle),
                  node('arm_top', 0, CLAW_HINGE['arm_top']), node('arm_bottom', 0, CLAW_HINGE['arm_bottom'])],
        'objects': [{'name': p, 'node': [0, 3, 4][i], 'meshes': [i], 'visibility': 1.0, 'frame': 0,
                     'material_frame': 0} for i, p in enumerate(parts)],
        'details': [{'name': 'detail32', 'pixel_threshold': 32.0, 'object_start': 0, 'object_count': len(parts),
                     'mesh_offset': 0, 'collision': False}],
        'meshes': [mesh(p) for p in parts],
        'materials': [dict(name=n, unlit=u, **plain) for n, _, _, u in MATERIALS],
        # Snapping open overshoots a little and settles; shutting is quick.
        'animations': [animation('open', 0.18, [0.0, 0.8, 1.12, 1.0]),
                       animation('close', 0.12, [1.0, 0.4, 0.0])],
    }


def png(width, height, rgb):
    raw = bytearray()
    for _ in range(height):
        raw.append(0)
        raw.extend(bytes(rgb) * width)

    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data) & 0xFFFFFFFF)
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(bytes(raw), 9)) + chunk(b'IEND', b''))


def write(out, model):
    out.mkdir(parents=True, exist_ok=True)
    data = (json.dumps(model, separators=(',', ':')) + '\n').encode()
    (out / 'gravity-gun.shape.json').write_bytes(data)
    for name, _, colour, _ in MATERIALS:
        (out / f'{name}.png').write_bytes(png(4, 4, colour))
    tris = sum(len(p['triangles']) for m in model['meshes'] for p in m['primitives'])
    print(f'gravity-gun.shape.json: {tris} triangles, {hashlib.sha256(data).hexdigest()}')


# ---------------------------------------------------------------- previews

CENTRE = Vector((0, 0.55, 0.30))


def studio():
    """A floor-free studio round the gun (a key, a fill and a rim) and a
    camera: the scene, the camera's object and its camera."""
    sc = bpy.context.scene
    sc.render.engine = 'CYCLES'
    sc.cycles.samples = 64
    sc.cycles.device = 'CPU'
    sc.cycles.use_denoising = True
    sc.cycles.seed = 0
    sc.render.resolution_x, sc.render.resolution_y = 960, 640
    sc.view_settings.view_transform = 'Standard'
    sc.render.film_transparent = False
    w = bpy.data.worlds.new('w')
    sc.world = w
    w.use_nodes = True
    bg = w.node_tree.nodes['Background']
    bg.inputs[0].default_value = (0.70, 0.69, 0.66, 1)
    bg.inputs[1].default_value = 0.6
    centre = CENTRE

    def light(name, loc, energy, size):
        l = bpy.data.lights.new(name, 'AREA')
        l.energy = energy
        l.size = size
        o = bpy.data.objects.new(name, l)
        o.location = loc
        o.rotation_euler = (centre - Vector(loc)).to_track_quat('-Z', 'Y').to_euler()
        sc.collection.objects.link(o)
    light('key', (-3.5, -2.0, 4.0), 600, 3)
    light('fill', (4.0, 1.0, 1.5), 180, 5)
    light('rim', (0.5, 5.0, 3.0), 450, 3)
    cam = bpy.data.cameras.new('cam')
    cam.lens = 70
    co = bpy.data.objects.new('cam', cam)
    sc.collection.objects.link(co)
    sc.camera = co
    return sc, co, cam


def render(out, mount):
    sc, co, cam = studio()
    centre = CENTRE
    views = {
        'left': ((-6.5, 0.55, 0.35), 'PERSP'),
        'right': ((6.5, 0.55, 0.35), 'PERSP'),
        'front': ((0.0, 7.0, 0.45), 'PERSP'),
        'top': ((0.0, 0.6, 7.0), 'PERSP'),
        'three_quarter': ((-4.2, 4.0, 2.4), 'PERSP'),
        'rear_three_quarter': ((4.0, -3.6, 2.6), 'PERSP'),
    }
    out.mkdir(parents=True, exist_ok=True)
    for name, (loc, _) in views.items():
        co.location = loc
        co.rotation_euler = (centre - Vector(loc)).to_track_quat('-Z', 'Y').to_euler()
        if name == 'top':
            co.location = (0, 0.6, 7.0)
            co.rotation_euler = (0, 0, -math.pi / 2)
        sc.render.filepath = str(out / f'{name}.png')
        bpy.ops.render.render(write_still=True)
    # The claw open, as while the beam holds something.
    pose_claw(1.0)
    loc = views['three_quarter'][0]
    co.location = loc
    co.rotation_euler = (centre - Vector(loc)).to_track_quat('-Z', 'Y').to_euler()
    sc.render.filepath = str(out / 'three_quarter_open.png')
    bpy.ops.render.render(write_still=True)
    pose_claw(0.0)
    first_person(out, mount, sc, co, cam)


def first_person(out, mount, sc, co, cam):
    """The gun as its holder sees it: the camera at the eye with the
    game's 90 degree field of view, the gun scaled to game units at
    EYE_OFFSET, over a plain backdrop."""
    x, y, z = EYE_OFFSET
    at = Vector((x, -z, y))
    rx, ry, rz = (math.radians(-d) for d in EYE_ROTATION)
    # The engine-family composition Ry(-y) Rx(-x) Rz(-z), in Torque's
    # axes, which are the design's.
    turn = Matrix.Rotation(ry, 4, 'Y') @ Matrix.Rotation(rx, 4, 'X') @ Matrix.Rotation(rz, 4, 'Z')
    place = Matrix.Translation(at) @ turn @ Matrix.Scale(SCALE, 4) @ Matrix.Translation(-mount)
    # Every mesh, the cutters too, so the grooves move with the gun.
    moved = [o for o in bpy.data.objects if o.type == 'MESH']
    saved = {o.name: o.matrix_world.copy() for o in moved}
    for o in moved:
        o.matrix_world = place @ o.matrix_world
    cam.lens_unit = 'FOV'
    cam.sensor_fit = 'HORIZONTAL'
    cam.angle = math.radians(90)
    cam.clip_start = 0.01
    co.location = (0, 0, 0)
    co.rotation_euler = (math.radians(90), 0, 0)
    sc.render.resolution_x, sc.render.resolution_y = 1280, 720
    sc.render.filepath = str(out / 'first_person.png')
    bpy.ops.render.render(write_still=True)
    for o in moved:
        o.matrix_world = saved[o.name]


def icon(file):
    """The item icon the game shows until it has drawn one from the model
    (icons/gravity_gun.render.json): 128 pixels, on a clear background,
    nose up and to the right as the stock tools' icons point."""
    sc, co, cam = studio()
    sc.render.film_transparent = True
    sc.render.resolution_x = sc.render.resolution_y = 128
    sc.cycles.samples = 128
    tilt = (Matrix.Translation(CENTRE) @ Matrix.Rotation(math.radians(38), 4, 'X')
            @ Matrix.Translation(-CENTRE))
    moved = [o for o in bpy.data.objects if o.type == 'MESH']
    saved = {o.name: o.matrix_world.copy() for o in moved}
    for o in moved:
        o.matrix_world = tilt @ o.matrix_world
    cam.type = 'ORTHO'
    cam.ortho_scale = 2.8
    loc = CENTRE + Vector((6.0, 1.4, 2.2))
    co.location = loc
    co.rotation_euler = (CENTRE - loc).to_track_quat('-Z', 'Y').to_euler()
    sc.render.filepath = str(Path(file).resolve())
    bpy.ops.render.render(write_still=True)
    for o in moved:
        o.matrix_world = saved[o.name]


def main():
    argv = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else sys.argv[1:]
    ap = argparse.ArgumentParser()
    ap.add_argument('--render')
    ap.add_argument('--blend')
    ap.add_argument('--out', default=str(OUT))
    ap.add_argument('--icon', nargs='?', const=str(ICON))
    args = ap.parse_args(argv)
    mount, muzzle = build()
    write(Path(args.out), shape(mount, muzzle))
    if args.blend:
        bpy.ops.wm.save_as_mainfile(filepath=str(Path(args.blend).resolve()))
    if args.render:
        render(Path(args.render), mount)
    if args.icon:
        icon(args.icon)


if __name__ == '__main__':
    main()
