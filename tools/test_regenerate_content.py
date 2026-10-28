"""Curated source-view regressions, with invented files and no original assets."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import regenerate_content as regen
import content_packs


class SourceIdentityTests(unittest.TestCase):
    def test_source_tree_has_canonical_case_sensitive_order_and_line_endings(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            names = ['src/lib.rs', 'build.rs', 'base-packages.json', 'src.rs', 'Cargo.toml']
            expected = hashlib.sha256()
            for name in names:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b'source\r\n')
            for name in sorted(names):
                expected.update(name.encode() + b'\0source\n\0')
            self.assertEqual(regen.tree_digest(root), expected.hexdigest())
            for name in names:
                (root / name).write_bytes(b'source\n')
            self.assertEqual(regen.tree_digest(root), expected.hexdigest())


class ReferenceViewTests(unittest.TestCase):
    def test_packaged_menu_guard_rejects_empty_or_changed_art_even_with_matching_self_hash(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'docs').mkdir()
            pack = root / 'content/ui'
            pack.mkdir(parents=True)
            data = b'reference image fixture'
            digest = hashlib.sha256(data).hexdigest()
            (root / 'docs/vanilla-reference-files.json').write_text(json.dumps({'files': [
                dict(path='screenshots/Reference.jpg', bytes=len(data), sha256=digest)]}))
            (root / 'content/packages.json').write_text(json.dumps({'packages': [
                dict(dir='ui', role='ui_pack')]}))
            manifest = pack / 'ui-pack.json'
            manifest.write_text(json.dumps({'images': {}}))
            with self.assertRaisesRegex(ValueError, 'missing menu image'):
                content_packs.verify_menu_images(root / 'content', root)
            entry = dict(file='picture.jpg', sha256=digest)
            manifest.write_text(json.dumps({'images': {'screenshots/reference': entry}}))
            (pack / 'picture.jpg').write_bytes(data)
            self.assertEqual(content_packs.verify_menu_images(root / 'content', root), 1)
            (pack / 'picture.jpg').write_bytes(b'changed')
            entry['sha256'] = hashlib.sha256(b'changed').hexdigest()
            manifest.write_text(json.dumps({'images': {'screenshots/reference': entry}}))
            with self.assertRaisesRegex(ValueError, 'differs from the curated reference'):
                content_packs.verify_menu_images(root / 'content', root)

    def test_curated_menu_picture_survives_but_personal_screenshot_does_not(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source, repo, view = root / 'source', root / 'repo', root / 'view'
            repo.mkdir()
            records = []
            for name in ('base/core', 'Add-Ons/example.zip', 'saves/example.bls',
                         'screenshots/Reference.jpg'):
                path = source / name
                path.parent.mkdir(parents=True, exist_ok=True)
                data = name.encode()
                path.write_bytes(data)
                records.append(dict(path=name, bytes=len(data),
                                    sha256=hashlib.sha256(data).hexdigest()))
            (source / 'screenshots/Personal.jpg').write_bytes(b'unlisted')
            (repo / 'reference.json').write_text(json.dumps({'files': records}))
            with patch.object(regen, 'REPO', repo), patch.object(regen, 'REFERENCE_FILES', 'reference.json'):
                regen.reference_view(source, view)
            self.assertEqual((view / 'screenshots/Reference.jpg').read_bytes(),
                             b'screenshots/Reference.jpg')
            self.assertFalse((view / 'screenshots/Personal.jpg').exists())
            before = regen.v20_identity(view)
            (view / 'screenshots/Reference.jpg').write_bytes(b'changed size')
            self.assertNotEqual(before, regen.v20_identity(view))
            self.assertEqual((source / 'screenshots/Personal.jpg').read_bytes(), b'unlisted')


if __name__ == '__main__':
    unittest.main()
