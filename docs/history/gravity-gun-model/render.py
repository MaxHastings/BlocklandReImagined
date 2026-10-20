import sys, math, os
sys.path.insert(0, os.path.dirname(__file__))
import bpy, geom
from mathutils import Vector

out = sys.argv[1]
os.makedirs(out, exist_ok=True)
bpy.ops.wm.read_factory_settings(use_empty=True)
sc = bpy.context.scene
sc.render.engine = "CYCLES"
sc.cycles.samples = 48
sc.cycles.device = "CPU"
sc.render.resolution_x, sc.render.resolution_y = 900, 640
sc.view_settings.view_transform = "Standard"

w = bpy.data.worlds.new("w"); sc.world = w; w.use_nodes = True
bg = w.node_tree.nodes["Background"]
bg.inputs[0].default_value = (0.78, 0.76, 0.72, 1); bg.inputs[1].default_value = 0.9

def mat(name, col, emit=0.0, rough=0.55):
    m = bpy.data.materials.new(name); m.use_nodes = True
    b = m.node_tree.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = col
    b.inputs["Roughness"].default_value = rough
    if emit:
        b.inputs["Emission Color"].default_value = col
        b.inputs["Emission Strength"].default_value = emit
    return m
mats = {
    "body": mat("body", (0.03, 0.04, 0.065, 1), rough=0.5),
    "glow": mat("glow", (0.0, 0.75, 1.0, 1), emit=1.6),
    "grip": mat("grip", (1.0, 0.85, 0.0, 1), rough=0.6),
    "purple": mat("purple", (0.55, 0.1, 1.0, 1), emit=1.5),
}
def B(v): return (v[0], -v[2], v[1])
P = geom.build()
me = bpy.data.meshes.new("gun"); verts = []; faces = []; fm = []
names = list(mats)
for mname, tris in P.items():
    for t in tris:
        i = len(verts)
        verts += [B(geom.to_game(p)) for p in t]
        faces.append((i, i+1, i+2)); fm.append(names.index(mname))
me.from_pydata(verts, [], faces)
for n in names: me.materials.append(mats[n])
for p, m in zip(me.polygons, fm): p.material_index = m
me.update()
ob = bpy.data.objects.new("gun", me); sc.collection.objects.link(ob)
# flat look: sharp
for p in me.polygons: p.use_smooth = False
# fix flipped normals check: recalc outward
import bmesh
bm = bmesh.new(); bm.from_mesh(me); bmesh.ops.recalc_face_normals(bm, faces=bm.faces); bm.to_mesh(me); bm.free()

# lights
def light(kind, loc, energy, size=3):
    l = bpy.data.lights.new(kind, "AREA"); l.energy = energy; l.size = size
    o = bpy.data.objects.new(kind, l); o.location = loc; sc.collection.objects.link(o)
    d = Vector(B((0, 0, -1.2))) - Vector(loc)
    o.rotation_euler = d.to_track_quat("-Z", "Y").to_euler()
light("key", B((-4, 5, -5)), 500); light("fill", B((5, 2, -3)), 150, 5); light("rim", B((0, 4, 6)), 300)

cam = bpy.data.cameras.new("c"); cam.lens = 55
co = bpy.data.objects.new("c", cam); sc.collection.objects.link(co); sc.camera = co
target = Vector(B((0, -0.1, -1.3)))
views = {
    "left": (-7, 0.3, -1.3), "right": (7, 0.3, -1.3),
    "front": (0, 0.3, -8), "three_quarter": (-4.2, 2.0, -5.2), "top": (0, 8, -1.3),
    "rear_three_quarter": (4, 1.5, 4),
}
for name, loc in views.items():
    loc = B(loc)
    co.location = loc
    if name == "top":
        co.rotation_euler = (0, 0, 0); co.location = B((0, 8, -1.3)); co.location = (0, -1.3*-1, 8)
        co.location = (0, 1.3, 8)
    else:
        co.rotation_euler = (target - Vector(loc)).to_track_quat("-Z", "Y").to_euler()
    sc.render.filepath = os.path.join(out, name + ".png")
    bpy.ops.render.render(write_still=True)
print("done")
