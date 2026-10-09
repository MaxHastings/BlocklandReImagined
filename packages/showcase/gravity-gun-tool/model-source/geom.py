"""Gravity gun geometry, in design coordinates: u forward, v up, w right.
Each part is convex, so face winding is fixed by pointing normals away from
the part's centre. `build()` returns {material: [tri, ...]} with every tri
three (u, v, w) points; `to_game` maps to the game's x right, y up, -z forward.
"""
import math

PRONGS = 4


def sub(a, b): return (a[0]-b[0], a[1]-b[1], a[2]-b[2])
def add(a, b): return (a[0]+b[0], a[1]+b[1], a[2]+b[2])
def mul(a, s): return (a[0]*s, a[1]*s, a[2]*s)
def dot(a, b): return a[0]*b[0]+a[1]*b[1]+a[2]*b[2]
def cross(a, b): return (a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0])
def norm(a):
    l = math.sqrt(dot(a, a)) or 1.0
    return (a[0]/l, a[1]/l, a[2]/l)


def octagon(r, rot=22.5):
    return [(r*math.cos(math.radians(rot+45*i)), r*math.sin(math.radians(rot+45*i))) for i in range(8)]


def chamfer_rect(hx, hy, c):
    """Rectangle half-sizes hx, hy with corners cut by c (octagon-like)."""
    return [(hx-c, -hy), (hx, -hy+c), (hx, hy-c), (hx-c, hy),
            (-hx+c, hy), (-hx, hy-c), (-hx, -hy+c), (-hx+c, -hy)]


def loft(p0, p1, poly0, poly1=None, up=(0, 1, 0)):
    """Extrude polygon from p0 to p1 (optionally tapering to poly1). The
    cross-section axes are (up projected off the axis, and the axis x that)."""
    poly1 = poly1 or poly0
    ax = norm(sub(p1, p0))
    a = sub(up, mul(ax, dot(up, ax)))
    if dot(a, a) < 1e-6:
        a = sub((0, 0, 1), mul(ax, ax[2]))
    a = norm(a)
    b = norm(cross(ax, a))
    def ring(p, poly):
        return [add(p, add(mul(a, x), mul(b, y))) for x, y in poly]
    r0, r1 = ring(p0, poly0), ring(p1, poly1)
    centre = mul(add(p0, p1), 0.5)
    n = len(poly0)
    faces = [[r0[i], r0[(i+1) % n], r1[(i+1) % n], r1[i]] for i in range(n)]
    faces.append(list(reversed(r0)))
    faces.append(r1)
    tris = []
    for f in faces:
        nrm = cross(sub(f[1], f[0]), sub(f[2], f[0]))
        fc = mul(add(add(f[0], f[1]), add(f[2], f[3] if len(f) > 3 else f[2])), 0.25)
        if len(f) > 3 and dot(nrm, sub(fc, centre)) < 0:
            f = list(reversed(f))
        elif len(f) == 3:
            pass
        # caps with many vertices: fan
        for k in range(1, len(f)-1):
            t = [f[0], f[k], f[k+1]]
            nn = cross(sub(t[1], t[0]), sub(t[2], t[0]))
            if dot(nn, sub(mul(add(add(t[0], t[1]), t[2]), 1/3), centre)) < 0:
                t = [t[0], t[2], t[1]]
            tris.append(tuple(t))
    return tris


def box(c, h, up=(0, 1, 0)):
    """Axis-aligned box centre c, half sizes h=(u,v,w)."""
    return loft((c[0]-h[0], c[1], c[2]), (c[0]+h[0], c[1], c[2]),
                [(-h[1], -h[2]), (h[1], -h[2]), (h[1], h[2]), (-h[1], h[2])], up=(0, 1, 0))


def bar(p0, p1, t0, t1=None, up=(0, 1, 0), bevel=0.0):
    """A beam from p0 to p1, cross-section t0 x t1 (full widths)."""
    t1 = t1 if t1 is not None else t0
    hx, hy = t0/2, t1/2
    poly = chamfer_rect(hx, hy, bevel) if bevel else [(-hx, -hy), (hx, -hy), (hx, hy), (-hx, hy)]
    return loft(p0, p1, poly, up=up)


