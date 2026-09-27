#!/usr/bin/env python3
"""Build a manifest of assets referenced by the v20 client UI sources.

Scans decompiled vanilla GUI/client scripts (and selected server script lines
for tool/UI sounds) for literal asset paths, resolves them against the
original installation READ-ONLY, and records existence, image size and
SHA-256. GuiBitmapButtonCtrl bitmaps are expanded to their _n/_h/_d/_i state
images, which the engine loads implicitly.

Usage: asset_manifest.py <v20_root> <out_json> <out_md> <src>...
"""
import re, sys, os, json, hashlib
from collections import defaultdict
try:
    from PIL import Image
except ImportError:
    Image = None

root, out_json, out_md, srcs = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4:]
UI_DIR = 'base/client/ui'
EXTS = ['', '.png', '.jpg', '.jpeg', '.bmp', '.gif', '.wav', '.ogg', '.gft', '.dts', '.txt', '.ifl']
STR = re.compile(r'"((?:\./|~/|base/|Add-Ons/|config/)[^"$%@]*?)"')
BUTTON_STATES = ['_n', '_h', '_d', '_i']

def resolve(ref, src):
    """Map a script path to an install-relative path (TorqueScript conventions)."""
    r = ref.replace('\\', '/')
    if r.startswith('./'):
        base = UI_DIR if 'Guis' in os.path.basename(src) or src.endswith('.gui') else 'base/client/scripts'
        r = base + '/' + r[2:]
    elif r.startswith('~/'):
        r = 'base/' + r[2:]          # "~" = mod root of the executing script (base)
    return os.path.normpath(r).replace('\\', '/')

def find(rel):
    rel = rel.split(' ')[0] if re.search(r'\.dsq ', rel) else rel   # "m_run.dsq run" sequence alias
    for e in EXTS:
        if (rel + e).lower() in ZIPS:
            return 'zip:' + rel + e
        p = os.path.join(root, rel + e)
        if os.path.isfile(p):
            return rel + e
    # case-insensitive fallback (Windows install, Linux mount)
    d, b = os.path.split(rel)
    full = os.path.join(root, d)
    if os.path.isdir(full):
        low = {f.lower(): f for f in os.listdir(full)}
        for e in EXTS:
            if (b + e).lower() in low and os.path.isfile(os.path.join(full, low[(b + e).lower()])):
                return os.path.join(d, low[(b + e).lower()]).replace('\\', '/')
    return None

import zipfile
ZIPS = {}   # lowercased virtual path -> (zip file, member)
addons = os.path.join(root, 'Add-Ons')
for f in sorted(os.listdir(addons)) if os.path.isdir(addons) else []:
    if f.lower().endswith('.zip'):
        try:
            z = zipfile.ZipFile(os.path.join(addons, f))
        except zipfile.BadZipFile:
            continue                      # e.g. RAR renamed .zip; reported by inventory
        for m in z.namelist():
            if not m.endswith('/'):
                ZIPS[f'add-ons/{f[:-4]}/{m}'.lower()] = (os.path.join(addons, f), m)

refs = defaultdict(lambda: {'sources': set(), 'kinds': set()})
for src in srcs:
    lines = open(src, 'rb').read().decode('utf-8', 'replace').replace('\r', '').split('\n')
    cls = None
    for n, line in enumerate(lines, 1):
        m = re.match(r'\s*new\s+(\w+)\(', line)
        if m: cls = m.group(1)
        for s in STR.findall(line):
            if s.endswith('/') or '*' in s:
                continue
            rel = resolve(s, src)
            kind = 'button' if (cls == 'GuiBitmapButtonCtrl' and re.match(r'\s*bitmap\s*=', line)) else 'direct'
            if 'setBitmap' in line and 'BitmapButton' in ''.join(lines[max(0, n-15):n]):
                kind = 'button?'
            refs[rel]['sources'].add(f'{os.path.relpath(src)}:{n}')
            refs[rel]['kinds'].add(kind)

