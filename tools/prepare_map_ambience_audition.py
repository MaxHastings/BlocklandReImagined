#!/usr/bin/env python3
"""Prepare an offline visual/listening comparison; this does not install game content.

Inputs are pinned public MP3 previews, never described as lossless recordings.
Requires numpy, scipy, soundfile. Original map imagery is read from --content.
"""
import argparse
import hashlib
import html
import io
import json
from pathlib import Path
import shutil
from urllib.request import urlopen
from urllib.error import URLError

HERE = Path(__file__).resolve().parent / 'map_ambience'
LEVELS = {'room': -40, 'fridge': -35, 'dry-air': -36, 'cold-air': -34,
          'shore': -33, 'leaves': -35, 'night': -38, 'night-alt': -38,
          'rain': -32, 'palms': -36}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def prepare(raw, target_db):
    import numpy as np
    import soundfile as sf
    from scipy.signal import butter, resample_poly, sosfilt
    import math

    pcm, rate = sf.read(io.BytesIO(raw), dtype='float64', always_2d=True)
    if rate < 8000 or not len(pcm) or not np.isfinite(pcm).all():
        raise ValueError('invalid audio input')
    source_seconds = len(pcm) / rate
    # Preserve stereo and natural dynamics. No compression, artificial events,
    # pitch changes, or noise reduction. Remove sub-bass microphone rumble.
    pcm = sosfilt(butter(2, 35, 'highpass', fs=rate, output='sos'), pcm, axis=0)
    divisor = math.gcd(rate, 48000)
    pcm = resample_poly(pcm, 48000 // divisor, rate // divisor, axis=0)
    pcm = pcm[:180 * 48000]
    overlap = min(2 * 48000, len(pcm) // 8)
    mix = np.linspace(0, 1, overlap, endpoint=False)[:, None]
    # Linear overlap avoids the +3dB swell of equal-power fades on correlated
    # beds. Keep every join internal, with ordinary adjacent samples at wrap.
    pcm = np.concatenate((pcm[overlap:-overlap],
                          pcm[-overlap:] * (1 - mix) + pcm[:overlap] * mix))
    rms = float(np.sqrt(np.mean(pcm * pcm)))
    if rms < 1e-8:
        raise ValueError('silent input')
    gain = min(10 ** (target_db / 20) / rms, 0.35 / float(np.max(np.abs(pcm))))
    pcm *= gain
    output = io.BytesIO()
    sf.write(output, pcm, 48000, format='WAV', subtype='PCM_16')
    # A short excerpt straddles the actual loop boundary twice. It is a join
    # audition, not the shipped loop length, and does not start automatically.
    excerpt = np.concatenate((pcm[-5 * 48000:], pcm[:10 * 48000],
                              pcm[-5 * 48000:], pcm[:5 * 48000]))
    preview = io.BytesIO()
    sf.write(preview, excerpt, 48000, format='WAV', subtype='PCM_16')
    return output.getvalue(), preview.getvalue(), {
        'source_seconds': source_seconds, 'loop_seconds': len(pcm) / 48000,
        'rate': 48000, 'channels': pcm.shape[1], 'gain_db': 20 * math.log10(gain),
        'rms_dbfs': 20 * math.log10(float(np.sqrt(np.mean(pcm * pcm)))),
        'peak_dbfs': 20 * math.log10(float(np.max(np.abs(pcm)))),
        'wrap_delta': float(np.max(np.abs(pcm[-1] - pcm[0]))),
        'overlap_seconds': overlap / 48000,
        'encoding': 'PCM16 WAV derived from lossy MP3 preview',
    }


def build(cache, output, content, download=False, selected_keys=None):
    catalog = json.loads((HERE / 'candidates.json').read_text(encoding='utf-8'))
    maps = json.loads((HERE / 'maps.json').read_text(encoding='utf-8'))['maps']
    output.mkdir(parents=True, exist_ok=True)
    cache.mkdir(parents=True, exist_ok=True)
    for sub in ['loops', 'joins', 'references']:
        (output / sub).mkdir(exist_ok=True)
    receipt = []
    for row in catalog['clips']:
        if selected_keys is not None and row['key'] not in selected_keys:
            continue
        path = cache / row['input']
        if not path.exists():
            if not download:
                raise ValueError(f'{path} missing; use --download')
            url = row['preview_url']
            if not url.startswith('https://cdn.freesound.org/previews/'):
                raise ValueError('unexpected preview origin')
            try:
                with urlopen(url, timeout=45) as response:
                    raw = response.read(32 * 1024 * 1024 + 1)
            except URLError as error:
                raise ValueError(f"{row['key']}: approved source download unavailable ({error}); provide pinned cache file {path}, SHA256 {row['input_sha256']}. No alternate source was adopted.") from error
            if len(raw) > 32 * 1024 * 1024:
                raise ValueError('preview exceeds 32MiB')
            if sha(raw) != row['input_sha256']:
                raise ValueError(f"{row['key']}: downloaded preview changed")
            path.write_bytes(raw)
        raw = path.read_bytes()
        if sha(raw) != row['input_sha256']:
            raise ValueError(f"{row['key']}: cached preview changed")
        wav, join, stats = prepare(raw, LEVELS[row['key']])
        (output / 'loops' / (row['key'] + '.wav')).write_bytes(wav)
        (output / 'joins' / (row['key'] + '.wav')).write_bytes(join)
        receipt.append(dict(row, processing=stats, loop_sha256=sha(wav), join_sha256=sha(join)))
    by_key = {row['key']: row for row in receipt}
    cards = []
    for index, row in enumerate(maps):
        source = content / row['reference']
        if sha(source.read_bytes()) != row['reference_sha256']:
            raise ValueError(f"{row['name']}: reference image differs from inspected input")
        label = 'Packaged map preview'
        if row.get('supplemental_reference'):
            supplemental = HERE.parent.parent / row['supplemental_reference']
            if supplemental.is_file():
                if sha(supplemental.read_bytes()) != row['supplemental_reference_sha256']:
                    raise ValueError(f"{row['name']}: supplemental reference changed")
                source = supplemental
                label = row['supplemental_reference_label']
            else:
                label += '; supplemental overview unavailable in this checkout'
        image = f'references/{index:02}{source.suffix}'
        shutil.copyfile(source, output / image)
        layers = []
        for key in row['candidates']:
            if key not in by_key:
                continue
            recording = by_key[key]
            layers.append(f'<p>{html.escape(recording["title"])} — {html.escape(recording["author"])}'
                          f' · CC0 · <a href="{html.escape(recording["source"])}">source/original download</a><br>'
                          f'<audio controls preload="none" loop src="loops/{key}.wav"></audio></p>')
        cards.append(f'<article><img src="{image}" alt="{html.escape(row["name"])} visual reference">'
                     f'<small>{html.escape(label)}</small>'
                     f'<h2>{html.escape(row["name"])}</h2><p>{html.escape(row["visual_assessment"])}</p>'
                     + ''.join(layers) + ('<p>Original authored sound or deliberate quiet; see assessment.</p>' if not layers else '')
                     + '</article>')
    alternatives = ''.join(f'<p>{html.escape(row["title"])} — {html.escape(row["author"])} '
                           f'<a href="{html.escape(row["source"])}">source</a><br>'
                           f'<audio controls preload="none" src="joins/{row["key"]}.wav"></audio>'
                           f' {row["processing"]["loop_seconds"]:.1f}s full loop; '
                           f'{row["processing"]["rms_dbfs"]:.1f} dBFS RMS</p>' for row in receipt)
    details = ''
    detail_receipt = output / 'details/details-provenance.json'
    if detail_receipt.is_file():
        details = '<h2>New: individual details and a spaced outdoor example</h2><p>These six previews use original PCM WAV recordings by Joseph Sardin, CC0. They demonstrate candidate layers and spacing; in-game scheduling and enclosure acceptance remain pending.</p>'
        for row in json.loads(detail_receipt.read_text(encoding='utf-8')):
            credits = ', '.join(f'<a href="{html.escape(v["source"])}">{html.escape(v["source_key"])}</a>' for v in row['inputs'])
            details += (f'<p><strong>{html.escape(row["file"].replace(".wav", "").replace("-", " "))}</strong>'
                        f' · {row["seconds"]:.1f}s<br><audio controls preload="none" src="details/{row["file"]}"></audio><br>'
                        f'{html.escape(row["treatment"])} Sources: {credits}</p>')
    originals = ''
    approvals = json.loads((HERE / 'source-approvals.json').read_text(encoding='utf-8'))
    for row in approvals['sources']:
        if not row['id'].startswith('original-map:'):
            continue
        preview = output / row['preview']
        if not preview.is_file():
            continue
        if sha(preview.read_bytes()) != row['preview_sha256']:
            raise ValueError(f"{row['id']}: original-map audition changed")
        title = row['id'].split(':', 1)[1].replace('-', ' ')
        originals += (f'<p><strong>{html.escape(title)}</strong> — {html.escape(row["author"])}'
                      f' · {row["seconds"]:.1f}s · approval {html.escape(row["state"])}<br>'
                      f'<audio controls preload="none" loop src="{html.escape(row["preview"])}"></audio><br>'
                      f'{html.escape(row["quality"])}. {html.escape(row["license"])}.</p>')
    if originals:
        originals = ('<h2>Original map recordings proposed for restoration</h2>'
                     '<p>Creek has localized frogs, waterfall and shore emitters; Forest Dark has its authored cricket bed. '
                     'These recordings were present in their original map archives but missing from the converted game. '
                     'Listen before approving restoration. Preview gains are for audition only; these controls do not demonstrate in-game distances.</p>'
                     + originals)
    document = ('<!doctype html><meta charset="utf-8"><title>Map ambience audition</title>'
                '<style>body{background:#171c22;color:#e8edf2;font:17px system-ui;max-width:1200px;margin:40px auto;padding:20px}'
                'a{color:#97d5ff}main{display:grid;grid-template-columns:repeat(auto-fit,minmax(340px,1fr));gap:24px}'
                'article{background:#242c35;padding:18px;border-radius:10px}img{width:100%;height:240px;object-fit:contain}'
                'audio{width:100%;margin-top:8px}p{line-height:1.5}</style>'
                '<h1>Blockland ReImagined · map ambience audition</h1>'
                '<p>21 shipped map variants. These are candidate recordings, not accepted game mixes. '
                'The continuous-bed candidates below come from public Freesound HQ MP3 previews, decoded to WAV for editing; '
                'those beds are not lossless originals. The separate new detail previews use original PCM WAVs. '
                'CC0 source pages and exact input/output hashes are in the accompanying provenance files. '
                'Original-map recordings retain their map-package provenance and are not labelled CC0. '
                'Every proposed source remains pending your individual approval.</p>'
                '<p>Start at your ordinary game volume. Beds are intentionally subtle. Play one card at a time; '
                'several candidates on a card show proposed layers, without pretending their spatial treatment is implemented. '
                'No audio plays automatically. This report does not change your installed game.</p>'
                + originals + details + '<h2>Map bed candidates</h2><main>' + ''.join(cards) + '</main><h2>Loop joins and alternatives</h2>'
                '<p>These 25-second excerpts deliberately cross each full loop boundary twice. '
                'Reject audible pulses, clicks, voices, traffic, repeated calls or masking noise. '
                'Palms is an alternative with reported preview artifacts; night alternatives must not overlap Forest Dark’s original crickets.</p>'
                + alternatives)
    (output / 'audition.html').write_text(document, encoding='utf-8')
    (output / 'provenance.json').write_text(json.dumps({'schema_version': 1, 'clips': receipt, 'maps': maps}, indent=2), encoding='utf-8')
    return receipt


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cache', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--content', type=Path, required=True)
    parser.add_argument('--download', action='store_true')
    args = parser.parse_args()
    for item in build(args.cache, args.output, args.content, args.download):
        print(item['key'], json.dumps(item['processing']))
