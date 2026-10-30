#!/usr/bin/env python3
"""Prepare the three heard/selected tool swings and augment native weapons data.

The read-only v20 conversion runs first. This explicit native policy adds sound
to two Fire states; contact policy, timing, animations and runtime stay intact.
Original/downloaded/generated recordings never enter Git.
"""
import argparse
import array
import hashlib
import io
import json
import math
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import wave
import zipfile

REPO = Path(__file__).resolve().parents[1]
INPUTS = ['tools/prepare_tool_swishes.py', 'tools/tool_swishes/sources.json',
          'tools/tool_swishes/policy.json']
RATE = 48000
RESOURCE_PREFIX = 'bri/foley/tools/'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def recipe_hash():
    hasher = hashlib.sha256()
    for name in INPUTS:
        hasher.update(name.encode() + b'\0')
        hasher.update((REPO / name).read_bytes().replace(b'\r\n', b'\n'))
    return hasher.hexdigest()


def read_json(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


def write_json(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8', newline='\n')


def selected_sources():
    sources = read_json(REPO / INPUTS[1])['sources']
    if len(sources) != 3 or any(s['selection'] != 'accepted-source' for s in sources):
        raise ValueError('Only the three explicitly approved tool swishes may be prepared')
    return sources


def source_bytes(sources, inputs, download):
    """Verify the pinned archive AND each named member, before any outputs."""
    archive = inputs / 'swishes.zip'
    if not archive.exists() and download:
        data = urllib.request.urlopen(sources[0]['url'], timeout=30).read(16 * 1024 * 1024)
        if digest(data) != sources[0]['archive_sha256']:
            raise ValueError('Downloaded swishes archive differs from the approved source')
        inputs.mkdir(parents=True, exist_ok=True)
        archive.write_bytes(data)
    data = archive.read_bytes()
    if any(digest(data) != s['archive_sha256'] for s in sources):
        raise ValueError('Swishes archive hash mismatch')
    with zipfile.ZipFile(io.BytesIO(data)) as zipped:
        out = {}
        for s in sources:
            member = zipped.getinfo(s['member'])
            if member.file_size > 1024 * 1024:
                raise ValueError('Oversized swish source')
            clip = zipped.read(member)
            if digest(clip) != s['sha256']:
                raise ValueError(f"Approved member hash mismatch: {s['id']}")
            out[s['id']] = clip
    return out


def process(source, raw_wav, ffmpeg):
    # Identical command and processing to the A/B/C auditions Maxwell heard.
    result = subprocess.run([ffmpeg, '-v', 'error', '-i', 'pipe:0', '-af', source['filter'],
                             '-ac', '1', '-ar', str(RATE), '-f', 'f32le', '-'],
                            input=raw_wav, capture_output=True, check=True)
    samples = array.array('f')
    samples.frombytes(result.stdout)
    if sys.byteorder != 'little':
        samples.byteswap()
    if not 2400 <= len(samples) <= 12000 or not all(map(math.isfinite, samples)):
        raise ValueError('Swish must be a finite 50–250ms recording')
    dc = sum(samples) / len(samples)
    samples = [v - dc for v in samples]
    peak = max(map(abs, samples))
    rms = math.sqrt(sum(v * v for v in samples) / len(samples))
    if rms < 1e-5:
        raise ValueError('Silent tool swish')
    gain = min(10 ** (source['peak_dbfs'] / 20) / peak,
               10 ** (source['rms_dbfs'] / 20) / rms)
    pcm = array.array('h', (round(v * gain * 32767) for v in samples))
    peak_out = max(map(abs, pcm)) / 32768
    rms_out = math.sqrt(sum(v * v for v in pcm) / len(pcm)) / 32768
    if sys.byteorder != 'little':
        pcm.byteswap()
    output = io.BytesIO()
    with wave.open(output, 'wb') as wav:
        wav.setparams((1, 2, RATE, 0, 'NONE', 'not compressed'))
        wav.writeframes(pcm.tobytes())
    data = output.getvalue()
    if source.get('processed_sha256') and digest(data) != source['processed_sha256']:
        raise ValueError(f"Processed swish differs from the heard version: {source['id']}")
    return data, dict(id=source['id'], file=source['id'] + '.wav', sha256=digest(data),
                      bytes=len(data), frames=len(samples), peak=peak_out, rms=rms_out,
                      gain_db=20 * math.log10(gain), source_sha256=source['sha256'])


def prepare(output, inputs, download, ffmpeg):
    if output.exists():
        raise ValueError('Prepared output already exists; use a fresh directory')
    sources = selected_sources()
    original = source_bytes(sources, inputs, download)
    if not ffmpeg:
        install = {'win32': 'winget install Gyan.FFmpeg', 'darwin': 'brew install ffmpeg'}.get(
            sys.platform, 'sudo apt install ffmpeg')
        raise ValueError(f'ffmpeg is required to prepare approved audio: {install}; or pass --ffmpeg PATH')
    clips = [process(s, original[s['id']], ffmpeg) for s in sources]
    output.mkdir(parents=True)
    for data, row in clips:
        (output / row['file']).write_bytes(data)
    credits = ('Tool swing Foley: artisticdude, Swishes Sound Pack (CC0-1.0).\n'
               'https://opengameart.org/content/swishes-sound-pack\n'
               'https://creativecommons.org/publicdomain/zero/1.0/\n'
               'Selected members: swish-1.wav, swish-6.wav, swish-10.wav.\n'
               'Changes: highpass120Hz, mono48kHz PCM16, DC removal, bounded level.\n'
               'New native immersion, separate from faithful v20 contact sounds.\n')
    (output / 'ATTRIBUTION.txt').write_text(credits, encoding='utf-8', newline='\n')
    receipt = dict(schema_version=1, recipe_sha256=recipe_hash(), sources=sources,
                   clips=[row for _, row in clips],
                   attribution_sha256=digest((output / 'ATTRIBUTION.txt').read_bytes()))
    write_json(output / 'tool-swishes-generated.json', receipt)
    return receipt


def verify(prepared):
    receipt = read_json(prepared / 'tool-swishes-generated.json')
    sources = selected_sources()
    if receipt['recipe_sha256'] != recipe_hash() or receipt['sources'] != sources:
        raise ValueError('Prepared tool swishes predate current recipe/approval')
    expected = {s['id']: s for s in sources}
    if len(receipt['clips']) != len(expected) or {r['id'] for r in receipt['clips']} != set(expected):
        raise ValueError('Prepared swish inventory differs from approved sources')
    for row in receipt['clips']:
        source = expected[row['id']]
        if row['file'] != source['id'] + '.wav' or row['source_sha256'] != source['sha256']:
            raise ValueError('Invalid prepared swish identity')
        data = (prepared / row['file']).read_bytes()
        if len(data) != row['bytes'] or digest(data) != row['sha256']:
            raise ValueError('Prepared swish missing or changed')
        if row['sha256'] != source.get('processed_sha256', row['sha256']):
            raise ValueError('Prepared swish differs from approved waveform')
    if digest((prepared / 'ATTRIBUTION.txt').read_bytes()) != receipt['attribution_sha256']:
        raise ValueError('Prepared attribution changed')
    return receipt


def augment(weapons, prepared):
    receipt = verify(prepared)
    policy = read_json(REPO / INPUTS[2])
    file = weapons / 'weapons.json'
    pack = read_json(file)
    targets = []
    for target in policy['targets']:
        states = pack['images'][target['image']]['states']
        matching = [s for s in states if s['name'] == target['state']]
        if len(matching) != 1 or matching[0].get('sound', '') not in ('', policy['profile']):
            raise ValueError('Tool state changed; refusing to replace authored audio')
        targets.append(matching[0])
    destination = weapons / 'sounds/tool-swishes'
    profiles = {}
    resources = []
    for row in receipt['clips']:
        native_file = 'sounds/tool-swishes/' + row['file']
        profiles['bri:foley/tools/' + row['id']] = dict(file=native_file, volume=1.0)
        resources.append(dict(path=RESOURCE_PREFIX + row['file'], sha256=row['sha256'],
                              native_file=native_file, diagnostics=[]))
    profiles[policy['profile']] = dict(file=next(iter(profiles.values()))['file'],
                                      variants=policy['variants'], volume=policy['volume'])
    for key in profiles:
        if key in pack.get('sounds', {}) and pack['sounds'][key] != profiles[key]:
            raise ValueError(f'Conflicting tool profile {key}')
    destination.mkdir(parents=True, exist_ok=True)
    for name in [r['file'] for r in receipt['clips']] + ['ATTRIBUTION.txt', 'tool-swishes-generated.json']:
        shutil.copyfile(prepared / name, destination / name)
        if name in ['ATTRIBUTION.txt', 'tool-swishes-generated.json']:
            resources.append(dict(path=RESOURCE_PREFIX + name,
                                  sha256=digest((destination / name).read_bytes()),
                                  native_file='sounds/tool-swishes/' + name, diagnostics=[]))
    pack.setdefault('sounds', {}).update(profiles)
    pack['resources'] = [r for r in pack.get('resources', [])
                         if not r['path'].startswith(RESOURCE_PREFIX)] + resources
    for state in targets:
        state['sound'] = policy['profile']
    # The source definitions/evidence remain original; this hashed receipt names
    # every new file, approval and authored policy separately.
    write_json(file, pack)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument('--output', type=Path)
    action.add_argument('--augment-weapons', type=Path)
    action.add_argument('--verify', type=Path)
    parser.add_argument('--prepared', type=Path)
    parser.add_argument('--inputs', type=Path, default=REPO / 'content/_regeneration/tool-swish-inputs')
    parser.add_argument('--download', action='store_true')
    parser.add_argument('--ffmpeg', default=shutil.which('ffmpeg'))
    args = parser.parse_args()
    if args.verify:
        verify(args.verify)
    elif args.output:
        prepare(args.output, args.inputs, args.download, args.ffmpeg)
    elif args.prepared:
        augment(args.augment_weapons, args.prepared)
    else:
        with tempfile.TemporaryDirectory(prefix='tool-swishes-') as scratch:
            prepared = Path(scratch) / 'prepared'
            prepare(prepared, args.inputs, args.download, args.ffmpeg)
            augment(args.augment_weapons, prepared)
    print('Verified three approved tool swishes; original contact policy preserved.')


if __name__ == '__main__':
    main()
