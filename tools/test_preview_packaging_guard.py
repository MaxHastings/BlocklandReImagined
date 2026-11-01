"""Private-preview verification must never claim a release package."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


@unittest.skipUnless(os.name == 'nt' and shutil.which('pwsh'), 'requires PowerShell on Windows')
class PreviewGuardTests(unittest.TestCase):
    def test_manifest_marker_and_version_are_required(self):
        script = Path(__file__).with_name('package_playtest.ps1')
        with tempfile.TemporaryDirectory() as folder:
            manifest = Path(folder) / 'MANIFEST.json'
            def verify(data, *flags):
                manifest.write_text(json.dumps(data), encoding='utf-8')
                return subprocess.run(['pwsh', '-NoProfile', '-File', str(script),
                                       '-VerifyPackage', folder, *flags], capture_output=True, text=True)
            base = {'schema_version': 1, 'version': 'v0.4.3', 'files': [{'path': 'unused'}],
                    'previous_imports_for_private_preview': True}
            result = verify(base)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('not a release-verified', result.stderr)
            result = verify(base, '-SkipArchive', '-PreviousImportsForPrivatePreview',
                            '-Version', 'v0.4.3-preview.3')
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('matching preview version', result.stderr)
            base['version'] = 'v0.4.3-preview.3'
            del base['previous_imports_for_private_preview']
            result = verify(base, '-SkipArchive', '-PreviousImportsForPrivatePreview',
                            '-Version', 'v0.4.3-preview.3')
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('manifest marker', result.stderr)


if __name__ == '__main__':
    unittest.main()
