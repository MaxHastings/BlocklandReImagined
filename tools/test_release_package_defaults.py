"""Installed-source default refresh used by Linux release packaging."""
import copy
import unittest

import addon_bundle


class ReleaseDefaultsTests(unittest.TestCase):
    def test_installed_defaults_refresh_without_mutating_or_duplicating_source(self):
        source = [dict(id='base', version='1', side='shared', dir='base', role='audio'),
                  dict(id='mod', version='old', side='shared', dir='addons/mod'),
                  dict(id='mod-rules', version='old', side='shared', dir='addons/mod-rules'),
                  dict(id='optional', version='old', side='shared', dir='addons/optional')]
        before = copy.deepcopy(source)
        mods = [dict(id='mod', version='new', side='shared', dir='addons/mod'),
                dict(id='mod-rules', version='new', side='server', dir='addons/mod-rules'),
                dict(id='optional', version='new', side='client', dir='addons/optional', enabled=False)]
        result = addon_bundle.release_packages(source, mods)
        self.assertEqual(source, before)
        self.assertEqual([p['id'] for p in result], ['base', 'mod', 'mod-rules'])
        self.assertEqual(result[1]['version'], 'new')
        self.assertEqual(result[2]['side'], 'server')
        self.assertEqual(result[0], source[0])
        self.assertEqual(addon_bundle.release_packages(result, mods), result)

    def test_fresh_defaults_append_in_dependency_order(self):
        mods = [dict(id=i, version='1', side='server', dir='addons/'+i)
                for i in ['mod', 'mod-rules']]
        self.assertEqual([p['id'] for p in addon_bundle.release_packages([], mods)],
                         ['mod', 'mod-rules'])

    def test_conflicting_identity_directory_or_duplicate_is_refused(self):
        mod = dict(id='mod', version='1', side='shared', dir='addons/mod')
        for source in [[dict(mod, id='other')], [dict(mod, dir='other')], [mod, mod]]:
            with self.subTest(source=source), self.assertRaises(SystemExit):
                addon_bundle.release_packages(source, [mod])


if __name__ == '__main__':
    unittest.main()
