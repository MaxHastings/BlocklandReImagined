#!/usr/bin/env python3
"""Offscreen LAYOUT RECONSTRUCTION of v20 GUIs from decompiled static GUI data
and original install bitmaps/font caches. Not a screenshot and not engine-exact:
it reproduces geometry (Torque resize rules), skins (bitmap arrays, _n button
states, mColor tint), and text (original .gft glyph caches). Engine features
that are approximated or omitted are listed in engine-behavior.md.

Usage: gui_render.py <static-objects.json> <v20_root> <out_dir> WxH GuiName [GuiName...]
       add --annotate to also write *_annotated.png with control outlines/lines.
Read-only with respect to the install.
"""
import json, os, re, sys
from PIL import Image, ImageDraw
sys.path.insert(0, os.path.dirname(__file__))
from gft import GFT

args = [a for a in sys.argv[1:] if not a.startswith('--')]
ANNOTATE = '--annotate' in sys.argv
BG = next((a.split('=',1)[1] for a in sys.argv if a.startswith('--bg=')), None)
data = {}
for part in args[0].split(','):
    data.update(json.load(open(part)))
ROOT = args[1]; OUT = args[2]
W, H = map(int, args[3].split('x')); names = args[4:]
UI = os.path.join(ROOT, 'base/client/ui')
os.makedirs(OUT, exist_ok=True)

# ---------- profiles ----------
PROF = {}
def collect(o):
    if o['class'] == 'GuiControlProfile' and o['name']:
        PROF[o['name']] = o
    for c in o['children']: collect(c)
for objs in data.values():
    for o in objs: collect(o)
def pf(name, key, default=None, depth=0):
    p = PROF.get(str(name))
    if not p or depth > 8: return default
    if key in p['fields']:
        v = p['fields'][key]
        return v['expr'] if isinstance(v, dict) else v
    if p.get('parent'): return pf(p['parent'], key, default, depth + 1)
    return default

def color(v, default=(0, 0, 0, 255)):
    if v is None or v == '': return default
    if isinstance(v, (int, float)): v = str(v)
    parts = [float(x) for x in str(v).split()]
    if parts and all(x <= 1.0 for x in parts) and any('.' in s for s in str(v).split()):
        parts = [x * 255 for x in parts]
    while len(parts) < 4: parts.append(255)
    return tuple(max(0, min(255, int(round(x)))) for x in parts[:4])

# ---------- assets ----------
_img = {}
def load(ref):
    if not ref: return None
    r = ref.replace('\\', '/')
    if r.startswith('./'): r = 'base/client/ui/' + r[2:]
    elif r.startswith('~/'): r = 'base/' + r[2:]
    if r in _img: return _img[r]
    im = None
    for e in ('', '.png', '.jpg'):
        p = os.path.join(ROOT, r + e)
        if os.path.isfile(p):
            im = Image.open(p).convert('RGBA'); break
    if im is None:   # case-insensitive fallback
        d, b = os.path.split(os.path.join(ROOT, r))
        if os.path.isdir(d):
            low = {f.lower(): f for f in os.listdir(d)}
            for e in ('', '.png', '.jpg'):
                if (b + e).lower() in low and os.path.isfile(os.path.join(d, low[(b + e).lower()])):
                    im = Image.open(os.path.join(d, low[(b + e).lower()])).convert('RGBA'); break
    _img[r] = im
    return im

def bitmap_array(im):
    """GuiControlProfile::constructBitmapArray (Torque3D guiTypes.cpp:558)."""
    px = im.load(); sep = px[0, 0]; rects = []; y = 0
    while y < im.height:
        if px[0, y] == sep: y += 1; continue
        x = 0
        while x < im.width:
            if px[x, y] == sep: x += 1; continue
            sx = x
            while x < im.width and px[x, y] != sep: x += 1
            sy = y
            while sy < im.height and px[sx, sy] != sep: sy += 1
            rects.append((sx, y, x - sx, sy - y))
        while y < im.height and px[0, y] != sep: y += 1
    return rects

