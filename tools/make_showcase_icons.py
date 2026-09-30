"""Write the showcase Add-Ons' item icons: original art drawn here, so it
carries no one else's rights (it is not a render of any game model).

Outputs 128x128 RGBA PNG:
  packages/showcase/gravity-gun-tool/assets/icons/gravity_gun.png
    the Gravity Gun as it looks in play: a dark oily shell with green-lit
    edges and glowing teal veins, a glowing muzzle, a soft teal halo so it
    reads on the dark tool slots.

Run it again after changing the drawing below; the output is the same
every run. Only the Python standard library is used.
"""
import math
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / 'packages' / 'showcase'
SIZE = 128
SAMPLES = 4  # per side, per pixel


def rotate(x, y, cx, cy, degrees):
    a = math.radians(degrees)
    dx, dy = x - cx, y - cy
    return cx + dx * math.cos(a) + dy * math.sin(a), cy - dx * math.sin(a) + dy * math.cos(a)


def ellipse(x, y, cx, cy, rx, ry, degrees=0.0):
    """Approximate signed distance to an ellipse (negative inside)."""
    x, y = rotate(x, y, cx, cy, degrees)
    k = math.hypot((x - cx) / rx, (y - cy) / ry)
    return (k - 1.0) * min(rx, ry)


def box(x, y, cx, cy, hw, hh, r, degrees=0.0):
    """Signed distance to a rounded box (negative inside)."""
    x, y = rotate(x, y, cx, cy, degrees)
    qx, qy = abs(x - cx) - hw + r, abs(y - cy) - hh + r
    return math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r


def smooth_union(a, b, k):
    h = max(k - abs(a - b), 0.0) / k
    return min(a, b) - h * h * k * 0.25


TILT = -10.0


def gun(x, y):
    """The shell's signed distance, and the distance to its seam."""
    body = box(x, y, 58, 50, 36, 16, 14, TILT)
    front = box(x, y, 98, 42, 12, 22, 7, TILT)
    grip = box(x, y, 49, 85, 10, 22, 6, 14)
    shell = smooth_union(smooth_union(body, front, 4), grip, 8)
    return shell, abs(front) if body < 0 else 9.0


def veins(x, y):
    """Distance, in pixels, to the nearest of the wandering veins."""
    best = 9.0
    slope = math.tan(math.radians(-TILT))
    for f, phase, off in ((0.12, 0.0, 40), (0.10, 1.9, 54), (0.14, 3.3, 30)):
        wave = off + 5.0 * math.sin(x * f + phase) + 2.0 * math.sin(x * f * 2.7 + phase * 1.3)
        best = min(best, abs(y - wave + (x - 58) * slope))
    # One down the grip.
    if y > 64:
        g = 49 + 3.5 * math.sin(y * 0.22) - (y - 85) * math.tan(math.radians(14))
        best = min(best, abs(x - g))
    return best


def mix(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


SHELL = (0.035, 0.04, 0.06)
SHINE = (0.16, 0.2, 0.3)
RIM = (0.27, 0.82, 0.56)
VEIN = (0.18, 0.72, 0.78)
GLOW = (0.55, 1.0, 0.95)


def shade(x, y):
    """Straight RGBA for one sample point."""
    d, seams = gun(x, y)
    muzzle_glow = math.hypot(x - 108, y - 40)
    halo = VEIN
    if d > 0:
        # Outside: the teal halo, stronger by the muzzle.
        a = max(0.0, 1.0 - d / 5.0) ** 2 * 0.45
        m = max(0.0, 1.0 - muzzle_glow / 16.0) ** 2 * 0.9
        a = max(a, m)
        return (*mix(halo, GLOW, min(1.0, m * 1.2)), a)
    # Inside: dark shell lit from above.
    light = max(0.0, min(1.0, (70 - y) / 50.0)) ** 1.5 * 0.55
    spec = max(0.0, 1.0 - math.hypot((x - 56) / 24.0, (y - 40) / 5.0)) ** 2
    colour = mix(SHELL, SHINE, min(1.0, light + spec * 0.8))
    v = veins(x, y) if -d > 3.5 else 9.0
    if v < 3.0:
        colour = mix(colour, VEIN, max(0.0, 1.0 - v / 3.0) ** 1.5 * 0.55)
    if v < 0.9:
        colour = mix(colour, GLOW, 0.5)
    if -d < 2.4 or seams < 1.2:
        colour = mix(colour, RIM, 0.9)
    if muzzle_glow < 10:
        colour = mix(colour, GLOW, max(0.0, 1.0 - muzzle_glow / 10.0) ** 0.8)
    return (*colour, 1.0)


def render():
    rows = []
    step = 1.0 / SAMPLES
    for py in range(SIZE):
        row = bytearray([0])
        for px in range(SIZE):
            r = g = b = a = 0.0
            for sy in range(SAMPLES):
                for sx in range(SAMPLES):
                    cr, cg, cb, ca = shade(px + (sx + 0.5) * step, py + (sy + 0.5) * step)
                    r += cr * ca
                    g += cg * ca
                    b += cb * ca
                    a += ca
            n = SAMPLES * SAMPLES
            if a > 0:
                r, g, b = r / a, g / a, b / a
            row += bytes(int(max(0, min(1, c)) * 255 + 0.5) for c in (r, g, b, a / n))
        rows.append(bytes(row))
    return b''.join(rows)


def png(pixels):
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    header = struct.pack('>IIBBBBB', SIZE, SIZE, 8, 6, 0, 0, 0)
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', header) + chunk(b'IDAT', zlib.compress(pixels, 9)) + chunk(b'IEND', b'')


if __name__ == '__main__':
    out = ROOT / 'gravity-gun-tool' / 'assets' / 'icons' / 'gravity_gun.png'
    out.write_bytes(png(render()))
    print(out.relative_to(ROOT), out.stat().st_size)
