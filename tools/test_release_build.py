"""The release wrapper preserves shipping targets and propagates build failure."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import release_build as release


class BuildTests(unittest.TestCase):
    def test_preserves_the_original_platform_artifact_sets(self):
        expected = {
            'windows': ['bri-client', 'bri-import-addon', 'bri-server', 'bri-addon-check', 'bri-addon-run'],
            'linux': ['bri-client', 'bri-import-addon', 'bri-server', 'bri-addon-check', 'bri-addon-run'],
            'macos': ['bri-client', 'bri-import-addon', 'bri-addon-check', 'bri-addon-run'],
        }
        for system, binaries in expected.items():
            invocation = release.command(system)
            self.assertEqual(invocation[:5], ['cargo', 'build', '--release', '--locked', '--timings'])
            self.assertEqual([invocation[i + 1] for i, arg in enumerate(invocation) if arg == '--bin'], binaries)
            self.assertNotIn('--jobs', invocation)

    def test_failure_keeps_the_cargo_exit_code_and_diagnostic_report(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(release, 'stats', return_value={}), \
                patch.object(release.Monitor, 'sample'):
            self.assertEqual(release.build('windows', tmp, run=lambda _: 7), 7)
            report = json.loads((Path(tmp) / 'summary.json').read_text())
            self.assertEqual(len(report['steps']), 1)
            self.assertEqual(report['steps'][0]['exit_code'], 7)

    def test_missing_cargo_is_a_failure_with_a_report(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(release, 'stats', return_value={}), \
                patch.object(release.Monitor, 'sample'):
            def missing(_):
                raise FileNotFoundError('cargo missing')
            self.assertEqual(release.build('macos', tmp, run=missing), 1)
            report = json.loads((Path(tmp) / 'summary.json').read_text())
            self.assertIn('cargo missing', report['launch_error'])

    def test_observation_failure_does_not_fail_a_successful_build_or_change_environment(self):
        before = dict(os.environ)
        with tempfile.TemporaryDirectory() as tmp, patch.object(release, 'stats', return_value={}), \
                patch.object(release, 'memory', side_effect=OSError('unsupported host')):
            def complete(_):
                import time
                deadline = time.monotonic() + 2
                while not (Path(tmp) / 'host-memory.jsonl').exists() or not (Path(tmp) / 'host-memory.jsonl').stat().st_size:
                    if time.monotonic() >= deadline:
                        self.fail('monitor did not record unavailable memory')
                    time.sleep(0.01)
                return 0
            self.assertEqual(release.build('linux', tmp, run=complete), 0)
            sample = json.loads((Path(tmp) / 'host-memory.jsonl').read_text().splitlines()[0])
            self.assertIn('unsupported host', sample['unavailable'])
        self.assertEqual(dict(os.environ), before)


if __name__ == '__main__':
    unittest.main()
