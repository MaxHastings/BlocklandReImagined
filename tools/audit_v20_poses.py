#!/usr/bin/env python3
"""Compare every stock image's hand pose and state machine with v20's own files.

Reads the recovered core script and every stock add-on ZIP of a read-only v20
install, resolves each `ShapeBaseImageData` through its parents, and checks
what the game loads against it:

- the weapons pack (`weapons.json`): mountPoint, offset, eyeOffset, armReady,
  melee, correctMuzzleVector, and every state's name, timeout (seconds to
  ticks), wait flag, trigger/timeout/ammo transitions, script, sequence,
  sound, emitter, emitter node and emitter time;
- the item presentation pack (`presentation.json`): mountPoint, offset,
  eyeOffset, and the rotation and eyeRotation as written in the datablock
  (`eulerToMatrix("x y z")` or an axis-angle `"x y z deg"`).

    python tools/audit_v20_poses.py --v20 "E:/.../Blockland v20" \\
        --content ../BlocklandReImagined/content [--core ...allGameScripts-Vanilla.cs]

Prints one line per mismatch and a summary; exits 1 on any mismatch. Read
only: nothing is written inside the v20 folder or the content packs.
"""
import argparse
import json
import math
import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from audit_v20_fields import CORE, collect  # noqa: E402

TICK_HZ = 120
# The core script's slot globals.
SLOTS = {'$righthandslot': 0, '$lefthandslot': 1, '$backslot': 2, '$rightfootslot': 3,
         '$leftfootslot': 4, '$headslot': 5, '$visorslot': 6, '$hipslot': 7, '$weaponslot': 0}
STATE_FIELDS = re.compile(r'(?i)^state(\w+?)\s*\[\s*(\d+)\s*\]$')
ARRAY = re.compile(r'(?m)([A-Za-z_]\w*)\s*\[\s*(\d+)\s*\]\s*=\s*([^;]*);')


def unquote(v):
    v = v.strip()
    return v[1:-1] if len(v) >= 2 and v[0] == v[-1] == '"' else v


def floats(v, n=3):
    try:
        out = [float(x) for x in unquote(v).split()]
    except ValueError:
        return None
    return (out + [0.0] * n)[:n]


def truthy(v):
    v = unquote(v).lower()
    return v not in ('', '0', 'false')


def resolve(blocks, name, seen=()):
    b = blocks.get(name.lower())
    if not b or name.lower() in seen:
        return {}
    fields = dict(resolve(blocks, b['parent'], seen + (name.lower(),))) if b['parent'] else {}
    fields.update(b['fields'])
    return fields


def arrays(blocks, name, text_of):
    """`state...[n]` fields, which `collect` folds to their first index."""
    b = blocks.get(name.lower())
    if not b:
        return {}
    out = dict(arrays(blocks, b['parent'], text_of)) if b['parent'] else {}
    for field, index, value in ARRAY.findall(text_of(b)):
        out[(field.lower(), int(index))] = value.strip()
    return out


def rotation(value):
    """Datablock rotation text -> ('euler', xyz) or ('axis', xyzw)."""
    if value is None:
        return None
    m = re.match(r'(?i)\s*eulerToMatrix\s*\(\s*"([^"]*)"\s*\)', value)
    if m:
        return ('euler', floats(m.group(1)))
    v = floats(value, 4)
    return ('axis', v) if v else None


def axis_to_euler_like(axis):
    """An axis-angle rotation expressed as the pack's Euler degrees, when it is
    a single-axis turn (the only kind stock images use)."""
    x, y, z, deg = axis
    if deg == 0:
        return [0.0, 0.0, 0.0]
    for i, c in enumerate((x, y, z)):
        others = [abs(o) for j, o in enumerate((x, y, z)) if j != i]
        if abs(c) > 0 and max(others) == 0:
            out = [0.0, 0.0, 0.0]
            out[i] = deg if c > 0 else -deg
            return out
    return None


def native(v):
    """Torque (x right, y forward, z up) to the packs' native basis."""
    return None if v is None else [v[0], v[2], -v[1]]


