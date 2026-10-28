#!/usr/bin/env python3
"""Build a private, optional NPC reaction Add-On from an explicit source catalog.

Recordings are never part of the repository. Install processing dependencies with
`python -m pip install numpy scipy soundfile`; then use --download --output DIR.
No downloads happen without --download. Existing cached downloads are hash checked.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import html
import json
import math
import os
from pathlib import Path
import re
import shutil
import time
from urllib.parse import urlparse
from urllib.request import Request, urlopen

HERE = Path(__file__).resolve().parent / 'npc_reactions'
RATE = 22050
TARGET_RMS = 0.16
PEAK_LIMIT = 0.88


def sha(data):
    return hashlib.sha256(data).hexdigest()


def prepare(raw, start=0.0, end=None):
    import io
    import numpy as np
    import soundfile as sf
    from scipy.signal import resample_poly
    pcm, rate = sf.read(io.BytesIO(raw), dtype='float32', always_2d=True)
    if not len(pcm) or not np.isfinite(pcm).all() or rate < 8000:
        raise ValueError('empty, non-finite or unsupported recording')
    source_seconds = len(pcm) / rate
    stop = source_seconds if end is None else end
    if not 0 <= start < stop <= source_seconds + 0.001:
        raise ValueError(f'crop {start}..{stop} outside {source_seconds:.3f}s')
    pcm = pcm[int(start * rate):int(stop * rate)].mean(axis=1)
    pcm -= pcm.mean()  # remove DC before finding the audible region
    audible = np.flatnonzero(np.abs(pcm) > max(0.001, float(np.max(np.abs(pcm))) * 0.008))
    if not len(audible):
        raise ValueError('silent recording')
    margin = int(rate * 0.035)
    pcm = pcm[max(0, audible[0] - margin):min(len(pcm), audible[-1] + margin + 1)]
    divisor = math.gcd(rate, RATE)
    pcm = resample_poly(pcm, RATE // divisor, rate // divisor)
    rms = float(np.sqrt(np.mean(pcm * pcm)))
    # Nearby speech should survive ordinary combat, rather than being attenuated
    # once here and again by both its profile and its positional reaction cue.
    # Retain each recording's dynamics and reserve peak headroom.
    pcm *= min(TARGET_RMS / max(rms, 1e-6), PEAK_LIMIT / max(float(np.max(np.abs(pcm))), 1e-6), 8.0)
    fade = min(int(RATE * 0.006), len(pcm) // 2)
    pcm[:fade] *= np.linspace(0, 1, fade)
    pcm[-fade:] *= np.linspace(1, 0, fade)
    output = io.BytesIO()
    sf.write(output, pcm, RATE, format='WAV', subtype='PCM_16')
    return output.getvalue(), {'source_seconds': source_seconds, 'seconds': len(pcm) / RATE,
                              'peak': float(np.max(np.abs(pcm))), 'rms': float(np.sqrt(np.mean(pcm * pcm)))}


def obtain(row, cache, download):
    path = cache / (row['id'] + '.mp3')
    if not path.exists():
        if not download:
            raise ValueError(f"{row['id']}: not cached; use --download")
        url = urlparse(row['url'])
        allowed = {'memebtn.com': '/sounds/', 'www.myinstants.com': '/media/sounds/',
                   'storage.instantsbutton.com': '/'}
        if url.scheme != 'https' or url.netloc not in allowed or not url.path.startswith(allowed[url.netloc]):
            raise ValueError('source URL must be an explicitly cataloged public recording')
        for attempt in range(3):
            try:
                with urlopen(Request(row['url'], headers={'User-Agent': 'Mozilla/5.0'}), timeout=30) as response:
                    data = response.read(8 * 1024 * 1024 + 1)
                if len(data) > 8 * 1024 * 1024:
                    raise ValueError('recording exceeds 8 MiB')
                path.write_bytes(data)
                break
            except OSError:
                if attempt == 2:
                    raise
                time.sleep(attempt + 1)
    raw = path.read_bytes()
    if row.get('sha256') and sha(raw) != row['sha256']:
        raise ValueError(f"{row['id']}: source changed; cached bytes do not match catalog")
    return row, raw


def build(catalog, cache, output, download=False):
    rows = catalog['clips']
    if catalog['schema_version'] != 1 or len({r['id'] for r in rows}) != len(rows):
        raise ValueError('invalid catalog schema or duplicate recording')
    for row in rows:
        if not re.fullmatch('[a-z0-9-]+', row['id']):
            raise ValueError('unsafe recording ID')
    cache.mkdir(parents=True, exist_ok=True)
    if output.exists() and any(output.iterdir()) and not (output / 'npc-reactions-generated.json').exists():
        raise ValueError('refusing to overwrite a directory not owned by this generator')
    rules = output.with_name('npc_popculture_rules')
    if rules == output:
        raise ValueError('--output must name the recording package, not its rules companion')
    if rules.exists() and any(rules.iterdir()) and not (rules / 'npc-reactions-generated.json').exists():
        raise ValueError('refusing to overwrite an unowned rules directory')
    prepared = []
    with ThreadPoolExecutor(max_workers=4) as pool:
        for row, raw in pool.map(lambda r: obtain(r, cache, download), rows):
            wav, stats = prepare(raw, row.get('start', 0), row.get('end'))
            prepared.append((row, raw, wav, stats))
    too_long = [(r['id'], round(s['seconds'], 2)) for r, _, _, s in prepared if s['seconds'] > r.get('max_seconds', 5.0)]
    if too_long:
        raise ValueError(f'curate explicit excerpts before generating: {too_long}')
    (output / 'assets' / 'clips').mkdir(parents=True, exist_ok=True)
    sounds = {}
    provenance = []
    choices = []
    for row, raw, wav, stats in prepared:
        slug = row['id']
        (output / 'assets' / 'clips' / (slug + '.wav')).write_bytes(wav)
        profile = 'npc_popculture:sound/' + slug
        sounds[profile] = {'file': 'clips/' + slug + '.wav', 'seconds': stats['seconds'], 'volume': 1.0}
        provenance.append(dict(row, sha256=sha(raw), processed_sha256=sha(wav), **stats))
        choices.append(dict(id=slug, profile=profile, events=row['events'], tier=row['tier'],
                            iconic=row['iconic'], seconds=stats['seconds']))
    (output / 'assets' / 'weapons.json').write_text(json.dumps({'schema_version': 5, 'id': 'npc_popculture', 'sounds': sounds}, indent=2))
    shutil.copyfile(HERE / 'package.json', output / 'package.json')
    # Earlier local prototypes combined the server scripts with shared audio.
    # Only these known generator-owned leftovers are removed, never recordings.
    for file in ['behaviour.json', 'reactions.rhai', 'catalog.rhai']:
        (output / file).unlink(missing_ok=True)
    rules.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(HERE / 'rules-package.json', rules / 'package.json')
    for file in ['behaviour.json', 'reactions.rhai']:
        shutil.copyfile(HERE / file, rules / file)
    # Constant function, rather than persistent state or per-tick text parsing.
    def rhai(v):
        if isinstance(v, dict):
            return '#{' + ','.join(k + ':' + rhai(x) for k, x in v.items()) + '}'
        if isinstance(v, list):
            return '[' + ','.join(rhai(x) for x in v) + ']'
        return json.dumps(v)
    # Keep each returned value below the runtime's aggregate text limit.
    events = sorted({e for c in choices for e in c['events']})
    script = 'fn clips(event) {\n'
    for event in events:
        script += 'if event == ' + json.dumps(event) + ' { return ' + rhai([c for c in choices if event in c['events']]) + '; }\n'
    script += '[]\n}\n'
    (rules / 'catalog.rhai').write_text(script, encoding='utf-8')
    (rules / 'npc-reactions-generated.json').write_text(json.dumps({'schema_version': 1, 'recordings': output.name}))
    (output / 'npc-reactions-generated.json').write_text(json.dumps({'schema_version': 1, 'clips': provenance}, indent=2))
    buttons = ''.join(f'<p>{html.escape(r["title"])} — {html.escape(", ".join(r["events"]))}<br><audio controls preload="none" src="assets/clips/{r["id"]}.wav"></audio></p>' for r in rows)
    (output / 'audition.html').write_text('<!doctype html><meta charset="utf-8"><title>NPC reaction audition</title><h1>Local reaction recordings</h1><p>Unverified source rights; private audition pack.</p>' + buttons, encoding='utf-8')
    return provenance


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--download', action='store_true')
    parser.add_argument('--catalog', type=Path, default=HERE / 'catalog.json')
    parser.add_argument('--cache', type=Path, default=Path(os.environ.get('LOCALAPPDATA', str(Path.home()))) / 'BlocklandReImagined' / 'audio-cache' / 'npc-reactions-v1')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--inspect', action='store_true', help='download/decode and list durations without building')
    args = parser.parse_args()
    catalog = json.loads(args.catalog.read_text(encoding='utf-8'))
    if args.inspect:
        args.cache.mkdir(parents=True, exist_ok=True)
        for row in catalog['clips']:
            _, raw = obtain(row, args.cache, args.download)
            _, stats = prepare(raw, row.get('start', 0), row.get('end'))
            print(row['id'], json.dumps(dict(sha256=sha(raw), **stats)))
        return
    result = build(catalog, args.cache, args.output, args.download)
    print(f'Built {len(result)} unique recordings in {args.output}')


if __name__ == '__main__':
    main()
