"""Challenge manual playback selection without modifying converted originals."""
import json
from pathlib import Path
import shutil
import tempfile
import unittest

from map_ambience import original_policy as policy


class OriginalPolicy(unittest.TestCase):
    def test_only_waterfall_shore_play_and_other_source_bytes_remain_untouched(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);repo=root/'repo';config=repo/'tools/map_ambience';config.mkdir(parents=True)
            shutil.copyfile(policy.HERE/'original-emitter-policy.json',config/'original-emitter-policy.json')
            for package,names,active in [('map_creek',['waterfallloop','shoreloop','critterloop'],2),('map_forest',['crickets'],0)]:
                native=root/package;native.mkdir();rules=root/(package+'_ambience_rules');rules.mkdir()
                (native/'package.json').write_text(json.dumps({'id':package}))
                map_id='map_creek:map/map_creek' if package=='map_creek' else 'map_forest:map/forestnight'
                rows=[{'map':map_id,'profile':map_id+'/sound/'+name,'ambient_key':'map_audio/'+policy.sha((map_id+'/sound/'+name).encode())[:20],
                       'position':[1,2,3],'spatial':{'reference_distance':40,'max_distance':200}} for name in names]
                raw=json.dumps({'sources':rows}).encode();(native/'map-audio-provenance.json').write_bytes(raw)
                (native/'original.wav').write_bytes(b'synthetic unchanged source bytes')
                (rules/'map-audio.rhai').write_text('// original generated policy retained in override receipt')
                receipt=policy.apply(repo,native)
                self.assertEqual(len(receipt['enabled_profiles']),active)
                self.assertEqual(len(receipt['inactive_profiles']),len(names)-active)
                self.assertEqual((native/'map-audio-provenance.json').read_bytes(),raw)
                self.assertEqual((native/'original.wav').read_bytes(),b'synthetic unchanged source bytes')
                self.assertEqual(policy.problems(repo,native),[])
                self.assertEqual(policy.apply(repo,native),receipt,'repeat application preserves original conversion evidence')
                (rules/'map-audio.rhai').write_text('// somebody changed this')
                self.assertTrue(policy.problems(repo,native))
                with self.assertRaisesRegex(ValueError,'preserve edits'):
                    policy.apply(repo,native)


if __name__=='__main__':unittest.main()
