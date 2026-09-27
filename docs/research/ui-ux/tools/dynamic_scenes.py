#!/usr/bin/env python3
"""Synthesize the GUI trees that v20 builds at RUNTIME in script, so they can be
rendered with gui_render.py. Every synthetic control carries the source line of
the script statement that creates/positions it (allClientScripts-Vanilla.cs).

Scenes (names are prefixed Dyn_ to distinguish them from authored GUIs):
  Dyn_BSD_Tab0 / Dyn_BSD_Tab1  BrickSelectorDlg after BSD_LoadBricks + favorites 1 in cart
  Dyn_HUD_Idle / _Bricks / _Paint / _Tools   PlayGui HUD states at a given resolution
  Dyn_WrenchEvents             wrenchEventsDlg with three example rows

Usage: dynamic_scenes.py static.json stock-catalog.json WxH out.json
Assumptions (also in engine-behavior.md): catalog order == datablock order;
default colorset and favorites from stock scripts; profile defaulting per
Torque3D guiControl.cpp:688 (Gui<Class>Profile, else GuiDefaultProfile).
"""
import json, sys, copy
static = json.load(open(sys.argv[1])); catalog = json.load(open(sys.argv[2]))['bricks']
W, H = map(int, sys.argv[3].split('x')); OUT = sys.argv[4]
SRC = 'allClientScripts-Vanilla.cs'

def node(cls, line, name=None, children=None, **fields):
    f = {k: (' '.join(map(str, v)) if isinstance(v, (tuple, list)) else v) for k, v in fields.items()}
    f.setdefault('horizSizing', 'right'); f.setdefault('vertSizing', 'bottom'); f.setdefault('minExtent', '0 0')
    if 'profile' not in f:
        guess = {'GuiCheckBoxCtrl': 'GuiCheckBoxProfile', 'GuiTextEditCtrl': 'GuiTextEditProfile',
                 'GuiPopUpMenuCtrl': 'GuiPopUpMenuProfile', 'GuiScrollCtrl': 'GuiScrollProfile',
                 'GuiTextCtrl': 'GuiTextProfile', 'GuiSliderCtrl': 'GuiSliderProfile'}
        f['profile'] = guess.get(cls, 'GuiDefaultProfile')
    return {'class': cls, 'name': name, 'line': line, 'file': SRC, 'fields': f, 'children': children or []}

def rect(x, y, w, h): return {'position': (x, y), 'extent': (w, h)}
def find(name):
    for objs in static.values():
        for o in objs:
            if o['name'] == name: return copy.deepcopy(o)
def child(o, name):
    if o['name'] == name: return o
    for c in o['children']:
        r = child(c, name)
        if r: return r

# ---- stock data (scripts) ----
FAV1 = ["32x32 Base", "2x4", "2x2", "1x4", "1x2", "1x1", "1x16", "6x12F", "1x4x5 Window", "Vehicle Spawn"]  # defaults.cs:252-261
by_ui = {b['display_name']: b for b in catalog}
def icon(ui):
    b = by_ui.get(ui); return (b['icon_source'] if b and b.get('icon_source') else 'base/client/ui/brickIcons/unknown')
PALETTE = [  # setSprayCanColors() default colorset, allGameScripts-Vanilla.cs:12473-12515
 "0.900 0.000 0.000 1.000","0.900 0.900 0.000 1.000","0.000 0.500 0.250 1.000","0.200 0.000 0.800 1.000",
 "0.900 0.900 0.900 1.000","0.750 0.750 0.750 1.000","0.500 0.500 0.500 1.000","0.200 0.200 0.200 1.000","100 50 0 255",
 "230 87 20 255","191 46 123 255","99 0 30 255","34 69 69 255","0 36 85 255","27 117 196 255","255 255 255 255","20 20 20 255","255 255 255 64",
 "236 131 173 255","255 154 108 255","255 224 156 255","244 224 200 255","200 235 125 255","138 178 141 255","143 237 245 255","178 169 231 255","224 143 244 255",
 "0.667 0.000 0.000 0.700","1.000 0.500 0.000 0.700","0.990 0.960 0.000 0.700","0.000 0.471 0.196 0.700","0.000 0.200 0.640 0.700","152 41 100 178","0.550 0.700 1.000 0.700","0.850 0.850 0.850 0.700","0.100 0.100 0.100 0.700"]
