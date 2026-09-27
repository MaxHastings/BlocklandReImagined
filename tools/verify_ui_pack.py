"""Independent byte/geometry/provenance audit; does not launch the game.

python tools/verify_ui_pack.py INSTALL PACK --compare OLD --report REPORT
Run from the workspace root so relative recovered-script provenance resolves.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import struct
import zipfile
import zlib

from PIL import Image


def digest(data):
    return hashlib.sha256(data).hexdigest()


def bounded_path(root, name):
    result = (root / name).resolve()
    assert result.is_relative_to(root.resolve()), f"path outside pack: {name}"
    return result


def source_bytes(root, source):
    if '.zip:' in source.lower():
        split = source.lower().index('.zip:') + 4
        archive, member = source[:split], source[split + 1:]
        if '/' not in archive:
            archive = 'Add-Ons/' + archive
        with zipfile.ZipFile(bounded_path(root, archive)) as z:
            return z.read(member)
    return bounded_path(root, source).read_bytes()


def dimensions(data):
    with Image.open(io.BytesIO(data)) as image:
        image.load()
        return list(image.size)


def parse_gft(data):
    version, height, baseline, count = struct.unpack_from('<4I', data)
    assert version == 1 and count <= 65536
    records = [struct.unpack_from('<H4B3b', data, 16 + i * 9) for i in range(count)]
    offset = 16 + count * 9
    sheet_count, = struct.unpack_from('<I', data, offset)
    offset += 4
    sheets = []
    for _ in range(sheet_count):
        start = offset
        assert data[offset:offset + 8] == b'\x89PNG\r\n\x1a\n'
        offset += 8
        while True:
            length, = struct.unpack_from('>I', data, offset)
            kind = data[offset + 4:offset + 8]
            payload_end = offset + 8 + length
            crc, = struct.unpack_from('>I', data, payload_end)
            assert zlib.crc32(data[offset + 4:payload_end]) == crc
            offset = payload_end + 4
            if kind == b'IEND':
                assert length == 0
                break
        sheets.append(data[start:offset])
    assert len(data) - offset == 512
    remap = struct.unpack_from('<256H', data, offset)
    keys = ['sheet', 'x', 'y', 'w', 'h', 'x_origin', 'y_origin', 'advance']
    glyphs = [None if index == 65535 else dict(zip(keys, records[index])) for index in remap]
    return height, baseline, sheets, glyphs


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('install', type=Path)
    parser.add_argument('pack', type=Path)
    parser.add_argument('--compare', type=Path)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    pack = json.loads((args.pack / 'ui-pack.json').read_text())
    manifest = json.loads((args.pack / 'conversion-manifest.json').read_text())
    assert pack['schema_version'] == manifest['schema_version'] == 1
    for record in manifest['inputs']:
        raw = source_bytes(args.install, record['source'])
        assert len(raw) == record['bytes'] and digest(raw) == record['sha256'], record
    for record in pack['sources']:
        raw = Path(record['path']).read_bytes()
        assert len(raw) == record['bytes'] and digest(raw) == record['sha256'], record
    outputs = {}
    for record in manifest['outputs']:
        raw = bounded_path(args.pack, record['path']).read_bytes()
        assert len(raw) == record['bytes'] and digest(raw) == record['sha256'], record
        outputs[record['path']] = record
    actual = {p.relative_to(args.pack).as_posix() for p in args.pack.rglob('*') if p.is_file()}
    assert actual == set(outputs) | {'conversion-manifest.json'}, actual ^ set(outputs)
    image_sizes = {}
    for name, entry in pack['images'].items():
        raw = bounded_path(args.pack, entry['file']).read_bytes()
        assert digest(raw) == entry['sha256'] and raw == source_bytes(args.install, entry['source']), name
        size = dimensions(raw)
        assert size == [entry['width'], entry['height']], name
        image_sizes[name] = size
    font_reports = {}
    for name, font in pack['fonts'].items():
        raw = source_bytes(args.install, font['source'])
        assert digest(raw) == font['sha256'], name
        height, baseline, sheets, glyphs = parse_gft(raw)
        assert [height, baseline] == [font['line_height'], font['baseline']], name
        assert glyphs == font['glyphs'], name
        assert len(sheets) == len(font['sheets']), name
        sizes = []
        for sheet, filename in zip(sheets, font['sheets']):
            assert sheet == bounded_path(args.pack, filename).read_bytes(), filename
            sizes.append(dimensions(sheet))
        for glyph in filter(None, glyphs):
            width, height = sizes[glyph['sheet']]
            assert glyph['x'] + glyph['w'] <= width and glyph['y'] + glyph['h'] <= height, name
        font_reports[name] = {'mapped_glyphs': sum(g is not None for g in glyphs), 'sheet_sizes': sizes}
    for name, skin in pack['skins'].items():
        width, height = image_sizes[name]
        for x, y, w, h in skin['pieces']:
            assert w > 0 and h > 0 and x + w <= width and y + h <= height, name
    comparison = {}
    if args.compare:
        old = json.loads((args.compare / 'ui-pack.json').read_text())
        for key in ['images', 'fonts', 'skins', 'styles', 'layouts']:
            before, after = old[key], pack[key]
            comparison[key] = {'added': sorted(after.keys() - before.keys()),
                               'removed': sorted(before.keys() - after.keys()),
                               'changed': sorted(k for k in before.keys() & after.keys() if before[k] != after[k])}
        before = {m['mission']: m for m in old['maps']}
        after = {m['mission']: m for m in pack['maps']}
        comparison['maps'] = {'added': sorted(after.keys() - before.keys()),
                              'removed': sorted(before.keys() - after.keys()),
                              'changed': sorted(k for k in before.keys() & after.keys() if before[k] != after[k])}
        comparison['data_equal'] = old['data'] == pack['data']
        comparison['script_hashes_equal'] = [s['sha256'] for s in old['sources']] == [s['sha256'] for s in pack['sources']]
    report = {'schema_version': 1, 'passed': True,
              'scope': 'Byte preservation and structural checks only; installed inventory includes community content, not vanilla certification or visual/behavior fidelity.',
              'pack': args.pack.as_posix(),
              'counts': {k: len(pack[k]) for k in ['images', 'fonts', 'skins', 'styles', 'layouts', 'maps', 'warnings']},
              'verified_asset_inputs': len(manifest['inputs']), 'verified_script_inputs': len(pack['sources']),
              'verified_outputs': len(outputs), 'font_sheets': sum(len(f['sheets']) for f in pack['fonts'].values()),
              'manifest_sha256': digest((args.pack / 'conversion-manifest.json').read_bytes()),
              'outputs': list(outputs.values()), 'image_sizes': image_sizes,
              'fonts': font_reports, 'map_inventory': pack['maps'],
              'addon_image_packages': sorted({name.split('/')[1] for name in pack['images'] if name.startswith('add-ons/')}),
              'warnings': pack['warnings'], 'comparison': comparison}
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: report[k] for k in ['passed', 'counts', 'verified_asset_inputs', 'verified_outputs', 'font_sheets', 'comparison']}, indent=2))


if __name__ == '__main__':
    main()