_fonts = {}
def font(profile):
    ftype = str(pf(profile, 'fontType', 'Arial') or 'Arial')
    try: size = int(float(pf(profile, 'fontSize', 14) or 14))
    except ValueError: size = 14
    key = (ftype, size)
    if key in _fonts: return _fonts[key]
    cache = os.path.join(UI, 'cache')
    cands = [f for f in os.listdir(cache) if f.lower().startswith(ftype.lower() + '_')]
    best = None
    for f in cands:
        s = int(re.search(r'_(\d+)\.gft$', f).group(1))
        if best is None or abs(s - size) < abs(best[0] - size): best = (s, f)
    f = best[1] if best else 'Arial_14.gft'
    _fonts[key] = GFT(os.path.join(cache, f))
    return _fonts[key]

def strip_ml(t):
    # keep <cN> color markers (from \cN escapes); drop other ML tags like <just:center>
    return re.sub(r'<(?!c\d>)[^>]*>', '', str(t))

def text(canvas, rect, s, profile, justify=None, col=None, vcenter=True):
    if s is None or s == '': return
    s = strip_ml(s)
    f = font(profile)
    col = col or color(pf(profile, 'fontColor', '0 0 0'))
    x, y, w, h = rect
    just = str(justify or pf(profile, 'justify', 'left')).lower()
    if '$platform' in just: just = 'left'
    lines = s.split('\n')
    ty = y + ((h - f.height * len(lines)) // 2 if vcenter else 0)
    for ln in lines:
        segs, cur = [], col
        for part in re.split(r'(<c\d>)', ln):
            m = re.match(r'<c(\d)>', part)
            if m:
                cur = color(pf(profile, f'fontColors[{m.group(1)}]', None), col); continue
            if part: segs.append((part, cur))
        plain = ''.join(p for p, _ in segs)
        tw = f.width(plain)
        tx = x + (w - tw) // 2 if just == 'center' else (x + w - tw if just == 'right' else x)
        if pf(profile, 'doFontOutline') in (1, '1'):
            oc = color(pf(profile, 'fontOutlineColor', '0 0 0'))
            for dx, dy in ((-1,0),(1,0),(0,-1),(0,1)):
                f.draw(canvas, (tx+dx, ty+dy), plain, oc)
        px_ = tx
        for part, c in segs:
            px_ = f.draw(canvas, (px_, ty), part, c)
        ty += f.height

def blit(canvas, im, rect, tint=None, wrap=False):
    x, y, w, h = rect
    if im is None or w <= 0 or h <= 0: return
    if wrap:
        tile = Image.new('RGBA', (w, h))
        for ty in range(0, h, im.height):
            for tx in range(0, w, im.width): tile.paste(im, (tx, ty))
        src = tile
    else:
        src = im.resize((w, h), Image.BILINEAR)
    if tint and tint[:3] != (255, 255, 255):
        r, g, b, a = src.split()
        r = r.point(lambda v: v * tint[0] // 255); g = g.point(lambda v: v * tint[1] // 255); b = b.point(lambda v: v * tint[2] // 255)
        src = Image.merge('RGBA', (r, g, b, a))
    canvas.alpha_composite(src, (x, y)) if x >= 0 and y >= 0 else canvas.paste(src, (x, y), src)

def fill(canvas, rect, c):
    x, y, w, h = rect
    if w <= 0 or h <= 0 or c[3] == 0: return
    ov = Image.new('RGBA', (w, h), c)
    canvas.paste(ov, (x, y), ov) if (x < 0 or y < 0) else canvas.alpha_composite(ov, (x, y))

# ---------- layout (GuiControl::parentResized, Torque3D guiControl.cpp:1348) ----------
def pair(v, d=(0, 0)):
    try:
        a = [int(float(t)) for t in str(v).split()]; return (a[0], a[1])
    except Exception: return d

def layout(o, old_parent, new_parent, px, py, out):
    f = o['fields']
    x, y = pair(f.get('position', '0 0')); w, h = pair(f.get('extent', '0 0'))
    ow, oh = w, h
    hs = str(f.get('horizSizing', 'right')).lower(); vs = str(f.get('vertSizing', 'bottom')).lower()
    dx, dy = new_parent[0] - old_parent[0], new_parent[1] - old_parent[1]
    nx, ny, nw, nh = x, y, w, h
    if hs == 'center': nx = (new_parent[0] - w) // 2
    elif hs == 'width': nw = w + dx
    elif hs == 'left': nx = x + dx
    elif hs == 'relative' and old_parent[0]:
        nx = round(x / old_parent[0] * new_parent[0]); nw = round(w / old_parent[0] * new_parent[0])
    if vs == 'center': ny = (new_parent[1] - h) // 2
    elif vs == 'height': nh = h + dy
    elif vs == 'top': ny = y + dy
    elif vs == 'relative' and old_parent[1]:
        ny = round(y / old_parent[1] * new_parent[1]); nh = round(h / old_parent[1] * new_parent[1])
    mn = pair(f.get('minExtent', '8 2'))
    if not (nw >= mn[0] and nh >= mn[1]): nx, ny, nw, nh = x, y, w, h
    node = {'o': o, 'rect': (px + nx, py + ny, nw, nh), 'kids': []}
    out.append(node)
    for c in o['children']:
        layout(c, (ow, oh), (nw, nh), px + nx, py + ny, node['kids'])
    return node

# ---------- draw ----------
ANN = []
def inter(a, b):
    x0, y0 = max(a[0], b[0]), max(a[1], b[1])
    x1, y1 = min(a[0] + a[2], b[0] + b[2]), min(a[1] + a[3], b[1] + b[3])
    return (x0, y0, x1 - x0, y1 - y0) if x1 > x0 and y1 > y0 else None

def draw(canvas, node, clip=None):
    """Torque clips every control (and its children) to its parent's bounds."""
    clip = clip or (0, 0, W, H)
    o = node['o']; f = o['fields']; r = node['rect']
    if f.get('visible', 1) in (0, '0'): return
    vis = inter(r, clip)
    if vis is None: return
    if vis == r:
        draw_self(canvas, node)
    else:
        layer = Image.new('RGBA', (W, H), (0, 0, 0, 0))
        draw_self(layer, node)
        canvas.alpha_composite(layer.crop((vis[0], vis[1], vis[0] + vis[2], vis[1] + vis[3])), (vis[0], vis[1]))
    if o['name'] or o['class'] in ('GuiWindowCtrl', 'GuiBitmapButtonCtrl'):
        ANN.append((vis, f"L{o['line']} {o['name'] or o['class']}"))
    if o['class'] == 'GuiShapeNameHud':
        return   # assumption: name HUD renders player names itself; its authored child swatch is not visible in-game
    for k in node['kids']:
        draw(canvas, k, vis)

def draw_self(canvas, node):
    o = node['o']; f = o['fields']; cls = o['class']; r = node['rect']
    prof = f.get('profile', 'GuiDefaultProfile')
    x, y, w, h = r
    if cls == 'GuiWindowCtrl':
        im = load(pf(prof, 'bitmap'))
        if im is not None:
            b = bitmap_array(im)
            if len(b) >= 23:
                crop = lambda i: im.crop((b[i][0], b[i][1], b[i][0] + b[i][2], b[i][1] + b[i][3]))
                TLK, TRK, TK, BL, BR, BBL, BB, BBR = 12, 13, 14, 18, 19, 20, 21, 22
                fill(canvas, (x + b[BL][2], y + b[TK][3], w - b[BL][2] - b[BR][2], h - b[TK][3] - b[BB][3]),
                     color(pf(prof, 'fillColor', '200 200 200')))
                blit(canvas, crop(TLK), (x, y, b[TLK][2], b[TLK][3]))
                blit(canvas, crop(TRK), (x + w - b[TRK][2], y, b[TRK][2], b[TRK][3]))
                blit(canvas, crop(TK), (x + b[TLK][2], y, w - b[TLK][2] - b[TRK][2], b[TK][3]))
                blit(canvas, crop(BL), (x, y + b[TLK][3], b[BL][2], h - b[TLK][3] - b[BBL][3]))
                blit(canvas, crop(BR), (x + w - b[BR][2], y + b[TRK][3], b[BR][2], h - b[TRK][3] - b[BBR][3]))
                blit(canvas, crop(BBL), (x, y + h - b[BBL][3], b[BBL][2], b[BBL][3]))
                blit(canvas, crop(BBR), (x + w - b[BBR][2], y + h - b[BBR][3], b[BBR][2], b[BBR][3]))
                blit(canvas, crop(BB), (x + b[BBL][2], y + h - b[BB][3], w - b[BBL][2] - b[BBR][2], b[BB][3]))
                if f.get('canClose', 1) not in (0, '0'):
                    c0 = crop(0); blit(canvas, c0, (x + w - c0.width - 4, y + 3, c0.width, c0.height))
                to = pair(pf(prof, 'textOffset', '5 2'))
                text(canvas, (x + to[0] + 4, y + to[1], w - 40, b[TK][3] - to[1]), f.get('text', ''), prof,
                     justify='left', col=color(pf(prof, 'fontColor', '255 255 255')))
        else:
            fill(canvas, r, (200, 200, 200, 255))
    elif cls in ('GuiBitmapCtrl', 'GuiChunkedBitmapCtrl', 'GuiFadeinBitmapCtrl'):
        if f.get('bitmap'):
            tint = color(f['mColorTint']) if f.get('mColorTint') else None   # setColor() on bitmap ctrls
            im = load(f['bitmap'])
            if im is not None and tint and tint[3] < 255:
                im = im.copy(); im.putalpha(im.getchannel('A').point(lambda v: v * tint[3] // 255))
            blit(canvas, im, r, tint=tint, wrap=f.get('wrap') in (1, '1'))
    elif cls == 'GuiCrossHairHud':
        blit(canvas, load(f.get('bitmap', '')), r)
    elif cls == 'GuiAnimatedBitmapCtrl':
        blit(canvas, load(str(f.get('bitmap', '')) + '_00'), r)
    elif cls == 'GuiBitmapButtonCtrl':
        im = load(str(f.get('bitmap', '')) + '_n') or load(f.get('bitmap', ''))
        blit(canvas, im, r, tint=color(f.get('mColor', '255 255 255 255')))
        t = f.get('text', '')
        if str(t).strip(): text(canvas, r, t, prof, justify='center')
    elif cls == 'GuiSwatchCtrl':
        fill(canvas, r, color(f.get('color', '255 255 255 255')))
    elif cls in ('GuiTextCtrl',):
        text(canvas, r, f.get('text', ''), prof)
    elif cls in ('GuiMLTextCtrl',):
        text(canvas, r, f.get('text', ''), prof, vcenter=False)
    elif cls in ('GuiTextEditCtrl',):
        fill(canvas, r, color(pf(prof, 'fillColor', '255 255 255')))
        ImageDraw.Draw(canvas).rectangle((x, y, x + w - 1, y + h - 1), outline=(0, 0, 0, 255))
        text(canvas, (x + 2, y, w - 4, h), f.get('text', ''), prof)
    elif cls in ('GuiCheckBoxCtrl', 'GuiRadioCtrl'):
        im = load(pf(prof, 'bitmap'))
        if im is not None:
            b = bitmap_array(im); idx = 1 if f.get('value') in (1, '1') else 0
            if b:
                bx = b[min(idx, len(b) - 1)]; piece = im.crop((bx[0], bx[1], bx[0] + bx[2], bx[1] + bx[3]))
                blit(canvas, piece, (x, y + (h - bx[3]) // 2, bx[2], bx[3]))
                text(canvas, (x + bx[2] + 3, y, w - bx[2] - 3, h), f.get('text', ''), prof)
    elif cls == 'GuiPopUpMenuCtrl':
        fill(canvas, r, color(pf(prof, 'fillColor', '149 152 166')))
        d = ImageDraw.Draw(canvas); d.rectangle((x, y, x + w - 1, y + h - 1), outline=(0, 0, 0, 255))
        d.polygon([(x + w - 12, y + h // 2 - 2), (x + w - 4, y + h // 2 - 2), (x + w - 8, y + h // 2 + 3)], fill=(0, 0, 0, 255))
        if f.get('text'): text(canvas, (x + 3, y, w - 16, h), f['text'], prof, justify='left')
    elif cls in ('GuiScrollCtrl',):
        fill(canvas, r, color(pf(prof, 'fillColor', '255 255 255 0'), (255, 255, 255, 0)))
        if str(f.get('vScrollBar', '')) in ('alwaysOn', 'dynamic'):
            fill(canvas, (x + w - 12, y, 12, h), (200, 200, 210, 255))
    elif cls == 'GuiSliderCtrl':
        d = ImageDraw.Draw(canvas); d.line((x + 4, y + h // 2, x + w - 4, y + h // 2), fill=(0, 0, 0, 255), width=2)
        d.rectangle((x + w // 2 - 3, y + h // 2 - 7, x + w // 2 + 3, y + h // 2 + 7), fill=(149, 152, 166, 255), outline=(0, 0, 0, 255))
    elif cls in ('GuiProgressCtrl',):
        fill(canvas, r, color(pf(prof, 'fillColor', '0 0 128 128')))
    elif cls in ('GameTSCtrl', 'GuiObjectView'):
        if cls == 'GuiObjectView':
            fill(canvas, r, (60, 60, 70, 200)); text(canvas, r, '[3D avatar preview]', 'GuiTextProfile', justify='center', col=(255, 255, 255, 255))
    elif cls in ('GuiButtonCtrl',):
        fill(canvas, r, (220, 220, 220, 255)); text(canvas, r, f.get('text', ''), prof, justify='center')

SETS = [a.split('=', 1)[1] for a in sys.argv if a.startswith('--set=')]
def apply_sets(o):
    for spec in SETS:
        target, val = spec.split('=', 1)
        nm, fld = target.rsplit('.', 1)
        if o['name'] == nm:
            o['fields'][fld] = int(val) if val.lstrip('-').isdigit() else val
    for c in o['children']: apply_sets(c)
for objs in data.values():
    for o in objs: apply_sets(o)

def find(nm):
    for objs in data.values():
        for o in objs:
            if o['name'] == nm: return o

for nm in names:
    g = find(nm)
    if not g: print('not found', nm); continue
    # Canvas forces every top-level control to the canvas size (guiCanvas.cpp:1535 maintainSizing)
    ext = pair(g['fields'].get('extent', '640 480'))
    root = {'o': dict(g, fields=dict(g['fields'], position='0 0', extent=f'{ext[0]} {ext[1]}')), 'kids': []}
    top = layout(dict(g, fields=dict(g['fields'], position='0 0', horizSizing='width', vertSizing='height')), ext, (W, H), 0, 0, [])
    base = Image.new('RGBA', (W, H), (26, 30, 38, 255))
    if BG and (nm == 'MainMenuGui' or nm.startswith('Dyn_HUD')):   # MM slideshow frame / 3D view stand-in
        base.alpha_composite(Image.open(BG).convert('RGBA').resize((W, H)))
    ANN.clear()
    draw(base, top)
    base.save(os.path.join(OUT, f'{nm}_{W}x{H}.png'))
    if ANNOTATE:
        a = base.copy(); d = ImageDraw.Draw(a)
        for (x, y, w, h), lab in ANN:
            d.rectangle((x, y, x + w - 1, y + h - 1), outline=(255, 0, 255, 200))
            d.text((x + 2, y + 1), lab, fill=(255, 0, 255, 255))
        a.save(os.path.join(OUT, f'{nm}_{W}x{H}_annotated.png'))
    print('wrote', nm)
