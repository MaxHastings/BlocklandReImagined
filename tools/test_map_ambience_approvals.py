"""Challenge shipping adoption, not audition generation, with synthetic media."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from map_ambience.approvals import original_map_problems


class Approvals(unittest.TestCase):
    def test_pending_substitution_and_deleted_receipt_are_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            registry_path = root / 'tools/map_ambience/source-approvals.json'
            registry_path.parent.mkdir(parents=True)
            package = root / 'package'
            clips = package / 'assets/map-audio'
            clips.mkdir(parents=True)
            raw = b'synthetic source bytes; not a licensed recording'
            sha = hashlib.sha256(raw).hexdigest()
            (clips / 'test.wav').write_bytes(raw)
            (package / 'assets/weapons.json').write_text(json.dumps({'sounds': {
                'test:map/sound/river': {'file': 'map-audio/test.wav'}}}))
            receipt_path = package / 'map-audio-provenance.json'
            receipt = {'schema_version': 1, 'sources': [
                {'profile': 'test:map/sound/river', 'sha256': sha}]}
            receipt_path.write_text(json.dumps(receipt))
            registry = {'schema_version': 1, 'sources': [
                {'id': 'test:river', 'input_sha256': sha, 'state': 'pending', 'approval_evidence': None}]}
            registry_path.write_text(json.dumps(registry))
            self.assertIn('approval pending', ';'.join(original_map_problems(root, package)))
            registry['sources'][0]['state'] = 'approved'
            registry_path.write_text(json.dumps(registry))
            self.assertTrue(original_map_problems(root, package), 'state without human evidence is insufficient')
            registry['sources'][0]['approval_evidence'] = 'Synthetic test-only explicit source choice'
            registry_path.write_text(json.dumps(registry))
            self.assertEqual(original_map_problems(root, package), [])
            (clips / 'test.wav').write_bytes(b'unapproved replacement')
            self.assertIn('differs from its source receipt', ';'.join(original_map_problems(root, package)))
            (clips / 'test.wav').write_bytes(raw)
            receipt_path.unlink()
            self.assertTrue(original_map_problems(root, package), 'deleting receipt cannot bypass approval')

    def test_unchanged_package_requires_no_new_approval(self):
        with tempfile.TemporaryDirectory() as folder:
            self.assertEqual(original_map_problems(Path(folder), Path(folder)), [])


if __name__ == '__main__':
    unittest.main()
