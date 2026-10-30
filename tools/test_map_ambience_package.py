"""Fixed-palette source identity and private-candidate release boundary."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import addon_bundle
import build_map_ambience
import generated_addons


class Palette(unittest.TestCase):
    def test_default_bundle_carries_shared_media_and_required_server_companion(self):
        # Synthetic file-integrity fixture, not source recording fidelity.
        with tempfile.TemporaryDirectory() as folder:
            bundle=Path(folder)
            out=bundle/'addons/map_ambience'
            rules=out.with_name('map_ambience_rules')
            (out/'assets/clips').mkdir(parents=True)
            rules.mkdir(parents=True)
            here=build_map_ambience.HERE
            (out/'package.json').write_bytes((here/'package.json').read_bytes())
            (rules/'package.json').write_bytes((here/'rules-package.json').read_bytes())
            for name in ['behaviour.json','ambience.rhai']:
                (rules/name).write_bytes((here/name).read_bytes())
            palette,selected=build_map_ambience.selection()
            (rules/'palette.rhai').write_text(build_map_ambience.policy(palette))
            registry=json.loads((here/'source-approvals.json').read_text())
            clips=[]
            sounds={}
            for key,row in selected.items():
                raw=b'synthetic file-integrity sample'
                name=build_map_ambience.clip_file(row)
                (out/'assets/clips'/name).write_bytes(raw)
                clips.append(dict(row,file='assets/clips/'+name,processed_sha256=build_map_ambience.sha(raw),
                                  source_approval=build_map_ambience.approval_receipt(registry,row['input_sha256'])))
                sounds['map_ambience:'+key]=build_map_ambience.sound_definition(row)
            (out/'assets/weapons.json').write_text(json.dumps({'schema_version':5,'id':'map_ambience','sounds':sounds}))
            files=lambda p:{f.relative_to(p).as_posix():build_map_ambience.sha(f.read_bytes()) for f in p.rglob('*') if f.is_file()}
            receipt={'schema_version':1,'audition_only':False,
                     'recipe_sha256':generated_addons.recipe_hash(build_map_ambience.REPO,'map-ambience'),
                     'clips':clips,'maps':palette['maps'],'files':files(out),'rules_files':files(rules)}
            (out/'map-ambience-generated.json').write_text(json.dumps(receipt))
            (bundle/'CREDITS.md').write_text('Map Ambience: map-ambience-generated.json\n')
            spec={'id':'map_ambience','generated':'map-ambience','enabled':True}
            with patch.object(addon_bundle,'read_list',return_value=[spec]):
                sources,_=addon_bundle.default_sources(build_map_ambience.REPO,bundle)
            self.assertEqual([s['id'] for s in sources],['map_ambience','map_ambience_rules'])
            self.assertEqual(sources[1]['companion_of'],'map_ambience')
            self.assertTrue(all(s['enabled'] for s in sources))
            self.assertEqual({v['kind'] for v in json.loads((out/'package.json').read_text())['provides']},{'weapons'})
            # A self-consistent file receipt cannot silently turn the fridge
            # into an omnipresent room loop by deleting its authored distances.
            sounds['map_ambience:fridge-loop'].pop('spatial')
            (out/'assets/weapons.json').write_text(json.dumps({'schema_version':5,'id':'map_ambience','sounds':sounds}))
            receipt['files']=files(out)
            receipt['files'].pop('map-ambience-generated.json')
            (out/'map-ambience-generated.json').write_text(json.dumps(receipt))
            self.assertIn('spatial/context policy',';'.join(build_map_ambience.problems(build_map_ambience.REPO,out)))
            (rules/'package.json').unlink()
            self.assertTrue(addon_bundle.problems(out,spec,build_map_ambience.REPO),'missing server policy cannot ship as complete')

    def test_chosen_detail_identity_is_not_overwritten_by_master_catalog_key(self):
        palette, clips = build_map_ambience.selection()
        self.assertEqual(len(palette['maps']), 21)
        self.assertEqual(set(clips), {'room','dry-air','cold-air','leaves','shore','rain',
                                      'bird-robin','bird-blackbird','fridge-loop'})
        for key, row in clips.items():
            self.assertEqual(key,row['key'], 'master key must not replace the actual packaged clip identity')
        self.assertEqual(clips['bird-robin']['source_id'],'original-wav:robin')
        self.assertEqual(clips['bird-blackbird']['source_id'],'original-wav:blackbird')

    def test_local_fridge_retains_evidenced_position_and_no_listener_environment_gate(self):
        palette, clips=build_map_ambience.selection()
        local_maps=[m for m in palette['maps'] if m['locals']]
        self.assertEqual({m['name'] for m in local_maps},{'Kitchen','Kitchen - Dark'})
        for m in local_maps:
            self.assertEqual(m['locals'][0]['at'],[-382.75,170.284,288.171])
            self.assertEqual(m['locals'][0]['evidence']['material'],'FRIDGE1')
        sound=build_map_ambience.sound_definition(clips['fridge-loop'])
        self.assertEqual(sound['spatial'],{'reference_distance':45.0,'max_distance':270.0})
        self.assertNotIn('environment',sound,'local fridge follows actual distance/obstruction on either side of a room')
        self.assertTrue(sound['music'] and sound['looping'],'local loop uses existing streaming path')
        self.assertEqual(build_map_ambience.sound_definition(clips['room'])['environment'],'interior')

    def test_audition_marker_is_rejected_by_public_generated_recipe_dispatch(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder)
            (p/'map-ambience-generated.json').write_text(json.dumps({'audition_only':True}))
            self.assertEqual(generated_addons.problems(build_map_ambience.REPO,p,'map-ambience'),
                             ['Private ambience audition is not a shipping-approved package'])


if __name__ == '__main__':
    unittest.main()