DIVS = [(8, 'Standard'), (17, 'Bold'), (26, 'Soft'), (35, 'Transparent')]
def fcolor(c):
    p = [float(x) for x in c.split()]
    if any(x > 1 for x in p): p = [x / 255 for x in p]
    return p
def fstr(p): return ' '.join(f'{x:.3f}' for x in p)

# ---- Brick selector ----
def bsd(tab):
    g = find('BrickSelectorDlg'); win = child(g, 'BSD_Window')
    cats, subs, order = [], {}, []
    for b in catalog:                                   # BSD_LoadBricks 9773-9800 (datablock order)
        c, sc = b['category'], b['subcategory']
        if not c or not sc or not b['display_name']: continue
        if c not in cats: cats.append(c); subs[c] = []
        if sc not in subs[c]: subs[c].append(sc)
        order.append(b)
    tabs = node('GuiControl', 9810, 'BSD_TabBox', **rect(3, 30, 634, 25))
    for i, c in enumerate(cats):                        # 9820-9830
        tabs['children'].append(node('GuiBitmapButtonCtrl', 9820, None, profile='BlockButtonProfile',
            bitmap='base/client/ui/tab1use' if i == tab else 'base/client/ui/tab1', text=c, **rect(i * 80, 0, 80, 25)))
    sbox = node('GuiControl', 9802, 'BSD_ScrollBox', **rect(3, 57, 634, 363))
    cat = cats[tab]
    box = node('GuiControl', 9843, None, profile='ColorScrollProfile', **rect(0, 0, 634, 2))
    height = 2; start = {}
    for sc in subs[cat]:                                # BSD_createSubHeadings 10015-10047
        st = height - 2; start[sc] = st
        n = sum(1 for b in order if b['category'] == cat and b['subcategory'] == sc)
        box['children'].append(node('GuiTextCtrl', 10033, None, profile='BlockButtonProfile', text=sc, **rect(18, st - 2, 100, 18)))
        height = st + 18 + (-(-n // 6)) * 96 + 5 + 2     # +2: minExtent height subtracted on next pass
    count = {sc: 0 for sc in subs[cat]}
    for b in order:                                     # BSD_CreateBrickButton 10051-10112
        if b['category'] != cat: continue
        sc = b['subcategory']; k = count[sc]; count[sc] += 1
        x = (k % 6) * 97 + 18; y = (k // 6) * 97 + start[sc] + 18
        box['children'] += [
            node('GuiBitmapCtrl', 10076, None, profile='BlockDefaultProfile', bitmap='base/client/ui/brickicons/brickiconbg', **rect(x, y, 96, 96)),
            node('GuiBitmapCtrl', 10082, None, profile='BlockDefaultProfile', bitmap=b.get('icon_source') or 'base/client/ui/brickIcons/unknown', **rect(x, y, 96, 96)),
            node('GuiBitmapButtonCtrl', 10096, None, profile='BlockButtonProfile', bitmap='base/client/ui/brickicons/brickIconBtn', text=' ', **rect(x, y, 96, 96)),
            node('GuiTextCtrl', 10104, None, profile='HUDBSDNameProfile', text=b['display_name'], justify='center', **rect(x, y + 96 - 18, 96, 18))]
    box['fields']['extent'] = f'634 {height}'
    scroll = node('GuiScrollCtrl', 9831, None, profile='BSDScrollProfile', vScrollBar='alwaysOn', hScrollBar='alwaysOff', **rect(0, 0, 634, 363), children=[box])
    sbox['children'].append(scroll)
    bw = 617 // 11 - 1                                  # BSD_CreateInventoryButtons 10123-10168
    inv = node('GuiSwatchCtrl', 10128, 'BSD_InvBox', color='0.2 0.5 1 1', **rect(3, 480 - bw - 4, 617 - 58, bw))
    for i, ui in enumerate(FAV1):                       # favorites 1 loaded (BSD_BuyFavorites 10435)
        x = i * (bw + 1)
        inv['children'] += [node('GuiBitmapCtrl', 10144, None, bitmap='base/client/ui/brickicons/brickiconbg', **rect(x, 0, bw, bw)),
                            node('GuiBitmapCtrl', 10149, None, profile='HUDBitmapProfile', bitmap=icon(ui), **rect(x, 0, bw, bw)),
                            node('GuiBitmapButtonCtrl', 10162, None, profile='BlockButtonProfile', bitmap='base/client/ui/brickicons/brickIconBtn', text=' ', **rect(x, 0, bw, bw))]
    # pushToBack(BSD_ClearBtn) 9878 keeps Clear Cart drawn above the inventory box
    clear = child(win, 'BSD_ClearBtn'); win['children'].remove(clear)
    win['children'] += [tabs, sbox, inv, clear]
    g['name'] = f'Dyn_BSD_Tab{tab}'
    return g, cats

# ---- HUD ----
def hud(mode):
    g = find('PlayGui'); g['name'] = f'Dyn_HUD_{mode}'
    child(g, 'HUD_Ghosting')['fields']['visible'] = 0   # hidden once initial ghosting ends (clientCmdSetLoadingIndicator 6910)
    iw = min(64, W // 10); bw = 10 * iw               # createInvHud 6027-6190
    cur = fcolor(PALETTE[0]); tint = fstr(cur[:3] + [max(0.1, min(1, cur[3]))])
    by = H - iw + (0 if mode == 'Bricks' else 64)     # hideBrickBox(64) when not in brick mode (6200/4866)
    bb = node('GuiSwatchCtrl', 6041, 'Hud_BrickBox', profile='HUDBitmapProfile', color='0 0 0 0.25', **rect(W // 2 - bw // 2, by, bw, iw))
    for i, ui in enumerate(FAV1):
        bb['children'] += [node('GuiSwatchCtrl', 6066, None, color='0 0 0 0.25', **rect(i * iw + 2, 4, iw - 4, iw - 8)),
                           node('GuiBitmapCtrl', 6075, None, profile='HUDBitmapProfile', bitmap=icon(ui), mColorTint=tint, **rect(i * iw, 0, iw, iw)),
                           node('GuiTextCtrl', 6105, None, profile='HUDBrickNameProfile', text=str((i + 1) % 10), **rect(i * iw, 2, 16, 18))]
    if mode == 'Bricks':
        bb['children'].append(node('GuiBitmapCtrl', 6052, 'HUD_BrickActive', profile='HUDBitmapProfile', bitmap='base/client/ui/brickIcons/brickIconActive', **rect(64, 0, 64, 64)))
    nb = node('GuiSwatchCtrl', 6116, 'HUD_BrickNameBG', profile='HUDBrickNameProfile', color='0 0 0.5 0', **rect(W // 2 - bw // 2, by - 18, bw, 18))
    nb['children'] = [node('GuiBitmapCtrl', 6126, None, bitmap='base/client/ui/BlueHudLeftCorner', **rect(0, 0, 10, 18)),
                      node('GuiBitmapCtrl', 6131, None, bitmap='base/client/ui/BlueHudRightCorner', **rect(bw - 10, 0, 10, 18)),
                      node('GuiSwatchCtrl', 6136, None, color='0 0 0.5 0.5', **rect(10, 0, bw - 20, 18)),
                      node('GuiTextCtrl', 6145, 'HUD_BrickName', profile='HUDBrickNameProfile', justify='center', text=('2x4' if mode == 'Bricks' else ''), **rect(0, 0, bw, 18)),
                      node('GuiTextCtrl', 6154, 'ToolTip_BSD', profile='HUDRightTextProfile', justify='right', text='Press B for more bricks   ', **rect(0, 0, bw, 18)),
                      # useFirstSlot is unbound by default, so the hint reads "1 or  2 3 ..." (6168-6179)
                      node('GuiTextCtrl', 6165, 'ToolTip_Bricks', profile='HUDLeftTextProfile', text='  Press 1 or  2 3 4 5 6 7 8 9 0 to use bricks', **rect(0, 0, bw, 18))]
    # paint box (PlayGui::LoadPaint 6240-6590), 4 divisions + FX column
    sw = 16; ndiv = 5; boxw = ndiv * (sw + 1) + 1; boxh = 9 * (sw + 1) + 1
    px = 0 if mode == 'Paint' else -((boxw + 100 - 100) + 5)           # hidePaintBox 6580
    pb = node('GuiSwatchCtrl', 6286, 'HUD_PaintBox', profile='HUDBitmapProfile', color='0 0 0 0', **rect(px, H - boxh - 18, boxw + 100, boxh + 18))
    pb['children'] += [node('GuiBitmapCtrl', 6296, None, bitmap='base/client/ui/paintLabelBG', **rect(boxw - 14, 0, 100, 100)),
                       node('GuiBitmapCtrl', 6305, None, bitmap='base/client/ui/paintLabelBGLoop', wrap=1, **rect(boxw - 14, 100, 100, 100)),
                       node('GuiBitmapCtrl', 6315, 'HUD_PaintIcon', bitmap='base/client/ui/paintLabel', mColorTint=fstr(cur), **rect(boxw - 14, 0, 100, 100)),
                       node('GuiBitmapCtrl', 6332, None, bitmap='base/client/ui/paintLabelTop', **rect(boxw - 14, 0, 100, 100)),
                       node('GuiSwatchCtrl', 6341, None, color='0 0 0 0.25', **rect(0, 18, boxw, boxh))]
    div = 0
    for i, c in enumerate(PALETTE):
        if i > DIVS[div][0]: div += 1
        n = i - DIVS[div - 1][0] - 1 if div > 0 else i
        p = fcolor(c)
        if not (mode == 'Paint' and div == 0): p = p[:3] + [p[3] * 0.3]      # FadePaintRow 6651
        pb['children'].append(node('GuiSwatchCtrl', 6366, None, profile='BlockDefaultProfile', color=fstr(p), **rect(17 * div + 1, 17 * n + 1 + 18, 16, 16)))
    pb['children'].append(node('GuiSwatchCtrl', 6387, None, color='0.2 0.2 0.2 0.3', **rect(17 * 4 + 1, 19, 16, 16)))
    for k, fx in enumerate(['FXpearl', 'FXchrome', 'FXglow', 'FXblink', 'FXswirl', 'FXrainbow', 'FXstable', 'FXjello'], 1):
        pb['children'].append(node('GuiBitmapCtrl', 6398 + 12 * (k - 1), None, bitmap=f'base/client/ui/{fx}', **rect(17 * 4 + 1, 17 * k + 1 + 18, 16, 16)))
    if mode == 'Paint':
        pb['children'].append(node('GuiBitmapCtrl', 6493, 'HUD_PaintActive', bitmap='base/client/ui/paintActive', **rect(0, 18, 18, 18)))
    tl = node('GuiBitmapCtrl', 6541, 'ToolTip_Paint', profile='HUDBrickNameProfile', bitmap='base/client/ui/ItemIcons/toolLabelBG', **rect(boxw + 4, 61, 64, 18))
    tl['children'].append(node('GuiTextCtrl', 6551, None, profile='HUDCenterTextProfile', justify='center', text='E = Paint', **rect(0, 0, 64, 18)))
    if mode != 'Paint': pb['children'].append(tl)      # tooltip shown while box is hidden (4890)
    pn = node('GuiSwatchCtrl', 6503, 'HUD_PaintNameBG', color='0 0 0.5 0', **rect(px, H - boxh - 18, boxw, 18))
    pn['children'] = [node('GuiBitmapCtrl', 6513, None, bitmap='base/client/ui/BlueHudLeftCorner', **rect(0, 0, 10, 18)),
                      node('GuiBitmapCtrl', 6518, None, bitmap='base/client/ui/BlueHudRightCorner', **rect(boxw - 10, 0, 10, 18)),
                      node('GuiSwatchCtrl', 6523, None, color='0 0 0.5 0.5', **rect(10, 0, boxw - 20, 18)),
                      node('GuiTextCtrl', 6532, 'HUD_PaintName', profile='HUDCenterTextProfile', justify='center', text=('Standard - 1' if mode == 'Paint' else ''), **rect(0, 0, boxw, 18))]
    # tool box (createToolHud 6704-6836): 5 slots, hammer/wrench/printer
    ty = 0 if mode == 'Tools' else -5 * 64
    tb = node('GuiBitmapCtrl', 6719, 'HUD_ToolBox', profile='HUDBitmapProfile', bitmap='base/client/ui/itemIcons/ToolBG', wrap=1, **rect(W - iw, ty, iw, 5 * iw))
    for i, ic in enumerate(['Hammer', 'wrench', 'Printer']):
        tb['children'].append(node('GuiBitmapCtrl', 6743, None, profile='HUDBitmapProfile', bitmap=f'base/client/ui/itemIcons/{ic}', **rect(0, i * iw, iw, iw)))
    if mode == 'Tools':
        tb['children'].append(node('GuiBitmapCtrl', 6730, 'HUD_ToolActive', bitmap='base/client/ui/itemIcons/ItemActive', **rect(0, 0, iw, iw)))
    tn = node('GuiBitmapCtrl', 6781, 'HUD_ToolNameBG', profile='HUDBrickNameProfile', bitmap='base/client/ui/ItemIcons/toolLabelBG', **rect(W - iw, ty + 5 * iw, iw, 18))
    tn['children'] = [node('GuiTextCtrl', 6791, 'HUD_ToolName', profile='HUDCenterTextProfile', justify='center', text=('Hammer' if mode == 'Tools' else ''), **rect(0, 0, iw, 18))]
    if mode != 'Tools':
        tn['children'].append(node('GuiTextCtrl', 6800, 'ToolTip_Tools', profile='HUDCenterTextProfile', justify='center', text='Q = tools', **rect(0, 0, iw, 18)))
    # chat (NewChatHud pushed by PlayGui::onWake 5972; ChatSize 4 -> BlockChatTextSize4Profile, 5554)
    chat = node('GuiMLTextCtrl', 19723, 'newChatText', profile='BlockChatTextSize4Profile',
                text='<c1>Blockhead connected.\n<c7><c3>Blockhead<c7><c6>: hello!\n<c7><c3>Maxwell<c7><c6>: nice build', **rect(2, 20, W - 4, 90))
    g['children'] += [bb, nb, pb, pn, tb, tn, chat]
    return g

# ---- Events dialog rows (wrenchEventsDlg::newEvent 17953, createOutputParameters 18248) ----
def events():
    g = find('wrenchEventsDlg'); g['name'] = 'Dyn_WrenchEvents'
    box = child(g, 'WrenchEvents_Box'); rows = []
    spec = [(1, '0', 'onActivate', 'Self', 'setColor', 'paint'),
            (1, '1000', 'onActivate', 'Self', 'setRendering', 'bool'),
            (1, '0', '-', None, None, None)]
    for i, (en, delay, inp, tgt, out, par) in enumerate(spec):
        r = node('GuiSwatchCtrl', 17955, None, color='0 0 0 0.2', **rect(0, (36 + 3) * i, 768, 36))
        x = 0
        r['children'].append(node('GuiCheckBoxCtrl', 17965, None, text=str(i), value=en, **rect(0, 0, 36, 18))); x = 38
        r['children'].append(node('GuiTextEditCtrl', 17975, None, text=delay, **rect(x, 0, 36, 18))); x += 38
        r['children'].append(node('GuiPopUpMenuCtrl', 17985, None, text=inp, **rect(x, 0, 100, 18))); x += 102
        if tgt:
            r['children'].append(node('GuiPopUpMenuCtrl', 18080, None, text=tgt, **rect(x, 0, 100, 18))); x += 102
            r['children'].append(node('GuiPopUpMenuCtrl', 18209, None, text=out, **rect(x, 0, 100, 18))); x += 102
            if par == 'paint':
                sw = node('GuiSwatchCtrl', 18486, None, color=PALETTE[3], **rect(x, 0, 18, 18))
                sw['children'].append(node('GuiBitmapButtonCtrl', 18491, None, bitmap='base/client/ui/btnColor', text='', **rect(0, 0, 18, 18)))
                r['children'].append(sw)
            else:
                r['children'].append(node('GuiCheckBoxCtrl', 18332, None, text='', value=0, **rect(x, 0, 18, 18)))
        rows.append(r)
    box['children'] = rows
    box['fields']['extent'] = f'768 {(36 + 3) * len(rows)}'
    return g

out = {'dynamic-scenes': []}
for t in (0, 1):
    g, cats = bsd(t); out['dynamic-scenes'].append(g)
for m in ('Idle', 'Bricks', 'Paint', 'Tools'):
    out['dynamic-scenes'].append(hud(m))
out['dynamic-scenes'].append(events())
json.dump(out, open(OUT, 'w'), indent=1)
print('categories (tab order):', cats)