def build():
    P = {"body": [], "glow": [], "grip": [], "purple": []}

    def add_(mat, tris): P[mat].extend(tris)

    # ---- receiver: a chunky chamfered block with a raised, tapered hump
    add_("body", loft((0.0, 0, 0), (1.0, 0, 0), chamfer_rect(0.44, 0.44, 0.16), up=(0, 1, 0)))
    add_("body", loft((0.12, 0.40, 0), (0.95, 0.40, 0), chamfer_rect(0.30, 0.20, 0.10), chamfer_rect(0.22, 0.14, 0.08), up=(0, 0, 1)))
    add_("body", loft((-0.10, 0, 0), (0.0, 0, 0), chamfer_rect(0.34, 0.36, 0.12), up=(0, 1, 0)))
    for s in (-1, 1):
        x = s*0.445
        add_("glow", box((0.62, 0.05, x), (0.12, 0.10, 0.012)))                    # window
        # cracked seam lines running across the shell
        add_("glow", bar((0.10, 0.30, x), (0.40, 0.05, x), 0.03, 0.02, up=(0, 0, 1)))
        add_("glow", bar((0.40, 0.05, x), (0.40, -0.30, x), 0.03, 0.02, up=(0, 0, 1)))
        add_("glow", bar((0.40, 0.05, x), (0.80, 0.22, x), 0.03, 0.02, up=(0, 0, 1)))
        add_("glow", bar((0.80, 0.22, x), (0.95, 0.12, x), 0.03, 0.02, up=(0, 0, 1)))
        add_("glow", bar((0.70, -0.2, x), (0.95, -0.1, x), 0.03, 0.02, up=(0, 0, 1)))
    for s_ in (-1, 1):
        for k in range(3):
            add_("purple", box((0.30 + 0.12*k, 0.30, s_*0.447), (0.04, 0.012, 0.012)))
    add_("purple", box((0.55, 0.56, -0.09), (0.12, 0.012, 0.03)))
    add_("purple", box((0.55, 0.56, 0.09), (0.12, 0.012, 0.03)))
    add_("glow", box((0.85, 0.50, 0), (0.12, 0.012, 0.012)))
    # shoulder seam where the shell meets the barrel
    add_("glow", loft((0.995, 0, 0), (1.015, 0, 0), chamfer_rect(0.45, 0.45, 0.16)))

    # ---- barrel: thick glowing tube with dark ribs on the corners
    add_("glow", loft((1.0, 0, 0), (1.75, 0, 0), octagon(0.31)))
    for i in range(8):
        a = math.radians(22.5+45*i+22.5)
        c = (0.34*math.cos(a), 0.34*math.sin(a))
        add_("body", bar((1.0, c[0], c[1]), (1.75, c[0], c[1]), 0.13, 0.13, up=(0, 1, 0)))
    add_("purple", box((1.3, 0.34, 0), (0.05, 0.012, 0.05)))

    # ---- flanges
    add_("body", loft((1.70, 0, 0), (1.84, 0, 0), octagon(0.58)))
    add_("glow", loft((1.84, 0, 0), (1.875, 0, 0), octagon(0.595)))
    add_("body", loft((1.875, 0, 0), (1.99, 0, 0), octagon(0.50)))
    add_("glow", loft((1.99, 0, 0), (2.015, 0, 0), octagon(0.515)))

    # ---- core: dark socket + a faceted glowing orb
    add_("body", loft((2.015, 0, 0), (2.12, 0, 0), octagon(0.40)))
    add_("glow", loft((2.12, 0, 0), (2.30, 0, 0), octagon(0.22), octagon(0.34)))
    add_("glow", loft((2.30, 0, 0), (2.56, 0, 0), octagon(0.34)))
    add_("glow", loft((2.56, 0, 0), (2.74, 0, 0), octagon(0.34), octagon(0.20)))

    # ---- claw: chunky prongs hugging the orb, each flaring out, running
    # forward, then curling its tip back in toward the axis
    for i in range(PRONGS):
        ang = math.radians(45.0 + 360.0*i/PRONGS)
        radial = (0.0, math.sin(ang), math.cos(ang))
        def at(u, r): return (u, radial[1]*r, radial[2]*r)
        up_hint = (0, radial[1], radial[2])
        add_("body", bar(at(1.95, 0.52), at(2.22, 0.98), 0.30, 0.50, up=up_hint, bevel=0.08))
        add_("body", bar(at(2.22, 0.98), at(2.86, 0.98), 0.30, 0.50, up=up_hint, bevel=0.08))
        add_("body", bar(at(2.86, 0.98), at(3.12, 0.60), 0.30, 0.44, up=up_hint, bevel=0.08))
        add_("glow", bar(at(2.08, 0.80), at(2.22, 1.14), 0.03, 0.10, up=up_hint))
        add_("glow", bar(at(2.34, 1.14), at(2.78, 1.14), 0.03, 0.10, up=up_hint))
        add_("glow", bar(at(3.00, 0.92), at(3.10, 0.76), 0.03, 0.10, up=up_hint))

    # ---- grip: dark raked handle, yellow thumb rest and finger blocks
    add_("body", loft((0.40, -0.35, 0), (0.28, -1.20, 0), chamfer_rect(0.22, 0.20, 0.05), chamfer_rect(0.22, 0.20, 0.05), up=(0, 0, 1)))
    add_("grip", box((0.14, -0.60, 0), (0.06, 0.30, 0.22)))      # rear strap
    add_("grip", box((0.00, -0.32, 0), (0.20, 0.07, 0.22)))      # thumb rest on top of the strap
    for k in range(3):
        add_("grip", box((0.68, -0.52 - 0.22*k, 0), (0.09, 0.09, 0.19)))
    add_("body", box((0.31, -1.24, 0), (0.28, 0.07, 0.24)))
    return P


def to_game(p):
    u, v, w = p
    return (w, v, -u)


if __name__ == "__main__":
    P = build()
    print({k: len(v) for k, v in P.items()})
