"""Independent, read-only source/native weather verification (Pillow + stdlib)."""
import argparse, hashlib, io, json, zipfile
from pathlib import Path
from PIL import Image


def digest(data):
    return hashlib.sha256(data).hexdigest()


def safe(root, name):
    p = (root / name).resolve()
    assert p.is_relative_to(root.resolve()) and p.is_file(), name
    assert p.stat().st_size <= 64 * 1024 * 1024, name
    return p.read_bytes()


def original(root, name):
    parts = name.split('/')
    assert all(p not in ('', '.', '..') for p in parts)
    if parts[0].lower() == 'add-ons':
        archives = [p for p in (root / 'Add-Ons').iterdir() if p.name.lower() == parts[1].lower() + '.zip']
        assert len(archives) == 1
        with zipfile.ZipFile(archives[0]) as z:
            members = [n for n in z.namelist() if n.lower() == '/'.join(parts[2:]).lower()]
            assert len(members) == 1 and z.getinfo(members[0]).file_size <= 64 * 1024 * 1024
            return z.read(members[0])
    return safe(root, name)


def main():
    p = argparse.ArgumentParser()
    p.add_argument('pack', type=Path)
    p.add_argument('original', type=Path)
    p.add_argument('core', type=Path)
    p.add_argument('maps', type=Path)
    p.add_argument('report', type=Path)
    a = p.parse_args()
    m = json.loads(safe(a.pack, 'weather.json'))
    assert m['schema_version'] == 1 and abs(m['legacy_tick_seconds'] - .032) < 1e-7
    assert len(m['definitions']) == 2 and len(m['placements']) == 2
    assert sorted(x['drops'] for x in m['placements']) == [500, 5000]
    sources = {}
    for s in m['sources']:
        data = safe(a.pack, s['copy_file'])
        assert digest(data) == s['sha256'] and len(data) == s['bytes']
        if s['kind'] == 'primary_original':
            reference = original(a.original, s['path'])
        elif s['kind'] == 'recovered_source':
            reference = a.core.read_bytes()
        elif s['kind'] == 'native_input':
            reference = safe(a.maps, s['path'].split('/')[-1])
        elif s['kind'] == 'manual_adaptation':
            reference = Path(__file__).with_name('atlas-adaptations.json').read_bytes()
        else:
            raise AssertionError(s['kind'])
        assert data == reference, s['path']
        sources[s['path']] = data
    textures = []
    for ident, t in m['textures'].items():
        data = safe(a.pack, t['file'])
        assert digest(data) == t['sha256']
        im = Image.open(io.BytesIO(data)).convert('RGBA')
        assert im.size == (t['width'], t['height']) and digest(im.tobytes()) == t['rgba_sha256']
        src = Image.open(io.BytesIO(sources[t['source_paths'][0]])).convert('RGBA')
        if len(t['source_paths']) == 2:
            alpha = Image.open(io.BytesIO(sources[t['source_paths'][1]]))
            assert alpha.mode == 'L' and alpha.size == src.size
            src.putalpha(alpha)
        # JPEG decoders may differ by rounding. Report this rather than call pixels byte-identical.
        delta = [abs(x-y) for x,y in zip(src.tobytes(), im.tobytes())]
        alpha_delta = max(delta[3::4])
        assert max(delta) <= 4 and alpha_delta <= 1, (ident, max(delta), alpha_delta)
        if ident.endswith('/snow'):
            assert max(delta) == 0
            alpha = im.getchannel('A')
            seam = max(alpha.getpixel((x,y)) for x in [127,128] for y in range(256))
            assert seam == 0
        textures.append(dict(id=ident, size=im.size, original_decoded_max_difference=max(delta), original_alpha_max_difference=alpha_delta, alpha_range=im.getchannel('A').getextrema(), native_png_sha256=digest(data)))
    assert len(sources) == 13 and len(textures) == 3
    report = dict(passed=True, definitions=len(m['definitions']), placements=len(m['placements']), original_drop_count=5500, source_records=len(sources), primary_original_records=sum(s['kind']=='primary_original' for s in m['sources']), textures=textures, note='Every source copy is byte-identical to the independent input. PNG RGBA hashes and bounded JPEG rounding differences checked separately.', assumptions=m['assumptions'])
    a.report.parent.mkdir(parents=True, exist_ok=True)
    a.report.write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(report, indent=2))

if __name__ == '__main__':
    main()
