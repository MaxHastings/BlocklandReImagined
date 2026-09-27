"""Independent v20 brick reference renderer (software, offscreen).

Reads `artifacts/brick-audit/layout.json` written by the `brick_audit` client
test, the original BLB files and surface PNGs from a read-only v20 install, and
renders the same scene with the fixed-function state recovered from
`blocklandv20.exe` (see docs/audits/bricks.md):

* per-vertex GL lighting (ambient + sun * N.L, COLOR_MATERIAL), clamped;
* GL_DECAL texturing: mix(lit, tex.rgb, tex.a), alpha from the vertex;
* brickSIDE: GL_CLAMP, nearest magnification; others GL_REPEAT, trilinear;
* opaque buckets, then translucent buckets (prints, RAMP, TOP, BOTTOMLOOP,
  BOTTOMEDGE, SIDE) with SRC_ALPHA blending and depth writes.

Generated `BRICK` geometry follows the loader at 0x53ad25. Nothing here reads
the native converter's output, so the comparison is independent.

Usage: python tools/brick_reference.py <v20-install> [artifacts/brick-audit]
"""
import json
import math
import sys
from pathlib import Path

import numpy as np
from PIL import Image

SS = 2  # supersampling factor
TEX = {"TOP": 0, "BOTTOMLOOP": 1, "BOTTOMEDGE": 2, "SIDE": 3, "RAMP": 4, "PRINT": 5}
FILES = {0: "brickTOP", 1: "brickBOTTOMLOOP", 2: "brickBOTTOMEDGE", 3: "brickSIDE", 4: "brickRAMP"}
TRANSLUCENT_ORDER = [5, 4, 0, 1, 2, 3]


def mip_chain(img):
    levels = [img]
    while levels[-1].shape[0] > 1 and levels[-1].shape[1] > 1:
        a = levels[-1]
        h, w = a.shape[0] // 2, a.shape[1] // 2
        levels.append(a[: h * 2, : w * 2].reshape(h, 2, w, 2, 4).mean(axis=(1, 3)))
    return levels


def load_texture(path):
    return mip_chain(np.asarray(Image.open(path).convert("RGBA"), dtype=np.float32) / 255.0)


def sample_level(level, u, v, clamp, nearest):
    h, w = level.shape[:2]
    x = u * w - 0.5
    y = v * h - 0.5
    if nearest:
        xi = np.floor(x + 0.5).astype(int)
        yi = np.floor(y + 0.5).astype(int)
        if clamp:
            xi, yi = np.clip(xi, 0, w - 1), np.clip(yi, 0, h - 1)
        else:
            xi, yi = xi % w, yi % h
        return level[yi, xi]
    x0 = np.floor(x).astype(int)
    y0 = np.floor(y).astype(int)
    fx = (x - x0)[..., None]
    fy = (y - y0)[..., None]

    def fetch(xx, yy):
        if clamp:
            return level[np.clip(yy, 0, h - 1), np.clip(xx, 0, w - 1)]
        return level[yy % h, xx % w]

    return (
        fetch(x0, y0) * (1 - fx) * (1 - fy)
        + fetch(x0 + 1, y0) * fx * (1 - fy)
        + fetch(x0, y0 + 1) * (1 - fx) * fy
        + fetch(x0 + 1, y0 + 1) * fx * fy
    )


def sample(chain, u, v, lod, clamp, nearest_mag):
    if clamp:
        u = np.clip(u, 0.0, 1.0)
        v = np.clip(v, 0.0, 1.0)
    if lod <= 0:
        return sample_level(chain[0], u, v, clamp, nearest_mag)
    lo = min(int(math.floor(lod)), len(chain) - 1)
    hi = min(lo + 1, len(chain) - 1)
    t = lod - math.floor(lod)
    a = sample_level(chain[lo], u, v, clamp, nearest_mag)
    b = sample_level(chain[hi], u, v, clamp, nearest_mag)
    return a * (1 - t) + b * t


