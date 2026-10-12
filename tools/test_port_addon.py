"""The one-step port workflow, with stand-in importer and checker programs; no content needed."""
import json
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

import addon_closure
import port_addon
import test_addon_closure as fixtures

IMPORTER = r'''
import json, sys
from pathlib import Path
source, out = Path(sys.argv[1]), Path(sys.argv[2])
ident = source.stem.lower()
(out / "assets").mkdir(parents=True)
(out / "package.json").write_text(json.dumps({
    "id": ident, "version": "1.2.0", "name": source.stem,
    "provides": [{"kind": "weapons", "id": ident + ":weapons/main", "file": "assets/weapons.json"}],
}))
(out / "IMPORT-REPORT.md").write_text("report")
(out / "import-report.json").write_text("{}")
(out / "assets" / "weapons.json").write_text("{}")
'''

CHECKER = r'''
import json, sys
from pathlib import Path
folder = Path(sys.argv[1])
broken = (folder / "BROKEN").exists()
print(json.dumps({"ok": not broken, "add_ons": [], "diagnostics": (
    [{"severity": "error", "code": "script.unknown_function", "message": "no such function"}] if broken else
    [{"severity": "warning", "code": "state.zero_time_cycle", "message": "loops"}] if (folder / "WARN").exists() else [])}))
sys.exit(1 if broken else 0)
'''


