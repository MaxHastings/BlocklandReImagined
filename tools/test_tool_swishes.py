"""Boundary regressions: don't replace source audio or trust a stale receipt."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import prepare_tool_swishes as sw


class ToolSwishes(unittest.TestCase):
    def fixture(self, root):
        prepared = root / 'prepared'
        prepared.mkdir()
        sources = sw.selected_sources()
        clips = []
        for s in sources:
            # Invented bytes test identity handling only. Actual approved PCM is
            # checked by the native consumer and the separate preparation run.
            data = s['id'].encode()
            s['processed_sha256'] = sw.digest(data)
            file = s['id'] + '.wav'
            (prepared / file).write_bytes(data)
            clips.append(dict(id=s['id'], file=file, source_sha256=s['sha256'],
                              sha256=sw.digest(data), bytes=len(data)))
        credits = b'invented attribution fixture\n'
        (prepared / 'ATTRIBUTION.txt').write_bytes(credits)
        sw.write_json(prepared / 'tool-swishes-generated.json',
                      dict(recipe_sha256=sw.recipe_hash(), sources=sources, clips=clips,
                           attribution_sha256=sw.digest(credits)))
        weapons = root / 'weapons'
        weapons.mkdir()
        original = dict(schema_version=5, evidence=['original evidence'],
                        sounds={'source_hit': {'file': 'original.wav'}},
                        resources=[dict(path='source/hit', sha256='source-hash')],
                        images={image: dict(states=[dict(name='Ready', sound=''),
                                                     dict(name='Fire', sound='', script='onFire'),
                                                     dict(name='StopFire', sound='')],
                                            scripts={'onfire': {'on_fire': 'unchanged'}})
                                for image in ['v20.image.hammerimage', 'v20.image.wrenchimage']})
        original['images']['other_tool'] = dict(states=[dict(name='Fire', sound='source_hit')])
        sw.write_json(weapons / 'weapons.json', original)
        return prepared, weapons, sources, original

    def test_augmentation_preserves_every_original_field_except_two_silent_fire_states(self):
        with tempfile.TemporaryDirectory() as directory:
            prepared, weapons, sources, original = self.fixture(Path(directory))
            with patch.object(sw, 'selected_sources', return_value=sources):
                sw.augment(weapons, prepared)
                first = sw.read_json(weapons / 'weapons.json')
                sw.augment(weapons, prepared)
                self.assertEqual(first, sw.read_json(weapons / 'weapons.json'))
            restored = copy.deepcopy(first)
            for target in sw.read_json(sw.REPO / sw.INPUTS[2])['targets']:
                states = restored['images'][target['image']]['states']
                self.assertEqual(next(s for s in states if s['name'] == 'Fire')['sound'],
                                 'bri:foley/tools/swing')
                next(s for s in states if s['name'] == 'Fire')['sound'] = ''
            restored['sounds'] = {k: v for k, v in restored['sounds'].items()
                                  if not k.startswith('bri:foley/tools/')}
            restored['resources'] = [r for r in restored['resources']
                                     if not r['path'].startswith(sw.RESOURCE_PREFIX)]
            self.assertEqual(restored, original)

    def test_authored_sound_or_changed_clip_is_refused_before_catalog_mutation(self):
        with tempfile.TemporaryDirectory() as directory:
            prepared, weapons, sources, original = self.fixture(Path(directory))
            original['images']['v20.image.hammerimage']['states'][1]['sound'] = 'source_hit'
            sw.write_json(weapons / 'weapons.json', original)
            before = (weapons / 'weapons.json').read_bytes()
            with patch.object(sw, 'selected_sources', return_value=sources):
                with self.assertRaisesRegex(ValueError, 'replace authored audio'):
                    sw.augment(weapons, prepared)
                self.assertEqual((weapons / 'weapons.json').read_bytes(), before)
                self.assertFalse((weapons / 'sounds').exists())
                (prepared / (sources[0]['id'] + '.wav')).write_bytes(b'changed')
                with self.assertRaisesRegex(ValueError, 'missing or changed'):
                    sw.augment(weapons, prepared)
                self.assertEqual((weapons / 'weapons.json').read_bytes(), before)


if __name__ == '__main__':
    unittest.main()