def parse_blb(text):
    lines = [l.split("//")[0].strip() for l in text.splitlines()]
    lines = [l for l in lines if l and not (l.startswith("---") and "quads" in l.lower())]
    size = [int(float(v)) for v in lines[0].split()]
    kind = lines[1]
    if kind == "BRICK":
        return size, generated(*size)
    i = 2 + size[1] * size[2]
    boxes = int(lines[i])
    i += 1 + boxes * 2
    if lines[i].upper().startswith("COVERAGE"):
        i += 7
    quads = []
    for _face in range(7):
        count = int(lines[i])
        i += 1
        for _ in range(count):
            tex = TEX[lines[i].split(":")[1].strip().upper()]
            pos = [list(map(float, lines[i + 2 + k].split())) for k in range(4)]
            uv = [list(map(float, lines[i + 7 + k].split())) for k in range(4)]
            i += 11
            colors = None
            if lines[i].upper() == "COLORS:":
                colors = [list(map(float, lines[i + 1 + k].split())) for k in range(4)]
                i += 5
            normals = [list(map(float, lines[i + 1 + k].split())) for k in range(4)]
            i += 5
            quads.append((tex, pos, uv, normals, colors))
    return size, quads


def generated(w, d, h):
    """BRICK quads in BLB stud/plate units, per blocklandv20.exe 0x53ad25."""
    e = 0.0012
    x, y, z = w * 0.25, d * 0.25, h * 0.1
    xe, ye, ze = x + e, y + e, z + e
    ix, iy = x - 0.25, y - 0.25
    rim, inset = 245 / 512, 0.084

    def span(n):
        a = rim * n / (n - inset)
        return 0.5 - a, 0.5 + a

    quads = []

    def add(tex, pos, uv, n):
        # Torque units back to BLB units (x,y studs of 0.5, z plates of 0.2).
        quads.append((tex, [[p[0] * 2, p[1] * 2, p[2] * 5] for p in pos], uv, [n] * 4, None))

    add(0, [[xe, -ye, ze], [-xe, -ye, ze], [-xe, ye, ze], [xe, ye, ze]],
        [[0, 0], [w, 0], [w, d], [0, d]], [0, 0, 1])
    dn = [0, 0, -1]
    add(2, [[-xe, -ye, -ze], [-ix, -iy, -ze], [-ix, iy, -ze], [-xe, ye, -ze]],
        [[d - .5, 0], [d - 1, .5], [0, .5], [-.5, 0]], dn)
    add(2, [[xe, ye, -ze], [ix, iy, -ze], [ix, -iy, -ze], [xe, -ye, -ze]],
        [[-.5, 0], [0, .5], [d - 1, .5], [d - .5, 0]], dn)
    add(2, [[-xe, ye, -ze], [-ix, iy, -ze], [ix, iy, -ze], [xe, ye, -ze]],
        [[w - .5, 0], [w - 1, .5], [0, .5], [-.5, 0]], dn)
    add(2, [[xe, -ye, -ze], [ix, -iy, -ze], [-ix, -iy, -ze], [-xe, -ye, -ze]],
        [[w - .5, 0], [w - 1, .5], [0, .5], [-.5, 0]], dn)
    if w > 1 and d > 1:
        add(1, [[ix, -iy, -ze], [ix, iy, -ze], [-ix, iy, -ze], [-ix, -iy, -ze]],
            [[0, w - 1], [d - 1, w - 1], [d - 1, 0], [0, 0]], dn)
    v0, v1 = span(h * 0.4)

    def side(u):
        return [[u[0], v0], [u[1], v0], [u[1], v1], [u[0], v1]]

    add(3, [[xe, ye, ze], [-xe, ye, ze], [-xe, ye, -ze], [xe, ye, -ze]], side(span(w)), [0, 1, 0])
    add(3, [[xe, -ye, ze], [xe, ye, ze], [xe, ye, -ze], [xe, -ye, -ze]], side(span(d)), [1, 0, 0])
    add(3, [[-xe, -ye, ze], [xe, -ye, ze], [xe, -ye, -ze], [-xe, -ye, -ze]], side(span(w)), [0, -1, 0])
    add(3, [[-xe, ye, ze], [-xe, -ye, ze], [-xe, -ye, -ze], [-xe, ye, -ze]], side(span(d)), [-1, 0, 0])
    return quads


