#!/usr/bin/env python3
"""Content snapshot integrity and private-upload regressions, without game assets."""
import hashlib
import json
import pathlib
import tempfile
import unittest
import zipfile
from unittest.mock import patch
import ci_content


class SnapshotTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = pathlib.Path(self.temp.name)
        self.archive = self.root / 'ci-content.zip'

    def snapshot(self, data=b'authored', extra=None, omit=False):
        info = {'schema_version': 1, 'packs': ['changed-name'], 'files': {
            'changed-name/manifest.json': hashlib.sha256(b'authored').hexdigest()}}
        with zipfile.ZipFile(self.archive, 'w') as out:
            out.writestr('ci-content.json', json.dumps(info))
            if not omit:
                out.writestr('changed-name/manifest.json', data)
            if extra:
                out.writestr(extra, b'unlisted')

    def test_round_trip_and_changed_folder_name(self):
        self.snapshot()
        self.assertEqual(ci_content.verify(self.archive)['packs'], ['changed-name'])
        ci_content.extract(self.archive, self.root / 'extracted')
        self.assertEqual((self.root / 'extracted/changed-name/manifest.json').read_bytes(), b'authored')

    def test_same_folder_changed_bytes(self):
        self.snapshot(data=b'obsolete')
        with self.assertRaisesRegex(SystemExit, 'changed or damaged'):
            ci_content.verify(self.archive)

    def test_missing_file(self):
        self.snapshot(omit=True)
        with self.assertRaisesRegex(SystemExit, 'file manifest'):
            ci_content.verify(self.archive)

    def test_unlisted_file(self):
        self.snapshot(extra='changed-name/stale.json')
        with self.assertRaisesRegex(SystemExit, 'file manifest'):
            ci_content.verify(self.archive)

    def test_unsafe_member_before_extraction(self):
        for name in ['../escape', '/absolute', 'C:/escape', r'..\escape']:
            self.snapshot(extra=name)
            with self.assertRaisesRegex(SystemExit, 'unsafe path'):
                ci_content.verify(self.archive)

    def test_pack_hashes_archived_bytes(self):
        content = self.root / 'content'
        (content / 'renamed').mkdir(parents=True)
        (content / 'renamed/file').write_bytes(b'fixture')
        (content / 'packages.json').write_text(json.dumps({'schema_version': 1, 'packages': [{'dir': 'renamed'}]}))
        ci_content.pack(content, self.archive)
        info = ci_content.verify(self.archive)
        self.assertEqual(info['files']['renamed/file'], hashlib.sha256(b'fixture').hexdigest())
        self.assertIn('packages.json', info['files'])

    def test_refuses_published_release_without_uploading(self):
        with patch('ci_content.shutil.which', return_value='gh'), patch('ci_content.subprocess.run') as run:
            run.return_value.returncode = 0
            run.return_value.stdout = '{"isDraft": false}'
            with self.assertRaisesRegex(SystemExit, 'published'):
                ci_content.upload(self.archive)
            self.assertEqual(run.call_count, 1)


if __name__ == '__main__':
    unittest.main()
