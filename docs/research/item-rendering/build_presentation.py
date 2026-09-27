"""Offline native presentation assembly; never imported by the game.

Reads previously converted JSON/images plus four explicitly selected recovered
core datablocks. No script execution. Original PNG fallbacks are read-only.
"""
import argparse
import hashlib
import json
import math
import pathlib
import re
import struct
import zipfile
from PIL import Image


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read(path, limit=64 * 1024 * 1024):
    with path.open('rb') as f:
        data = f.read(limit + 1)
    if len(data) > limit:
        raise ValueError(f'Input budget: {path}')
    return data


def original_model(root, path):
    """Read one named original resource without extracting or writing anything."""
    parts = pathlib.PurePosixPath(path).parts
    if len(parts) < 3 or '..' in parts or ':' in path or '\\' in path or path.startswith('/'):
        raise ValueError('Unsafe original model path')
    loose = (root / path).resolve()
    if not loose.is_relative_to(root.resolve()):
        raise ValueError('Original model escapes source root')
    if loose.is_file():
        return read(loose)
    if parts[0].lower() != 'add-ons':
        raise ValueError(f'Missing original model {path}')
    archive = root / parts[0] / (parts[1] + '.zip')
    member = '/'.join(parts[2:]).lower()
    with zipfile.ZipFile(archive) as z:
        matches = [i for i in z.infolist() if i.filename.lower() == member]
        if len(matches) != 1 or matches[0].file_size > 64 * 1024 * 1024:
            raise ValueError(f'Missing/ambiguous/oversized DTS member {path}')
        with z.open(matches[0]) as f:
            data = f.read(64 * 1024 * 1024 + 1)
        if len(data) > 64 * 1024 * 1024:
            raise ValueError('DTS member exceeds byte budget')
        return data


def dts24_bounds(data):
    """DTS24 typed-stream header only; geometry never determines these bounds."""
    if len(data) < 16:
        raise ValueError('Truncated DTS header')
    version, words, first16, first8 = struct.unpack_from('<4I', data)
    if version & 255 != 24 or not 32 <= first16 < first8 < words <= 16_000_000 or 16 + words * 4 > len(data):
        raise ValueError('Unsupported/truncated DTS24 typed-buffer layout')
    # V24: 17 counts, smallest-visible size/index, then guard0 in each stream.
    wide = 16
    short = 16 + first16 * 4
    byte = 16 + first8 * 4
    counts = struct.unpack_from('<17I', data, wide)
    if any(c > 1_000_000 for c in counts):
        raise ValueError('DTS count budget')
    for number, wide_word in [(0, 19), (1, 31)]:
        guards = (struct.unpack_from('<I', data, wide + wide_word * 4)[0],
                  struct.unpack_from('<H', data, short + number * 2)[0], data[byte + number])
        if guards != (number, number, number):
            raise ValueError(f'DTS typed guard mismatch {number}: {guards}')
    # radius, tube radius, center, source min, source max. All float32 bits retained.
    values = struct.unpack_from('<11f', data, wide + 20 * 4)
    if not all(math.isfinite(v) for v in values) or values[0] < 0 or values[1] < 0:
        raise ValueError('Nonfinite/invalid authored DTS bounds header')
    low, high = values[5:8], values[8:11]
    if any(a > b for a, b in zip(low, high)):
        raise ValueError('Inverted authored DTS bounds')
    # Rotation [x,y,z] -> [x,z,-y] reverses the Z interval endpoints.
    native_low = [low[0], low[2], -high[1]]
    native_high = [high[0], high[2], -low[1]]
    evidence = dict(version=24, exporter=version >> 16, float_byte_offset=96,
                    bounds_min_byte_offset=116, bounds_max_byte_offset=128,
                    typed_stream_dword_offsets=[0, first16, first8], count_values=list(counts),
                    radius=values[0], tube_radius=values[1], source_center=list(values[2:5]),
                    source_min=list(low), source_max=list(high), native_min=native_low, native_max=native_high,
                    header_float_bits=[f'{v:08x}' for v in struct.unpack_from('<11I', data, 96)])
    return native_low, native_high, evidence


