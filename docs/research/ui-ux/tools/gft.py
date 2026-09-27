#!/usr/bin/env python3
"""Decoder for Blockland v20 Torque font caches (base/client/ui/cache/*.gft).

Layout (verified against all caches in the v20 install, little-endian):
  u32 version (=1) | u32 height | u32 baseline | u32 glyphCount
  glyphCount x 9-byte records:
      u16 sheetIndex | u8 x | u8 y | u8 width | u8 height |
      s8 xOrigin | s8 yOrigin | s8 xIncrement
  u32 sheetCount | sheetCount x embedded PNG streams (glyph atlases)
  256 x u16 remap: character code (Latin-1/ANSI) -> glyph index, 0xFFFF = none
This is the older TGE layout; Torque3D's GFont::read (face-name string,
ascent/descent, u32 fields) does NOT match. Read-only research tool.
Glyph placement: draw at (penX + xOrigin, baselineY - yOrigin); advance xIncrement.
"""
import struct, io, sys, json, os
from PIL import Image

class GFT:
    def __init__(self, path):
        d = open(path, 'rb').read()
        self.path = path
        self.version, self.height, self.baseline, n = struct.unpack_from('<4I', d, 0)
        if self.version != 1:
            raise ValueError(f'{path}: unsupported gft version {self.version}')
        off = 16
        self.glyphs = []
        for _ in range(n):
            self.glyphs.append(struct.unpack_from('<HBBBBbbb', d, off)); off += 9
        (ns,) = struct.unpack_from('<I', d, off); off += 4
        self.sheets = []
        for _ in range(ns):
            end = d.index(b'IEND', off) + 8
            self.sheets.append(Image.open(io.BytesIO(d[off:end])).convert('RGBA')); off = end
        if len(d) - off != 512:
            raise ValueError(f'{path}: unexpected remap size {len(d) - off}')
        self.remap = list(struct.unpack_from('<256H', d, off))

    def glyph(self, ch):
        c = ord(ch) if isinstance(ch, str) else ch
        if c > 255 or self.remap[c] == 0xFFFF:
            return None
        return self.glyphs[self.remap[c]]

    def width(self, text):
        return sum((g[7] if g else 0) for g in map(self.glyph, text))

    def draw(self, canvas, xy, text, color=(0, 0, 0, 255)):
        """Alpha-composite text; glyph sheet alpha (or luminance) is the coverage mask."""
        x, y = xy
        for ch in text:
            g = self.glyph(ch)
            if not g:
                continue
            si, gx, gy, gw, gh, xo, yo, adv = g
            if gw and gh:
                crop = self.sheets[si].crop((gx, gy, gx + gw, gy + gh))
                a = crop.getchannel('A')
                if a.getextrema() == (255, 255):      # opaque sheet: use luminance as coverage
                    a = crop.convert('L')
                tile = Image.new('RGBA', crop.size, color)
                tile.putalpha(Image.eval(a, lambda v: v * color[3] // 255))
                canvas.alpha_composite(tile, (int(x + xo), int(y + self.baseline - yo)))
            x += adv
        return x

if __name__ == '__main__':
    cache, out = sys.argv[1], sys.argv[2]
    os.makedirs(out, exist_ok=True)
    summary = {}
    for f in sorted(os.listdir(cache)):
        if not f.endswith('.gft'):
            continue
        g = GFT(os.path.join(cache, f))
        mapped = [c for c in range(256) if g.remap[c] != 0xFFFF]
        summary[f] = {'height': g.height, 'baseline': g.baseline, 'glyphs': len(g.glyphs),
                      'sheets': [s.size for s in g.sheets], 'mapped_codes': [min(mapped), max(mapped), len(mapped)]}
        sample = 'The quick brown fox 0123 !?@#&'
        im = Image.new('RGBA', (g.width(sample) + 8, g.height + 8), (255, 255, 255, 255))
        g.draw(im, (4, 4), sample)
        im.save(os.path.join(out, f.replace('.gft', '.sample.png')))
    json.dump(summary, open(os.path.join(out, 'gft-summary.json'), 'w'), indent=1)
    print(json.dumps(summary)[:600])
