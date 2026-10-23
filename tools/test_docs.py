"""Reader-path and packaged-guide regressions; no original game content needed."""
import re
import tempfile
import unittest
from pathlib import Path

import check_docs
import package_guides


class DocumentationTests(unittest.TestCase):
    def test_platform_guide_inventory_matches_packagers(self):
        for platform, script in (('windows', 'package_playtest.ps1'),
                                 ('linux', 'package_playtest.sh'),
                                 ('macos', 'package_mac.sh')):
            with self.subTest(platform=platform):
                source = (check_docs.ROOT / 'tools' / script).read_text(encoding='utf-8')
                actual = set(re.findall(r'docs/([A-Z][A-Z-]*\.md)', source))
                expected = set(package_guides.PLAYER_GUIDES)
                expected.remove('DEDICATED-SERVER.md' if platform == 'macos' else 'PLAYTEST-MAC.md')
                self.assertEqual(actual, expected)

    def test_current_reader_links(self):
        cache = {}
        errors = [error for path in check_docs.maintained_docs(check_docs.ROOT)
                  for error in check_docs.check_links(path, check_docs.ROOT, cache,
                                                     online_root=check_docs.ROOT)]
        self.assertEqual(errors, [])

    def test_real_release_layouts(self):
        for platform in ('windows', 'linux', 'macos'):
            for stress_lab in (False, True):
                with self.subTest(platform=platform, stress_lab=stress_lab):
                    self.assertEqual(check_docs.check_release_guides(
                        check_docs.ROOT, platform, stress_lab), [])

    def test_distinguishes_player_and_creator_playtest(self):
        available = {'PLAYTEST.md', 'RULE-WORKSHOP-PLAYTEST.md'}
        source = '[recipes](rule-workshop/PLAYTEST.md#start-here)'
        self.assertEqual(package_guides.release_links(source, 'PLAYTEST.md', available),
                         '[recipes](RULE-WORKSHOP-PLAYTEST.md#start-here)')
        self.assertEqual(package_guides.release_links('[recipes](PLAYTEST.md)',
                         'rule-workshop/DESIGN.md', available),
                         '[recipes](RULE-WORKSHOP-PLAYTEST.md)')

    def test_release_contains_current_readme_and_organized_guides_not_old_test_cards(self):
        with tempfile.TemporaryDirectory() as folder:
            destination = Path(folder)
            (destination / 'TESTER-GUIDE.md').write_text('# Instructions\n')
            package_guides.copy_guides(check_docs.ROOT, destination, 'v0.3.4-fix1')
            self.assertIn('v0.3.4-fix1', (destination / 'README.txt').read_text())
            self.assertTrue((destination / 'docs/TESTER-GUIDE.md').is_file())
            self.assertTrue((destination / 'docs/RULE-WORKSHOP-PLAYTEST.md').is_file())
            self.assertEqual(list(destination.glob('*.md')), [])
            self.assertFalse(any(p.name.startswith('V0.') for p in destination.rglob('*')))
            self.assertFalse((destination / 'docs/RULE-WORKSHOP-DESIGN.md').exists())

    def test_absent_platform_guide_and_history_go_online(self):
        source = '[Mac](PLAYTEST-MAC.md#where-things-are)'
        self.assertEqual(package_guides.release_links(source, 'TESTER-GUIDE.md', set()),
                         '[Mac](' + package_guides.ONLINE_DOCS +
                         'PLAYTEST-MAC.md#where-things-are)')
        source = '[receipt](../progress/receipt.md)'
        self.assertEqual(package_guides.release_links(source,
                         'rule-workshop/V0.2.3-RELEASE-NOTES.md', set()),
                         '[receipt](' + package_guides.ONLINE_DOCS + 'progress/receipt.md)')

    def test_existing_flat_links_external_and_heading_links_stay(self):
        source = '[recipe](RULE-WORKSHOP-PLAYTEST.md) [web](https://example.com/a#b) [local](#start)'
        self.assertEqual(package_guides.release_links(source, 'PLAYTEST.md',
                         {'RULE-WORKSHOP-PLAYTEST.md'}), source)

    def test_heading_slugs_and_fenced_examples(self):
        text = '# One `thing`\n## One thing\n## Café & tea\n```md\n# Fake\n```\n'
        self.assertEqual(check_docs.anchors(text), {'one-thing', 'one-thing-1', 'café--tea'})
        self.assertEqual(check_docs.prose('```md\n[bad](missing)\n```\n'), '\n\n\n')

    def test_reports_broken_paths_anchors_and_escape(self):
        with tempfile.TemporaryDirectory(prefix='bri-doc-test-') as folder:
            root = Path(folder)
            page = root / 'README.md'
            page.write_text('# Start\n[bad](gone.md)\n[bad](#gone)\n[bad](../escape.md)\n'
                            '[good](#start)\n```md\n[example](missing.md)\n```\n', encoding='utf-8')
            errors = check_docs.check_links(page, root)
            self.assertEqual(len(errors), 3)
            self.assertIn('missing path', errors[0])
            self.assertIn('missing heading', errors[1])
            self.assertIn('escapes documentation root', errors[2])

    def test_reference_links_and_encoded_paths(self):
        with tempfile.TemporaryDirectory(prefix='bri-doc-test-') as folder:
            root = Path(folder)
            (root / 'Some page.md').write_text('# Heading\n', encoding='utf-8')
            page = root / 'README.md'
            page.write_text('[one](Some%20page.md#heading)\n[two][ref]\n'
                            '[ref]: <Some page.md#heading>\n[bad]: missing.md\n', encoding='utf-8')
            errors = check_docs.check_links(page, root)
            self.assertEqual(len(errors), 1)
            self.assertIn('missing.md', errors[0])

    def test_online_fallback_is_checked_against_repository_source(self):
        with tempfile.TemporaryDirectory(prefix='bri-doc-test-') as folder:
            root = Path(folder)
            (root / 'docs').mkdir()
            (root / 'docs/MAC.md').write_text('# Files\n', encoding='utf-8')
            package = root / 'release'
            package.mkdir()
            page = package / 'README.md'
            page.write_text('[good](' + check_docs.REPO_BLOB + 'docs/MAC.md#files)\n'
                            '[bad](' + check_docs.REPO_BLOB + 'docs/MAC.md#gone)\n'
                            '[bad](' + check_docs.REPO_BLOB + 'docs/missing.md)\n', encoding='utf-8')
            errors = check_docs.check_links(page, package, online_root=root)
            self.assertEqual(len(errors), 2)
            self.assertIn('missing heading', errors[0])
            self.assertIn('missing path', errors[1])


if __name__ == '__main__':
    unittest.main()
