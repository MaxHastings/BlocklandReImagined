"""Writes assets/models/gravity-gun.shape.json and its four texture PNGs from
geom.py. Plain Python, no Blender: `python model-source/export.py` from the
Add-On folder. (render.py previews the same geometry in Blender.)

The model is in the game's frame (x right, y up, -z forward), the grip's hold
point at the origin where the `mountPoint` node sits, scaled by SCALE.
"""
import json, os, struct, sys, zlib
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import geom

SCALE = 0.42
GRIP = (0.34, -0.62, 0.0)          # design (u, v, w): where the hand holds it
MUZZLE = (2.74, 0.0, 0.0)          # front of the glowing core
MATERIALS = [                       # name, rgb, unlit
    ("gg_body", (22, 28, 44), False),
    ("gg_glow", (30, 235, 255), True),
    ("gg_grip", (255, 215, 0), False),
    ("gg_purple", (170, 60, 255), True),
]
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "models")


def game(p):
    x, y, z = geom.to_game(geom.sub(p, GRIP))
    return [x * SCALE, y * SCALE, z * SCALE]


def png(path, rgb):
    raw = b"".join(b"\x00" + bytes(rgb + (255,)) * 4 for _ in range(4))
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 4, 4, 8, 6, 0, 0, 0))
                           + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def main():
    parts = geom.build()
    positions, normals, uv, primitives = [], [], [], []
    for mi, (name, _, _) in enumerate(MATERIALS):
        tris = []
        for t in parts[name.replace("gg_", "")]:
            pts = [game(p) for p in t]
            n = geom.norm(geom.cross(geom.sub(pts[1], pts[0]), geom.sub(pts[2], pts[0])))
            base = len(positions)
            for p in pts:
                positions.append([round(c, 5) for c in p]); normals.append([round(c, 5) for c in n]); uv.append([0.5, 0.5])
            tris.append([base, base + 1, base + 2])
        primitives.append({"material": mi, "triangles": tris})
    mesh = {"frame_vertices": len(positions), "positions": positions, "normals": normals, "uv": uv,
            "primitives": primitives, "skin": None, "billboard": False, "billboard_y": False}
    node = lambda n, t=(0.0, 0.0, 0.0): {"name": n, "parent": 0 if n != "root" else None,
                                          "translation": list(t), "rotation": [0.0, 0.0, 0.0, 1.0]}
    muzzle = game(MUZZLE)
    material = lambda n, unlit: {"name": n, "wrap_u": True, "wrap_v": True, "blend": "opaque", "unlit": unlit,
        "environment": False, "mipmaps": True, "detail_map": None, "bump_map": None,
        "reflectance_map": None, "detail_scale": 1.0, "reflectance": 1.0}
    shape = {
        "schema_version": 1, "id": "gravity-gun-tool:file/models/gravity-gun.shape.json",
        "nodes": [node("root"), node("mountPoint"), node("muzzlePoint", muzzle)],
        "objects": [
            {"name": "gravitygun", "node": 0, "meshes": [0], "visibility": 1.0, "frame": 0, "material_frame": 0},
            {"name": "gravitygunheld", "node": 0, "meshes": [2, 1], "visibility": 1.0, "frame": 0, "material_frame": 0},
        ],
        "details": [
            {"name": "detail32", "pixel_threshold": 32.0, "object_start": 0, "object_count": 1, "mesh_offset": 0, "collision": False},
            {"name": "detail9999", "pixel_threshold": 9999.0, "object_start": 1, "object_count": 1, "mesh_offset": 1, "collision": False},
        ],
        "meshes": [mesh, mesh, None],
        "materials": [material(n, u) for n, _, u in MATERIALS],
        "animations": [],
    }
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "gravity-gun.shape.json"), "w") as f:
        json.dump(shape, f, separators=(",", ":"))
    for n, rgb, _ in MATERIALS:
        png(os.path.join(OUT, n + ".png"), rgb)
    print(len(positions), "vertices;", sum(len(p["triangles"]) for p in primitives), "triangles")

main()