class Workflow(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.addCleanup(self.temp.cleanup)
        (self.root / "importer.py").write_text(IMPORTER)
        (self.root / "checker.py").write_text(CHECKER)
        self.tools = port_addon.Tools([sys.executable, str(self.root / "importer.py")],
                                      [sys.executable, str(self.root / "checker.py")])
        self.source = self.root / "Weapon_Demo.zip"
        with zipfile.ZipFile(self.source, "w") as archive:
            archive.writestr("Weapon_Demo/server.cs", "// original")

    def args(self, out="out", **extra):
        values = dict(sources=[self.source], needs=[], out=self.root / out, recipe=None, only=None,
                      install=None, target_game="v9.9.9")
        values.update(extra)
        return type("Args", (), values)

    def build(self, **extra):
        return port_addon.build_command(self.args(**extra), self.tools)

    def test_a_build_checks_packages_and_leaves_a_receipt(self):
        out, receipt, _ = self.build()
        zips = sorted(out.glob("*.zip"))
        self.assertEqual([z.name for z in zips], ["weapon_demo-1.2.0-for-v9.9.9.zip"])
        self.assertEqual(receipt["target_game"], "v9.9.9")
        self.assertEqual(receipt["recipe"], "plain")
        self.assertEqual(receipt["stages"]["package"]["sha256"], port_addon.sha256(zips[0]))
        self.assertEqual(receipt["sources"], {"Weapon_Demo.zip": port_addon.sha256(self.source)})
        self.assertEqual(json.loads((out / port_addon.RECEIPT).read_text()), receipt)
        with zipfile.ZipFile(zips[0]) as archive:
            names = archive.namelist()
        self.assertIn("weapon_demo/package.json", names)
        self.assertFalse([n for n in names if "IMPORT-REPORT" in n and not n.startswith("weapon_demo/")])

    def test_two_builds_make_the_same_zip(self):
        first, _, _ = self.build(out="a")
        second, _, _ = self.build(out="b")
        self.assertEqual(port_addon.sha256(next(first.glob("*.zip"))), port_addon.sha256(next(second.glob("*.zip"))))

    def test_a_checker_error_stops_the_build_before_any_zip(self):
        (self.root / "importer.py").write_text(IMPORTER + '\n(out / "BROKEN").write_text("x")\n')
        with self.assertRaises(port_addon.Failed) as caught:
            self.build()
        self.assertIn("[verify]", str(caught.exception))
        self.assertIn("script.unknown_function", str(caught.exception))
        self.assertEqual(list((self.root / "out").glob("*.zip")), [])

    def test_checker_warnings_reach_the_receipt(self):
        (self.root / "importer.py").write_text(IMPORTER + '\n(out / "WARN").write_text("x")\n')
        _, receipt, _ = self.build()
        warnings = receipt["stages"]["verify"]["add_ons"]["weapon_demo"]["warnings"]
        self.assertEqual(warnings[0]["code"], "state.zero_time_cycle")
        self.assertEqual(receipt["stages"]["verify"]["add_ons"]["weapon_demo"]["unreferenced_assets"], [])

    def test_a_hash_edited_after_recording_is_caught(self):
        (self.root / "importer.py").write_text(IMPORTER + '''
(out / "assets" / "presentation.json").write_text(json.dumps({"weapons_sha256": "00", "models": {}, "textures": {}}))
''')
        with self.assertRaises(port_addon.Failed) as caught:
            self.build()
        self.assertIn("weapons_sha256", str(caught.exception))

    def test_unreferenced_files_are_reported_not_deleted(self):
        (self.root / "importer.py").write_text(IMPORTER + '''
(out / "assets" / "debug.wav").write_bytes(b"x")
''')
        _, receipt, _ = self.build()
        self.assertEqual(receipt["stages"]["verify"]["add_ons"]["weapon_demo"]["unreferenced_assets"], ["debug.wav"])

    def test_it_refuses_a_folder_that_is_not_empty_and_changes_no_source(self):
        (self.root / "used").mkdir()
        (self.root / "used" / "x").write_text("x")
        with self.assertRaises(port_addon.Failed):
            self.build(out="used")
        self.build()
        self.assertEqual(port_addon.sha256(self.source), port_addon.sha256(self.source))

    def test_only_cuts_the_import_to_the_items_and_reports_the_rest(self):
        weapons, presentation, physics = fixtures.fixture()
        importer = IMPORTER + f'''
import sys
sys.path.insert(0, {str(Path(__file__).parent)!r})
import test_addon_closure
test_addon_closure.write_package(out)
'''
        (self.root / "importer.py").write_text(importer)
        out, receipt, _ = self.build(only=["pistol"])
        closure = receipt["stages"]["closure"]
        self.assertEqual(closure["items"], [f"{fixtures.NS}:weapon/pistol"])
        self.assertIn("models/smg.json", closure["files_removed"])
        with zipfile.ZipFile(next(out.glob("*.zip"))) as archive:
            self.assertNotIn("weapon_demo/assets/models/smg.json", archive.namelist())
            self.assertIn("weapon_demo/assets/models/pistol.json", archive.namelist())

    def test_an_unknown_item_names_itself(self):
        weapons, presentation, physics = fixtures.fixture()
        (self.root / "importer.py").write_text(IMPORTER + f'''
import sys
sys.path.insert(0, {str(Path(__file__).parent)!r})
import test_addon_closure
test_addon_closure.write_package(out)
''')
        with self.assertRaises(port_addon.Failed) as caught:
            self.build(only=["nothing"])
        self.assertIn("nothing", str(caught.exception))


class Install(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.addCleanup(self.temp.cleanup)
        self.content = self.root / "content"
        self.content.mkdir()
        (self.content / "packages.json").write_text('{"enabled": ["x"]}')

    def make_zip(self, name="a.zip", entries=None):
        entries = entries if entries is not None else {
            "weapon_a/package.json": json.dumps({"id": "weapon_a", "version": "1.0.0"}),
            "weapon_a-rules/package.json": json.dumps({"id": "weapon_a-rules", "version": "1.0.0"}),
            "weapon_a/assets/weapons.json": "{}",
        }
        path = self.root / name
        with zipfile.ZipFile(path, "w") as archive:
            for key, value in entries.items():
                archive.writestr(key, value)
        return path

    def test_it_installs_each_folder_and_leaves_the_enabled_list_alone(self):
        installed = port_addon.install_zip(self.make_zip(), self.content)
        self.assertEqual({i["folder"] for i in installed}, {"weapon_a", "weapon_a-rules"})
        self.assertTrue((self.content / "addons/weapon_a/assets/weapons.json").is_file())
        self.assertEqual((self.content / "packages.json").read_text(), '{"enabled": ["x"]}')
        self.assertEqual([p.name for p in (self.content / "addons").iterdir()], sorted(["weapon_a", "weapon_a-rules"]))

    def test_an_update_replaces_the_folder_whole(self):
        port_addon.install_zip(self.make_zip(), self.content)
        (self.content / "addons/weapon_a/stale.txt").write_text("old")
        installed = port_addon.install_zip(self.make_zip("b.zip"), self.content)
        self.assertTrue(all(i["replaced"] for i in installed))
        self.assertFalse((self.content / "addons/weapon_a/stale.txt").exists())

    def test_a_path_that_escapes_is_refused_and_nothing_is_installed(self):
        bad = self.make_zip("bad.zip", {"weapon_a/package.json": "{}", "../outside.txt": "x"})
        with self.assertRaises(port_addon.Failed):
            port_addon.install_zip(bad, self.content)
        self.assertFalse((self.root / "outside.txt").exists())
        self.assertEqual(list((self.content / "addons").iterdir()), [])

    def test_a_zip_with_no_add_on_or_stray_files_is_refused(self):
        with self.assertRaises(port_addon.Failed):
            port_addon.install_zip(self.make_zip("none.zip", {"readme.txt": "hi"}), self.content)
        with self.assertRaises(port_addon.Failed):
            port_addon.install_zip(self.make_zip("stray.zip", {"weapon_a/package.json": "{}", "notes.txt": "x"}), self.content)

    def test_one_identity_cannot_live_in_two_folders(self):
        port_addon.install_zip(self.make_zip(), self.content)
        clash = self.make_zip("clash.zip", {"other_folder/package.json": json.dumps({"id": "weapon_a"})})
        with self.assertRaises(port_addon.Failed) as caught:
            port_addon.install_zip(clash, self.content)
        self.assertIn("weapon_a", str(caught.exception))
        self.assertFalse((self.content / "addons/other_folder").exists())


if __name__ == "__main__":
    unittest.main()