def close(a, b, tol=1e-4):
    return a is not None and b is not None and len(a) == len(b) and all(
        abs(x - y) <= tol for x, y in zip(a, b))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--v20', required=True, type=pathlib.Path)
    ap.add_argument('--content', required=True, type=pathlib.Path)
    ap.add_argument('--core', type=pathlib.Path, default=CORE)
    ap.add_argument('--weapons', default='weapons-pack-009')
    ap.add_argument('--presentation', default='item-presentation-pack-010')
    args = ap.parse_args()

    raw = {}

    def remember(v20, core):
        from audit_v20_fields import sources, strip_comments, BLOCK
        for origin, text in sources(v20, core):
            for m in BLOCK.finditer(strip_comments(text)):
                raw[m.group(2).lower()] = m.group(4)
    remember(args.v20, args.core)
    blocks = collect(args.v20, args.core)
    weapons = json.loads((args.content / args.weapons / 'weapons.json').read_text(encoding='utf-8'))
    presentation = json.loads(
        (args.content / args.presentation / 'presentation.json').read_text(encoding='utf-8'))
    images = {i['name'].lower(): i for i in weapons['images'].values()}
    shown = presentation['images']
    problems = []

    def bad(name, what, v20, ours):
        problems.append(f'{name}: {what}: v20 {v20!r}, ours {ours!r}')

    checked = 0
    for key, b in sorted(blocks.items()):
        if b['cls'] != 'shapebaseimagedata':
            continue
        image = images.get(key)
        if image is None:
            continue
        checked += 1
        name = b['name']
        f = resolve(blocks, name)
        arr = arrays(blocks, name, lambda blk: raw.get(blk['name'].lower(), ''))
        mount = unquote(f.get('mountpoint', '0'))
        mount = SLOTS.get(mount.lower(), mount)
        mount = int(float(mount))
        offset = native(floats(f.get('offset', '0 0 0')))
        eye = native(floats(f.get('eyeoffset', '0 0 0')))
        if image['mount_point'] != mount:
            bad(name, 'mountPoint', mount, image['mount_point'])
        if not close(image['offset'], offset):
            bad(name, 'offset', offset, image['offset'])
        if not close(image['eye_offset'], eye):
            bad(name, 'eyeOffset', eye, image['eye_offset'])
        for field, ours in [('armready', image['arm_ready']), ('melee', image['melee']),
                            ('correctmuzzlevector', image['correct_muzzle'])]:
            want = truthy(f.get(field, '1' if field == 'correctmuzzlevector' else '0'))
            # Only an image that fires a projectile corrects its muzzle.
            if field == 'correctmuzzlevector' and not image['projectile']:
                continue
            if want != ours:
                bad(name, field, want, ours)
        p = shown.get(image['id'])
        if p is not None:
            if p['mount_point'] != mount:
                bad(name, 'presentation mountPoint', mount, p['mount_point'])
            if not close(p['offset'], offset):
                bad(name, 'presentation offset', offset, p['offset'])
            if not close(p['eye_offset'], eye):
                bad(name, 'presentation eyeOffset', eye, p['eye_offset'])
            for field, ours in [('rotation', p['source_rotation_degrees']),
                                ('eyerotation', p['eye_rotation_degrees'])]:
                r = rotation(f.get(field))
                want = [0.0, 0.0, 0.0] if r is None else (
                    r[1] if r[0] == 'euler' else axis_to_euler_like(r[1]))
                if want is None:
                    bad(name, f'{field} (multi-axis angle, check by hand)', r, ours)
                elif not close(want, ours):
                    bad(name, field, f.get(field), ours)
        else:
            bad(name, 'presentation', 'image', None)
        # State machine.
        names = {}
        for (field, i), v in arr.items():
            if field == 'statename':
                names[unquote(v).lower()] = i
        states = image['states']
        count = max(names.values()) + 1 if names else 0
        if len(states) != count:
            bad(name, 'state count', count, len(states))
        for i, s in enumerate(states):
            def get(field, default=''):
                return arr.get((field, i), default)
            if unquote(get('statename')).lower() != s['name'].lower():
                bad(name, f'state {i} name', unquote(get('statename')), s['name'])
            seconds = float(unquote(get('statetimeoutvalue', '0')) or 0)
            # Timeouts round up to whole ticks.
            if s['ticks'] != math.ceil(seconds * TICK_HZ - 1e-6):
                bad(name, f'{s["name"]} timeout', seconds, s['ticks'] / TICK_HZ)
            if truthy(get('statewaitfortimeout', '1')) != s['wait']:
                bad(name, f'{s["name"]} waitForTimeout', get('statewaitfortimeout', '1'), s['wait'])
            for field, key2 in [('statetransitionontimeout', 'timeout'),
                                ('statetransitionontriggerdown', 'down'),
                                ('statetransitionontriggerup', 'up'),
                                ('statetransitiononammo', 'ammo'),
                                ('statetransitiononnoammo', 'no_ammo')]:
                target = unquote(get(field)).lower()
                want = names.get(target) if target else None
                if want != s[key2]:
                    bad(name, f'{s["name"]} {field[15:]}', target or None,
                        states[s[key2]]['name'] if s[key2] is not None else None)
            for field, key2 in [('statescript', 'script'), ('statesequence', 'sequence'),
                                ('statesound', 'sound'), ('stateemitter', 'emitter'),
                                ('stateemitternode', 'emitter_node')]:
                want = unquote(get(field)).lower()
                if want != (s[key2] or '').lower():
                    bad(name, f'{s["name"]} {field[5:]}', want, s[key2])
            et = float(unquote(get('stateemittertime', '0')) or 0)
            if abs(et - s['emitter_seconds']) > 1e-4:
                bad(name, f'{s["name"]} emitterTime', et, s['emitter_seconds'])
            if truthy(get('stateejectshell', '0')) != s['eject_shell']:
                bad(name, f'{s["name"]} ejectShell', get('stateejectshell'), s['eject_shell'])
            allow = truthy(get('stateallowimagechange', '1'))
            if allow != s['allow_change']:
                bad(name, f'{s["name"]} allowImageChange', get('stateallowimagechange', '1'),
                    s['allow_change'])
    for p in problems:
        print(p)
    print(f'{checked} stock images checked, {len(problems)} mismatches')
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main())
