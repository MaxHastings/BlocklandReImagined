#!/usr/bin/env python3
"""Release publishing checks, without GitHub: run choice and cross-platform content."""
import json
import pathlib
import tempfile
import unittest
import zipfile
import publish_release
import release

PACKS = {'map-bundle-017', 'weapons-pack-009'}


def manifest(version, content_root, files):
    return {'version': version, 'content_config': f'{content_root}/packages.json',
            'files': [{'path': f'{content_root}/{path}', 'bytes': 1, 'sha256': sha} for path, sha in files.items()]}


BASE = {'map-bundle-017/a.json': 'aa', 'weapons-pack-009/b.bin': 'bb'}


class CompareTests(unittest.TestCase):
    def manifests(self, mac=None, linux=None, version='v1'):
        return {'windows': manifest('v1', 'content', {**BASE, 'packages.json': 'w', 'addons/x/p.json': '1'}),
                'macos': manifest(version, 'BlocklandReImagined.app/Contents/Resources/content',
                                  mac if mac is not None else {**BASE, 'packages.json': 'm'}),
                'linux': manifest('v1', 'content', linux if linux is not None else BASE)}

    def test_same_base_content_on_every_platform(self):
        self.assertEqual(publish_release.compare_platforms(self.manifests(), 'v1', PACKS), [])

    def test_wrong_version(self):
        problems = publish_release.compare_platforms(self.manifests(version='v0'), 'v1', PACKS)
        self.assertEqual(len(problems), 1)
        self.assertIn("macos zip is version 'v0'", problems[0])

    def test_changed_missing_and_extra_files(self):
        problems = publish_release.compare_platforms(
            self.manifests(mac={'map-bundle-017/a.json': 'zz', 'weapons-pack-009/b.bin': 'bb'},
                           linux={'map-bundle-017/a.json': 'aa', 'weapons-pack-009/c.bin': 'cc'}), 'v1', PACKS)
        self.assertEqual(problems, [
            'macos differs in 1 base content file(s) against Windows, first map-bundle-017/a.json',
            'linux lacks 1 base content file(s) against Windows, first weapons-pack-009/b.bin',
            'linux adds 1 base content file(s) against Windows, first weapons-pack-009/c.bin'])

    def test_files_outside_the_base_packs_are_not_compared(self):
        files = publish_release.base_pack_files(self.manifests()['windows'], PACKS)
        self.assertEqual(files, BASE)


class RunTests(unittest.TestCase):
    def test_newest_successful_run_at_the_commit_with_every_zip(self):
        names = set(publish_release.artifact_names('v1').values())
        runs = [{'id': 1, 'head_sha': 'abc', 'conclusion': 'success', 'created_at': '2026-10-12T01'},
                {'id': 2, 'head_sha': 'abc', 'conclusion': 'success', 'created_at': '2026-10-12T03'},
                {'id': 3, 'head_sha': 'abc', 'conclusion': 'failure', 'created_at': '2026-10-12T04'},
                {'id': 4, 'head_sha': 'def', 'conclusion': 'success', 'created_at': '2026-10-12T05'}]
        artifacts = {1: [{'name': n} for n in names],
                     2: [{'name': n, 'expired': n == 'mac-release'} for n in names]}
        self.assertEqual(publish_release.pick_run(runs, lambda i: artifacts.get(i, []), 'v1', 'abc'), 1)
        self.assertIsNone(publish_release.pick_run(runs, lambda i: artifacts.get(i, []), 'v2', 'abc'))

    def test_windows_artifact_carries_the_version(self):
        self.assertEqual(publish_release.artifact_names('v0.2.11')['windows'], 'BlocklandReImagined-v0.2.11-windows')


class ZipTests(unittest.TestCase):
    def test_reads_the_one_release_folder(self):
        with tempfile.TemporaryDirectory() as temp:
            path = pathlib.Path(temp) / 'r.zip'
            with zipfile.ZipFile(path, 'w') as archive:
                archive.writestr('Game-v1-linux/MANIFEST.json', '﻿' + json.dumps({'version': 'v1'}))
                archive.writestr('Game-v1-linux/bri-client', b'')
            self.assertEqual(publish_release.read_manifest(path), ('Game-v1-linux', {'version': 'v1'}))
            with zipfile.ZipFile(path, 'a') as archive:
                archive.writestr('stray.txt', b'')
            with self.assertRaises(SystemExit):
                publish_release.read_manifest(path)


class NotesTests(unittest.TestCase):
    def test_release_notes_go_above_the_download_instructions(self):
        with tempfile.TemporaryDirectory() as temp:
            notes = pathlib.Path(temp) / 'v1.md'
            notes.write_text('## What changed\n', encoding='utf-8')
            text = publish_release.notes_text('v1', notes)
            self.assertTrue(text.startswith('## What changed\n\n**Windows:**'))
            self.assertTrue(publish_release.notes_text('v1', pathlib.Path(temp) / 'none.md').startswith('**Windows:**'))


class BundleInputTests(unittest.TestCase):
    def test_release_refreshes_conversion_dependencies_and_ignores_documentation(self):
        self.assertTrue(release.bundle_inputs_changed(['crates/addon-import/ports/x/entry.json']))
        self.assertTrue(release.bundle_inputs_changed(['packages/default-addons.json']))
        self.assertTrue(release.bundle_inputs_changed(['tools/npc_reactions/reactions.rhai']))
        self.assertTrue(release.bundle_inputs_changed(['tools/build_npc_reactions.py']))
        self.assertTrue(release.bundle_inputs_changed(['crates/content/src/bot.rs']))
        self.assertTrue(release.bundle_inputs_changed(['Cargo.lock']))
        self.assertTrue(release.bundle_inputs_changed(['tools/imported_addons.py']))
        self.assertFalse(release.bundle_inputs_changed(['docs/x.md']))


if __name__ == '__main__':
    unittest.main()