def folded(ms, period):
    t = ms % period
    return min(t, period - t) / (period * 0.5)


def v20_fx(record, verts, normals, bases, paint_quad, eye, ms, sun, sun_color):
    """Colour/shape FX of quad emitter 0x52ed70, in Torque axes (x, y, z) =
    native (x, -z, y). Returns new vertices, normals and colours."""
    color_fx, shape_fx = record.get("color_fx", 0), record.get("shape_fx", 0)
    centre = np.array(record["position"], float)
    depth = record.get("depth_studs", 1)
    tau = 2 * math.pi
    verts = [v.copy() for v in verts]
    normals = [n.copy() for n in normals]
    bases = [b.copy() for b in bases]
    torque = lambda v: (v[0], -v[2], v[1])
    if shape_fx == 1:
        dist = np.linalg.norm(centre - eye)
        if dist < 100:
            amp = min(max(100 - dist, 0), 10) * 0.1 * 0.08
            phase = (ms % 1000) * 0.001 * tau
            for v in verts:
                x, y, z = torque(v)
                s_ = (y + x + z) * 1.256637 + phase
                dx, dy, dz = math.sin(s_ + 2.094395) * amp, math.sin(s_ + 4.18879) * amp, math.sin(s_ + tau) * amp
                v += np.array([dx, dz, -dy])
    elif shape_fx == 2:
        phase = (ms % 2000) * 0.0005 * tau
        for v, n in zip(verts, normals):
            a = v[0] * 1.256637 + phase
            v[1] -= (math.sin(a + tau) + 1) * 0.1
            n[1] += (math.sin(a + 2.094395) + 1) * 0.25
    if not paint_quad or color_fx == 0:
        return verts, normals, bases
    paint = bases[0]
    toward = centre - eye
    toward = toward / max(np.linalg.norm(toward), 1e-9)
    for k, (v, n) in enumerate(zip(verts, normals)):
        if color_fx == 1:
            t = min(max((v - centre) @ toward / (depth * 0.125) + 0.5, 0), 1)
            rgb = np.minimum(paint[:3] * 1.6, 1) * (1 - t) + paint[:3] * 0.9 * t
            bases[k] = np.append(rgb, paint[3] * (1 - t) + paint[3] * 0.9 * t)
        elif color_fx == 2:
            t = min(max((v - centre) @ toward / (depth * 0.5) + 0.5, 0), 1)
            if t > 0.5:
                s_ = min(max((t - 0.5) * 2, 0), 1)
                rgb = paint[:3] * 0.5 * s_ + paint[:3] * (1 - s_)
            else:
                s_ = min(max(t * 2, 0), 1)
                rgb = paint[:3] * s_ + (1 - s_)
            bases[k] = np.append(rgb, paint[3])
            normals[k] = n * 2
        elif color_fx == 3:
            shortest = 1.0
            for c in sun_color:
                if c > 0:
                    shortest = min(shortest, c)
            normals[k] = sun / shortest
        elif color_fx == 4:
            bases[k] = np.append(paint[:3] * (0.7 + 0.6 * folded(ms, 1000)), paint[3])
        elif color_fx == 5:
            bases[k] = np.append(paint[:3] * (0.4 + 0.6 * folded(ms + 250 * k, 1000)), paint[3])
        elif color_fx == 6:
            phase = (ms % 1000) * 0.001 * tau
            x, y, z = torque(v)
            wave = np.array([math.sin(y + z - x + phase + 2.094395), math.sin(x - y + z + phase + 4.18879),
                             math.sin(y + x - z + phase + tau)])
            bases[k] = np.append(wave * paint[:3], paint[3])
    return verts, normals, bases


