"""Fixed-palette source identity and private-candidate release boundary."""
import json
from pathlib import Path
import tempfile
import unittest

import build_map_ambience
import generated_addons


class Palette(unittest.TestCase):
    def test_chosen_detail_identity_is_not_overwritten_by_master_catalog_key(self):
        palette, clips = build_map_ambience.selection()
        self.assertEqual(len(palette['maps']), 21)
        self.assertEqual(set(clips), {'room','dry-air','cold-air','leaves','shore','rain',
                                      'bird-robin','bird-blackbird'})
        for key, row in clips.items():
            self.assertEqual(key,row['key'], 'master key must not replace the actual packaged clip identity')
        self.assertEqual(clips['bird-robin']['source_id'],'original-wav:robin')
        self.assertEqual(clips['bird-blackbird']['source_id'],'original-wav:blackbird')

    def test_audition_marker_is_rejected_by_public_generated_recipe_dispatch(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder)
            (p/'map-ambience-generated.json').write_text(json.dumps({'audition_only':True}))
            self.assertEqual(generated_addons.problems(build_map_ambience.REPO,p,'map-ambience'),
                             ['Private ambience audition is not a shipping-approved package'])


if __name__ == '__main__':
    unittest.main()