def info(path):
    if path.startswith('zip:'):
        zf, member = ZIPS[path[4:].lower()]
        data = zipfile.ZipFile(zf).read(member)
        d = {'path': path, 'zip': os.path.relpath(zf, root), 'member': member,
             'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
        if Image and member.lower().endswith(('.png', '.jpg', '.jpeg')):
            import io
            try:
                with Image.open(io.BytesIO(data)) as im:
                    d['size'] = list(im.size); d['mode'] = im.mode
            except Exception as ex:
                d['image_error'] = str(ex)
        return d
    full = os.path.join(root, path)
    d = {'path': path, 'bytes': os.path.getsize(full),
         'sha256': hashlib.sha256(open(full, 'rb').read()).hexdigest()}
    if Image and path.lower().endswith(('.png', '.jpg', '.jpeg', '.bmp', '.gif')):
        try:
            with Image.open(full) as im:
                d['size'] = list(im.size); d['mode'] = im.mode
        except Exception as e:
            d['image_error'] = str(e)
    return d

entries = []
for rel, v in sorted(refs.items()):
    e = {'ref': rel, 'sources': sorted(v['sources']), 'kinds': sorted(v['kinds'])}
    hit = find(rel)
    e['resolved'] = info(hit) if hit else None
    if hit is None and find(rel + '_00'):
        frames = []
        i = 0
        while find(rel + f'_{i:02d}'):
            frames.append(info(find(rel + f'_{i:02d}'))); i += 1
        e['frames'] = frames
    if 'button' in v['kinds'] or 'button?' in v['kinds'] or (hit is None and any(find(rel + s) for s in BUTTON_STATES)):
        e['states'] = {s: (info(find(rel + s)) if find(rel + s) else None) for s in BUTTON_STATES}
    entries.append(e)

# UI files never referenced literally (may be referenced dynamically)
referenced = set()
for e in entries:
    if e['resolved']: referenced.add(e['resolved']['path'].lower())
    for fr in e.get('frames') or []: referenced.add(fr['path'].lower())
    for s in (e.get('states') or {}).values():
        if s: referenced.add(s['path'].lower())
orphans = []
for dp, dn, fn in os.walk(os.path.join(root, UI_DIR)):
    for f in fn:
        rel = os.path.relpath(os.path.join(dp, f), root).replace('\\', '/')
        if rel.lower() not in referenced and not f.endswith(('.dso', '.gui', '.cs')):
            orphans.append(rel)

def status(e):
    if e['resolved']: return 'ok'
    if e.get('frames'): return 'ok-frames'
    r = e['ref']
    if r.startswith('config/'): return 'runtime-file'          # created at runtime by the game
    if os.path.isfile(os.path.join(root, r + '.dso')): return 'compiled-script'
    if os.path.isdir(os.path.join(root, r)): return 'directory'
    if r.startswith('base/help/') or r.startswith('base/client/help/'): return 'missing-editor-help'
    st = e.get('states') or {}
    if st and all(st.values()): return 'ok-states'
    if st and any(st.values()): return 'partial-states'
    return 'missing'

for e in entries: e['status'] = status(e)
summary = defaultdict(int)
for e in entries: summary[e['status']] += 1
json.dump({'install_root_note': 'read-only; paths are install-relative',
           'summary': summary, 'entries': entries, 'unreferenced_ui_files': sorted(orphans)},
          open(out_json, 'w'), indent=1, default=list)

with open(out_md, 'w') as o:
    o.write('# Generated UI asset reference manifest\n\n')
    o.write('Generated by `tools/asset_manifest.py` from the vanilla decompiled GUI and client scripts; resolved read-only against the v20 install. `L` references use repo-relative source paths. Do not hand-edit; regenerate.\n\n')
    o.write('Summary: ' + ', '.join(f'{k}={v}' for k, v in sorted(summary.items())) + f'; unreferenced files under `{UI_DIR}`: {len(orphans)}\n\n')
    o.write('| Status | Reference | Resolved file | Size | First source |\n|---|---|---|---|---|\n')
    for e in entries:
        r = e['resolved']
        size = ('x'.join(map(str, r['size'])) if r and 'size' in r else '')
        if not r and e.get('states'):
            ok = [s for s, v in e['states'].items() if v]
            rf = 'states: ' + ','.join(ok)
            first = next((v for v in e['states'].values() if v), None)
            if first and 'size' in first: size = 'x'.join(map(str, first['size']))
        elif not r and e.get('frames'):
            rf = f"{len(e['frames'])} frames {e['frames'][0]['path']}…"
            size = 'x'.join(map(str, e['frames'][0].get('size', [])))
        else:
            rf = r['path'] if r else '—'
        src = e['sources'][0].replace('.research/v20-dso/', '') + (f' (+{len(e["sources"])-1})' if len(e['sources']) > 1 else '')
        o.write(f"| {e['status']} | `{e['ref']}` | {rf} | {size} | {src} |\n")
print(dict(summary), 'orphans', len(orphans))
