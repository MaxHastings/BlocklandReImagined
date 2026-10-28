"""Regressions: fresh engine/old bot data and fresh engine/old Slayer policy."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import imported_addons
import addon_bundle


class ImportedGenerationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name)
        self.port = self.repo / 'crates/addon-import/ports/slayer/rules.rhai'
        self.port.parent.mkdir(parents=True)
        self.port.write_text('correct team policy', encoding='utf-8')
        self.addon = self.repo / 'content/addons/zombie'
        self.rules = self.addon.with_name('zombie-rules')
        self.addon.mkdir(parents=True)
        self.rules.mkdir()
        self.write(self.addon / 'package.json', {'id': 'zombie', 'companions': ['zombie-rules']})
        self.write(self.rules / 'package.json', {'id': 'zombie-rules'})
        self.write(self.addon / 'bots.json', {'weapons': False})
        (self.rules / 'rules.rhai').write_text('correct team policy', encoding='utf-8')
        self.mock = patch.object(imported_addons, 'importer_hash', return_value='current importer')
        self.mock.start()
        self.addCleanup(self.mock.stop)

    def write(self, path, data):
        path.write_text(json.dumps(data), encoding='utf-8')

    def test_old_or_unstamped_generated_bot_data_cannot_pass(self):
        recipe = imported_addons.recipe_hash(self.repo)
        self.assertIn('receipt', ' '.join(imported_addons.problems(self.addon, recipe)))
        imported_addons.write_receipt(self.addon, recipe)
        self.assertEqual(imported_addons.problems(self.addon, recipe), [])
        self.write(self.addon / 'bots.json', {'weapons': True})
        self.assertIn('bots.json', ' '.join(imported_addons.problems(self.addon, recipe)))

    def test_changed_source_or_shipped_team_policy_invalidates_bundle(self):
        recipe = imported_addons.recipe_hash(self.repo)
        imported_addons.write_receipt(self.addon, recipe)
        (self.rules / 'rules.rhai').write_text('old auto sort', encoding='utf-8')
        self.assertIn('rules.rhai', ' '.join(imported_addons.problems(self.addon, recipe)))
        (self.rules / 'rules.rhai').write_text('correct team policy', encoding='utf-8')
        self.port.write_text('new team binding', encoding='utf-8')
        self.assertIn('predates', ' '.join(imported_addons.problems(
            self.addon, imported_addons.recipe_hash(self.repo))))
        with patch.object(imported_addons, 'importer_hash', return_value='new conversion implementation'):
            self.assertIn('predates', ' '.join(imported_addons.problems(
                self.addon, imported_addons.recipe_hash(self.repo))))

    def test_packaging_rejects_an_old_complete_original_and_its_rules(self):
        original = {'id': 'zombie', 'original': {'sha256': ['a' * 64],
                    'authors': ['Tester'], 'version': '1.0.0'}}
        (self.repo / 'packages').mkdir()
        self.write(self.repo / 'packages/default-addons.json',
                   {'schema_version': 3, 'addons': [original]})
        self.write(self.addon / 'package.json', {'id': 'zombie', 'version': '1.0.0',
                   'authors': ['Tester'], 'companions': ['zombie-rules'],
                   'provenance': {'source': 'original sha256 ' + 'a' * 64}})
        self.write(self.rules / 'package.json', {'id': 'zombie-rules',
                   'dependencies': {'zombie': '^1.0.0'}})
        (self.repo / 'content/CREDITS.md').write_text('Tester', encoding='utf-8')
        with self.assertRaisesRegex(SystemExit, 'receipt'):
            addon_bundle.default_sources(self.repo, self.repo / 'content')
        imported_addons.write_receipt(self.addon, imported_addons.recipe_hash(self.repo))
        sources, _ = addon_bundle.default_sources(self.repo, self.repo / 'content')
        self.assertEqual([p['id'] for p in sources], ['zombie', 'zombie-rules'])
        self.port.write_text('new team binding', encoding='utf-8')
        with self.assertRaisesRegex(SystemExit, 'predates'):
            addon_bundle.default_sources(self.repo, self.repo / 'content')

    def test_reference_uses_pinned_dependency_without_modifying_the_install(self):
        vanilla = self.repo / 'v20'
        (vanilla / 'base').mkdir(parents=True)
        (vanilla / 'Add-Ons').mkdir()
        ambient = vanilla / 'Add-Ons/Weapon_Parent.zip'
        ambient.write_bytes(b'other installed edition')
        selected = self.repo / 'Weapon_Parent.zip'
        selected.write_bytes(b'pinned edition with the required inherited fields')
        sha = addon_bundle.source_sha(selected)
        listing = [{'id': 'weapon_parent', 'original':
                    {'addon': 'Weapon_Parent', 'sha256': [sha]}}]
        view = self.repo / 'reference'
        chosen = addon_bundle.reference_view(vanilla, {'weapon_parent': [ambient, selected]}, listing, view)
        self.assertEqual(chosen, {'Weapon_Parent': sha})
        self.assertEqual((view / 'Add-Ons/Weapon_Parent.zip').read_bytes(), selected.read_bytes())
        self.assertEqual(ambient.read_bytes(), b'other installed edition')

    def test_folder_pin_keeps_native_case_sensitive_member_order_on_windows(self):
        import hashlib
        folder = self.repo / 'MixedCase'
        folder.mkdir()
        (folder / 'a.cs').write_bytes(b'lower')
        (folder / 'Z.cs').write_bytes(b'upper')
        expected = hashlib.sha256(b'Z.cs' + hashlib.sha256(b'upper').hexdigest().encode()
                                  + b'a.cs' + hashlib.sha256(b'lower').hexdigest().encode()).hexdigest()
        self.assertEqual(addon_bundle.source_sha(folder), expected)