def bounds_self_test():
    """Independent fixed-offset fixture and all-corner interval check."""
    data = bytearray(16 + 36 * 4)
    struct.pack_into('<4I', data, 0, 24, 36, 32, 34)
    struct.pack_into('<11f', data, 96, 20, 12, 1, 2, 3, -2, -7, -11, 5, 13, 17)
    struct.pack_into('<I', data, 140, 1)
    struct.pack_into('<H', data, 146, 1)
    data[153] = 1
    low, high, _ = dts24_bounds(data)
    points = [(x, z, -y) for x in [-2, 5] for y in [-7, 13] for z in [-11, 17]]
    assert low == [min(p[i] for p in points) for i in range(3)] == [-2, -11, -13]
    assert high == [max(p[i] for p in points) for i in range(3)] == [5, 17, 7]
    for offset, encoded in [(0, struct.pack('<I', 23)), (8, struct.pack('<I', 1)),
                            (92, struct.pack('<I', 7)), (144, struct.pack('<H', 2)),
                            (152, bytes([2])), (140, struct.pack('<I', 7)),
                            (96, struct.pack('<f', math.nan)), (116, struct.pack('<f', 50))]:
        bad = data.copy()
        bad[offset:offset + len(encoded)] = encoded
        try:
            dts24_bounds(bad)
        except ValueError:
            pass
        else:
            raise AssertionError(f'Accepted corrupt DTS header at {offset}')
    try:
        dts24_bounds(data[:-1])
    except ValueError:
        pass
    else:
        raise AssertionError('Accepted truncated typed streams')
    print('DTS bounds self-test: exact offsets, all 8 transformed corners and 9 invalid headers passed')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--repo', type=pathlib.Path, default=pathlib.Path(__file__).resolve().parents[3])
    parser.add_argument('--original', type=pathlib.Path, default=pathlib.Path(r'E:\Downloads\B4v21Launcher\versions\Blockland v20'))
    parser.add_argument('--output', default='content/item-presentation-pack-008')
    parser.add_argument('--weapons', default='content/weapons-pack-007')
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    if args.self_test:
        bounds_self_test()
        return
    repo = args.repo.resolve()
    out = (repo / args.output).resolve()
    if out.exists() or not out.is_relative_to(repo / 'content') or out.is_relative_to(args.original.resolve()):
        raise ValueError('Fresh workspace content output required')
    weapons_root = repo / args.weapons
    weapon_bytes = read(weapons_root / 'weapons.json', 32 * 1024 * 1024)
    pack = json.loads(weapon_bytes)
    ui_root = repo / 'content/ui-pack-003'
    ui = json.loads(read(ui_root / 'ui-pack.json'))
    avatar_root = repo / 'content/avatar-pack-001'
    avatar = json.loads(read(avatar_root / 'avatar.json'))
    script_path = repo / '.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs'
    script_bytes = read(script_path)
    script = script_bytes.decode('utf-8-sig')
    files = {}
    manifest = dict(schema_version=2, id='v20.item-presentation.001', weapons_sha256=digest(weapon_bytes),
                    models={}, textures={}, items={}, images={}, projectiles={}, diagnostics=[])
    bounds_evidence = {}
    image_sources = {}
    for resource in pack['resources']:
        if resource.get('native_file') and pathlib.Path(resource['path']).suffix.lower() in ('.png', '.jpg', '.jpeg'):
            image_sources[resource['path'].lower()] = (weapons_root / resource['native_file'], resource['sha256'])
    for texture in avatar['textures'].values():
        image_sources.setdefault(texture['source'].lower(), (avatar_root / texture['file'], texture['sha256']))
    for texture in ui['images'].values():
        image_sources.setdefault(texture['source'].lower(), (ui_root / texture['file'], texture['sha256']))

    def texture(reference):
        for extension in ['', '.png', '.jpg', '.jpeg']:
            key = (reference + extension).lower()
            source = image_sources.get(key)
            if source is None and key.startswith('base/'):
                candidate = args.original / (reference + extension)
                if candidate.is_file():
                    source = (candidate, digest(read(candidate, 16 * 1024 * 1024)))
            if source is None:
                continue
            path, expected = source
            data = read(path, 16 * 1024 * 1024)
            if digest(data) != expected:
                raise ValueError(f'Image checksum changed: {path}')
            with Image.open(path) as image:
                width, height = image.size
                if width <= 0 or height <= 0 or max(width, height) > 4096:
                    raise ValueError(f'Image dimensions: {path}')
            name = f'textures/{expected}{path.suffix.lower()}'
            files[name] = data
            manifest['textures'][key] = dict(file=name, sha256=expected, width=width, height=height, source=key)
            return key
        raise ValueError(f'Unconverted texture/icon {reference}')

    def model(path, file, source_hash):
        key = path.lower()
        if key in manifest['models']:
            if manifest['models'][key]['source_sha256'] != source_hash:
                raise ValueError(f'Conflicting original model identity {path}')
            return key
        raw = original_model(args.original, path)
        if digest(raw) != source_hash:
            raise ValueError(f'Original model checksum changed: {path}')
        bounds_min, bounds_max, evidence = dts24_bounds(raw)
        evidence['source'] = path
        evidence['source_sha256'] = source_hash
        bounds_evidence[key] = evidence
        data = read(file)
        shape = json.loads(data)
        native_hash = digest(data)
        target = f'models/{native_hash}.json'
        files[target] = data
        bindings = []
        for material in shape['materials']:
            reference = str(pathlib.PurePosixPath(path).parent / material['name'])
            bindings.append(texture(reference))
        manifest['models'][key] = dict(file=target, sha256=native_hash, source=path,
                                      source_sha256=source_hash, textures=bindings,
                                      bounds_min=bounds_min, bounds_max=bounds_max)
        return key

    for resource in pack['resources']:
        if resource.get('native_file', '').startswith('shapes/'):
            model(resource['path'], weapons_root / resource['native_file'], resource['sha256'])
    defs = {d['name'].lower(): d for d in pack['definitions']}

    def fields(name, seen=()):
        if name.lower() in seen or len(seen) > 64:
            raise ValueError('Definition inheritance cycle')
        if name.lower() not in defs:
            match = re.search(r'datablock\s+\w+\(' + re.escape(name) + r'\)\s*\{(.*?)\n\};', script, re.S | re.I)
            if match is None:
                raise ValueError(f'Missing inherited core definition {name}')
            return {key.lower(): value.strip().strip('"') for key, value in re.findall(r'^\s*(\w+)\s*=\s*([^;]+);', match[1], re.M)}
        definition = defs[name.lower()]
        values = fields(definition['parent'], seen + (name.lower(),)) if definition['parent'] else {}
        values.update({k.lower(): v.strip().strip('"') for k, v in definition['fields'].items()})
        return values

    def tint(values):
        enabled = values.get('docolorshift', '0').lower() in ('1', 'true')
        if not enabled:
            return [1., 1., 1., 1.]
        value = values.get('colorshiftcolor', '1 1 1 1')
        if re.fullmatch(r'\w+\.\w+', value):
            owner, field = value.split('.')
            value = fields(owner)[field.lower()]
        def number(token):
            token = token.strip('()')
            if '/' in token:
                numerator, denominator = token.split('/')
                return float(numerator) / float(denominator)
            return float(token)
        result = [number(v) for v in value.replace('SPC', ' ').split()]
        # The glow can authors an overbright shift ("3 3 3 2.0"); colour
        # shifting saturates at white, so clamp such literals to [0, 1].
        if len(result) == 4 and all(0 <= v <= 4 for v in result):
            result = [min(v, 1.0) for v in result]
        if len(result) != 4 or not all(0 <= v <= 1 for v in result):
            raise ValueError(f'Unsupported tint literal {value}')
        return result

    for item in pack['items'].values():
        values = fields(item['name'])
        icon = texture(item['icon']) if item['icon'] else None
        if icon is None:
            manifest['diagnostics'].append(f"{item['id']}: source ItemData has no icon; host may show a native model thumbnail")
        manifest['items'][item['id']] = dict(model=item['model'].lower(), image=item['image'], tint=tint(values), icon=icon,
                                            evidence=defs[item['name'].lower()]['source'])
    for image in pack['images'].values():
        values = fields(image['name'])
        manifest['images'][image['id']] = dict(model=image['model'].lower(), mount_point=image['mount_point'],
            offset=image['offset'], eye_offset=image['eye_offset'], source_rotation_degrees=image['source_rotation_degrees'],
            eye_rotation_degrees=[float(v) for v in re.findall(r'-?\d+(?:\.\d+)?', values.get('eyerotation', '0 0 0'))],
            tint=tint(values),
            evidence=defs[image['name'].lower()]['source'])
    for projectile in pack['projectiles'].values():
        values = fields(projectile['name'])
        manifest['projectiles'][projectile['id']] = dict(model=projectile['model'].lower() or None, tint=tint(values))

    if sum(v['width'] * v['height'] * 4 for v in manifest['textures'].values()) > 256 * 1024 * 1024:
        raise ValueError('Aggregate texture budget')
    if sum(map(len, files.values())) > 256 * 1024 * 1024:
        raise ValueError('Aggregate file budget')
    physics = dict(schema_version=1, items={item_id: dict(min=manifest['models'][item['model']]['bounds_min'],
        max=manifest['models'][item['model']]['bounds_max']) for item_id, item in manifest['items'].items()})
    files['item-physics.json'] = json.dumps(physics, indent=2, sort_keys=True).encode() + b'\n'
    manifest['item_physics_sha256'] = digest(files['item-physics.json'])
    files['bounds-evidence.json'] = json.dumps(dict(schema_version=1, models=bounds_evidence), indent=2, sort_keys=True).encode() + b'\n'
    files['presentation.json'] = json.dumps(manifest, indent=2, sort_keys=True).encode() + b'\n'
    out.mkdir()
    for name, data in files.items():
        target = out / name
        target.parent.mkdir(exist_ok=True)
        target.write_bytes(data)
    print(json.dumps(dict(output=str(out), manifest_sha256=digest(files['presentation.json']), item_physics_sha256=manifest['item_physics_sha256'],
                         models=len(manifest['models']), textures=len(manifest['textures']), items=len(manifest['items']), images=len(manifest['images']), projectiles=len(manifest['projectiles']), bytes=sum(map(len, files.values()))), indent=2))


if __name__ == '__main__':
    main()