def look_at(eye, target):
    f = target - eye
    f /= np.linalg.norm(f)
    s = np.cross(f, [0, 1, 0])
    s /= np.linalg.norm(s)
    u = np.cross(s, f)
    m = np.eye(4)
    m[0, :3], m[1, :3], m[2, :3] = s, u, -f
    m[:3, 3] = -m[:3, :3] @ eye
    return m


def perspective(fov, aspect, near, far):
    t = 1 / math.tan(fov / 2)
    m = np.zeros((4, 4))
    m[0, 0], m[1, 1] = t / aspect, t
    m[2, 2], m[2, 3] = far / (near - far), near * far / (near - far)
    m[3, 2] = -1
    return m


def main():
    install = Path(sys.argv[1])
    out = Path(sys.argv[2] if len(sys.argv) > 2 else "artifacts/brick-audit")
    layout = json.loads((out / "layout.json").read_text())
    W, H = layout["width"] * SS, layout["height"] * SS
    vp = perspective(math.radians(layout["fov_y_degrees"]), W / H, layout["near"], layout["far"]) @ look_at(
        np.array(layout["eye"], float), np.array(layout["target"], float))
    eye = np.array(layout["eye"], float)
    sun = -np.array(layout["sun_direction"], float)
    sun /= np.linalg.norm(sun)
    sun_color = np.array(layout["sun_color"])
    ms = layout.get("time_seconds", 0.0) * 1000.0
    ambient = np.array(layout["ambient"])
    shapes = install / "base/data/shapes"
    textures = {k: load_texture(shapes / f"{v}.png") for k, v in FILES.items()}
    materials = Path(layout["materials"])
    color = np.zeros((H, W, 3), np.float32)
    color[:] = layout["background"]
    depth = np.full((H, W), np.inf, np.float32)
    tris = []  # (bucket, translucent, verts[3] clip, uv[3], lit rgba[3], tex chain, clamp)
    for record in layout["bricks"]:
        blb = install / record["blb"].removeprefix("v20/")
        _size, quads = parse_blb(blb.read_text(encoding="utf-8-sig"))
        paint = np.array(record["paint"], float)
        origin = np.array(record["position"], float)
        turns = record["quarter_turns"]
        ca, sa = math.cos(-turns * math.pi / 2), math.sin(-turns * math.pi / 2)
        rot = np.array([[ca, 0, sa], [0, 1, 0], [-sa, 0, ca]])
        print_chain = load_texture(materials / record["print"]) if record["print"] else None
        for tex, pos, uv, normals, colors in quads:
            chain = print_chain if tex == 5 else textures[tex]
            if chain is None:
                continue
            verts, norms, bases = [], [], []
            for k in range(4):
                p = np.array(pos[k], float)
                verts.append(rot @ np.array([p[0] * 0.5, p[2] * 0.2, -p[1] * 0.5]) + origin)
                n = np.array(normals[k], float)
                n = rot @ np.array([n[0], n[2], -n[1]])
                norms.append(n / max(np.linalg.norm(n), 1e-9))
                base = paint.copy()
                if colors is not None:
                    c = np.array(colors[k], float)
                    base = np.concatenate([np.clip(paint[:3] + c[:3], 0, 1), paint[3:]]) if c[3] < 0 else c
                bases.append(base)
            paint_quad = colors is None or all(c[3] < 0 for c in colors)
            verts, norms, bases = v20_fx(record, verts, norms, bases, paint_quad, eye, ms, sun, sun_color)
            # Fixed-function lighting with the (possibly unnormalised) FX normal.
            lits = [np.concatenate([np.clip(b[:3] * (ambient + sun_color * max(n @ sun, 0.0)), 0, 1), [b[3]]])
                    for b, n in zip(bases, norms)]
            translucent = any(l[3] < 1 for l in lits)
            for a, b, c in ((0, 1, 2), (0, 2, 3)):
                tris.append((tex, translucent, [verts[a], verts[b], verts[c]],
                             [uv[a], uv[b], uv[c]], [lits[a], lits[b], lits[c]], chain, tex == 3))
    ordered = [t for t in tris if not t[1]]
    for bucket in TRANSLUCENT_ORDER:
        ordered += [t for t in tris if t[1] and t[0] == bucket]
    for tex, translucent, verts, uvs, lits, chain, clamp in ordered:
        clip = [vp @ np.append(v, 1.0) for v in verts]
        if any(c[3] <= layout["near"] for c in clip):
            continue
        ndc = [c[:3] / c[3] for c in clip]
        sx = np.array([(n[0] * 0.5 + 0.5) * W for n in ndc])
        sy = np.array([(0.5 - n[1] * 0.5) * H for n in ndc])
        area = (sx[1] - sx[0]) * (sy[2] - sy[0]) - (sx[2] - sx[0]) * (sy[1] - sy[0])
        if area <= 0:  # back face
            continue
        x0, x1 = max(int(sx.min()), 0), min(int(sx.max()) + 1, W)
        y0, y1 = max(int(sy.min()), 0), min(int(sy.max()) + 1, H)
        if x0 >= x1 or y0 >= y1:
            continue
        px, py = np.meshgrid(np.arange(x0, x1) + 0.5, np.arange(y0, y1) + 0.5)
        w0 = ((sx[1] - px) * (sy[2] - py) - (sx[2] - px) * (sy[1] - py)) / area
        w1 = ((sx[2] - px) * (sy[0] - py) - (sx[0] - px) * (sy[2] - py)) / area
        w2 = 1 - w0 - w1
        inside = (w0 >= 0) & (w1 >= 0) & (w2 >= 0)
        if not inside.any():
            continue
        inv = np.array([1 / c[3] for c in clip])
        persp = w0 * inv[0] + w1 * inv[1] + w2 * inv[2]
        b = [w0 * inv[0] / persp, w1 * inv[1] / persp, w2 * inv[2] / persp]
        z = w0 * ndc[0][2] + w1 * ndc[1][2] + w2 * ndc[2][2]
        region = depth[y0:y1, x0:x1]
        mask = inside & (z < region)
        if not mask.any():
            continue
        uv = np.array(uvs)
        u = b[0] * uv[0, 0] + b[1] * uv[1, 0] + b[2] * uv[2, 0]
        v = b[0] * uv[0, 1] + b[1] * uv[1, 1] + b[2] * uv[2, 1]
        lit = sum(b[k][..., None] * lits[k] for k in range(3))
        size = chain[0].shape[0]
        texel_area = abs((uv[1, 0] - uv[0, 0]) * (uv[2, 1] - uv[0, 1]) - (uv[2, 0] - uv[0, 0]) * (uv[1, 1] - uv[0, 1])) * size * size
        lod = 0.5 * math.log2(max(texel_area / max(abs(area), 1e-6) / (SS * SS), 1e-9))
        t = sample(chain, u, v, lod, clamp, clamp)
        rgb = lit[..., :3] * (1 - t[..., 3:4]) + t[..., :3] * t[..., 3:4]
        alpha = lit[..., 3:4]
        dst = color[y0:y1, x0:x1]
        blended = rgb * alpha + dst * (1 - alpha)
        dst[mask] = blended[mask]
        region[mask] = z[mask]
    image = color.reshape(H // SS, SS, W // SS, SS, 3).mean(axis=(1, 3))
    Image.fromarray((np.clip(image, 0, 1) * 255 + 0.5).astype(np.uint8)).save(out / "v20-reference.png")
    ours = Image.open(out / "ours.png").convert("RGB")
    ref = Image.open(out / "v20-reference.png")
    both = Image.new("RGB", (ours.width, ours.height * 2))
    both.paste(ref, (0, 0))
    both.paste(ours, (0, ours.height))
    both.save(out / "side-by-side.png")
    diff = np.abs(np.asarray(ref, np.float32) - np.asarray(ours, np.float32)).mean()
    print(f"mean absolute difference {diff:.2f}/255")


if __name__ == "__main__":
    main()
