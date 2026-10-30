"""Microphone permission regressions in the actual Mac packaging recipe."""
import plistlib
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


RECIPE = Path(__file__).resolve().parent / 'package_mac.sh'


class MacMicrophonePermissionTests(unittest.TestCase):
    def test_generated_app_declares_microphone_access_before_signing(self):
        source = RECIPE.read_text(encoding='utf-8')
        generated = source.split('<<PLIST\n', 1)[1].split('\nPLIST', 1)[0]
        info = plistlib.loads(generated.encode())
        self.assertTrue(info['NSMicrophoneUsageDescription'].strip())
        self.assertLess(source.index('<<PLIST\n'),
                        source.index('codesign --force --deep --sign'))

    def test_package_verifier_rejects_apps_without_a_permission_description(self):
        source = RECIPE.read_text(encoding='utf-8')
        verifier = source.split("<<'PY'\n", 1)[1].split('\nPY', 1)[0]
        # Run the actual permission preflight before content verification,
        # which requires proprietary content and is checked by release builds.
        preflight = verifier.split('sys.path.insert', 1)[0]
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'BlocklandReImagined.app/Contents/Info.plist'
            path.parent.mkdir(parents=True)
            for description, allowed in [(None, False), ('', False), ('  ', False),
                                         (0, False), ('Voice chat', True)]:
                with self.subTest(description=description):
                    info = {} if description is None else {
                        'NSMicrophoneUsageDescription': description}
                    path.write_bytes(plistlib.dumps(info))
                    result = subprocess.run(
                        [sys.executable, '-c', preflight, folder, folder],
                        capture_output=True, text=True, check=False)
                    self.assertEqual(result.returncode, 0 if allowed else 1,
                                     result.stderr)
                    if not allowed:
                        self.assertIn('microphone permission description', result.stderr)


if __name__ == '__main__':
    unittest.main()
