"""Release integrity and fresh-install defaults, without downloaded recordings."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

import addon_bundle
import generated_addons


class GeneratedBundleTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.repo = Path(self.scratch.name) / 'repo'
        self.bundle = Path(self.scratch.name) / 'bundle'
        self.directory = self.bundle / 'addons/npc_popculture'
        self.rules = self.directory.with_name('npc_popculture_rules')
        self.addon = {'id': 'npc_popculture', 'generated': 'npc-reactions'}
        for name in generated_addons.RECIPES['npc-reactions']['inputs']:
            self.write(self.repo / name, 'fixture\n')
        self.write(self.repo / 'packages/default-addons.json', {'schema_version': 3, 'addons': [self.addon]})
        self.write(self.directory / 'package.json', {'schema_version': 1, 'id': 'npc_popculture',
                   'name': 'NPC reactions', 'version': '1.0.0', 'api': 1,
                   'companions': ['npc_popculture_rules'], 'provides': []})
        self.write(self.rules / 'package.json', {'schema_version': 1, 'id': 'npc_popculture_rules',
                   'version': '1.0.0', 'api': 1, 'dependencies': {'npc_popculture': '^1.0.0'}, 'provides': []})
        for name in ['behaviour.json', 'reactions.rhai']:
            self.write(self.rules / name, 'fixture\n')
        for source, target in [('package.json', self.directory / 'package.json'),
                               ('rules-package.json', self.rules / 'package.json'),
                               ('behaviour.json', self.rules / 'behaviour.json'),
                               ('reactions.rhai', self.rules / 'reactions.rhai')]:
            self.write(self.repo / 'tools/npc_reactions' / source, target.read_text())
        self.write(self.directory / 'assets/clips/one.wav', 'synthetic sample')
        sha = hashlib.sha256(b'synthetic sample').hexdigest()
        self.write(self.repo / 'tools/npc_reactions/catalog.json', {'clips': [{'id': 'one', 'sha256': sha}]})
        self.write(self.directory / 'npc-reactions-generated.json', {
            'schema_version': 1, 'recipe_sha256': generated_addons.recipe_hash(self.repo, 'npc-reactions'),
            'clips': [{'id': 'one', 'sha256': sha, 'processed_sha256': sha}],
        })
        self.write(self.bundle / 'CREDITS.md', 'NPC reactions: content/addons/npc_popculture/npc-reactions-generated.json\n')

    def write(self, path, value):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value) if isinstance(value, (dict, list)) else value, encoding='utf-8')

    def test_fresh_release_carries_and_enables_recordings_then_companion(self):
        sources, credits = addon_bundle.default_sources(self.repo, self.bundle)
        self.assertEqual([p['id'] for p in sources], ['npc_popculture', 'npc_popculture_rules'])
        self.assertTrue(all(p['enabled'] for p in sources))
        self.assertEqual(credits, self.bundle / 'CREDITS.md')
        content = self.bundle  # fixture's addons directory is a native content root
        self.write(content / 'packages.json', {'schema_version': 1, 'packages': [
            {'id': p['id'], 'dir': 'addons/' + p['id']} for p in sources]})
        addon_bundle.verify_defaults(self.repo, content, credits)

    def test_stale_recipe_corrupt_recording_and_missing_policy_fail_closed(self):
        self.assertEqual(generated_addons.problems(self.repo, self.directory, 'npc-reactions'), [])
        self.write(self.directory / 'assets/clips/one.wav', 'corrupted')
        self.assertIn('changed', ';'.join(generated_addons.problems(self.repo, self.directory, 'npc-reactions')))
        self.write(self.repo / 'tools/npc_reactions/reactions.rhai', 'changed recipe')
        self.assertIn('predates', ';'.join(generated_addons.problems(self.repo, self.directory, 'npc-reactions')))
        self.rules.joinpath('package.json').unlink()
        self.assertTrue(addon_bundle.problems(self.directory, self.addon, self.repo))


if __name__ == '__main__':
    unittest.main()
