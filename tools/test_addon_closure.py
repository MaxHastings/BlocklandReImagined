"""Dependency closure over a synthetic weapons Add-On; no content needed."""
import json
import tempfile
import unittest
from pathlib import Path

import addon_closure as closure

NS = "weapon_fw"


def fixture():
    """Two guns sharing a projectile, plus a launcher with its own effects."""
    weapons = {
        "schema_version": 1, "id": NS,
        "items": {
            f"{NS}:weapon/smg": {"id": f"{NS}:weapon/smg", "ui_name": "SMG", "image": f"{NS}:image/smg"},
            f"{NS}:weapon/pistol": {"id": f"{NS}:weapon/pistol", "ui_name": "Pistol", "image": f"{NS}:image/pistol"},
            f"{NS}:weapon/launcher": {"id": f"{NS}:weapon/launcher", "ui_name": "Launcher", "image": f"{NS}:image/launcher"},
        },
        "images": {
            f"{NS}:image/smg": {"model": "add-ons/weapon_fw/shapes/smg.dts", "projectile": f"{NS}:projectile/bullet",
                                "sound": f"{NS}:sound/smgloop", "states": []},
            f"{NS}:image/pistol": {"model": "add-ons/weapon_fw/shapes/pistol.dts", "projectile": f"{NS}:projectile/bullet",
                                   "reload": f"{NS}:image/pistolreload", "states": []},
            f"{NS}:image/pistolreload": {"model": "add-ons/weapon_fw/shapes/pistol.dts", "states": []},
            f"{NS}:image/launcher": {"model": "add-ons/weapon_fw/shapes/launcher.dts",
                                     "projectile": f"{NS}:projectile/rocket", "states": []},
        },
        "projectiles": {
            f"{NS}:projectile/bullet": {"damage_type": f"$DamageType::FwDamage", "model": "", "explosion": {"effect": None}},
            f"{NS}:projectile/rocket": {"damage_type": f"$DamageType::FwBlast", "model": "add-ons/weapon_fw/shapes/rocket.dts",
                                        "explosion": {"effect": "FwBoom"}},
        },
        "damage_types": {"fwdamage": {"name": "FwDamage"}, "fwblast": {"name": "FwBlast"}},
        "explosions": {"fwboom": {"name": "FwBoom"}},
        "sounds": {
            f"{NS}:sound/smgloop": {"file": "sounds/smg.wav"},
            f"{NS}:sound/boom": {"file": "sounds/boom.wav"},
        },
        "effects": {
            "particles": [{"id": f"{NS}:particle/smoke", "texture": "add-ons/weapon_fw/smoke.png"},
                          {"id": f"{NS}:particle/spark", "texture": "add-ons/weapon_fw/spark.png"}],
            "emitters": [{"id": f"{NS}:emitter/smoke", "particles": [f"{NS}:particle/smoke"]},
                         {"id": f"{NS}:emitter/spark", "particles": [f"{NS}:particle/spark"]}],
            "lights": [],
            "explosions": [{"id": f"{NS}:explosion/fwboom", "lifetime": 1.0,
                            "emitters": [f"{NS}:emitter/spark"], "sound": f"{NS}:sound/boom"}],
        },
        "definitions": [
            {"name": "FwBoom", "class": "ExplosionData", "parent": "FwBase", "fields": {"sound": "FwBoomSound"}},
            {"name": "FwBase", "class": "ExplosionData", "fields": {}},
            {"name": "FwUnused", "class": "ExplosionData", "fields": {}},
        ],
        "resources": [
            {"path": "Add-Ons/Weapon_Fw/shapes/smg.dts", "native_file": "models/smg.json"},
            {"path": "Add-Ons/Weapon_Fw/shapes/launcher.dts", "native_file": "models/launcher.json"},
            {"path": "Add-Ons/Weapon_Fw/smoke.png", "native_file": "textures/smoke.png"},
        ],
    }
    # The explosion also needs a sound the Add-On does not ship.
    weapons["effects"]["explosions"][0]["missing"] = f"{NS}:sound/absent"
    presentation = {
        "schema_version": 1, "id": NS, "weapons_sha256": "", "item_physics_sha256": "",
        "models": {
            "add-ons/weapon_fw/shapes/smg.dts": {"file": "models/smg.json", "textures": ["add-ons/weapon_fw/smg.png"]},
            "add-ons/weapon_fw/shapes/pistol.dts": {"file": "models/pistol.json", "textures": ["add-ons/weapon_fw/pistol.png"]},
            "add-ons/weapon_fw/shapes/launcher.dts": {"file": "models/launcher.json", "textures": ["add-ons/weapon_fw/shared.png"]},
            "add-ons/weapon_fw/shapes/rocket.dts": {"file": "models/rocket.json", "textures": ["add-ons/weapon_fw/shared.png"]},
        },
        "textures": {
            "add-ons/weapon_fw/smg.png": {"file": "textures/smg.png"},
            "add-ons/weapon_fw/pistol.png": {"file": "textures/pistol.png"},
            "add-ons/weapon_fw/shared.png": {"file": "textures/shared.png"},
            "add-ons/weapon_fw/smoke.png": {"file": "textures/smoke.png"},
            "add-ons/weapon_fw/spark.png": {"file": "textures/spark.png"},
        },
        "items": {k: {"image": v["image"], "model": ""} for k, v in weapons["items"].items()},
        "images": {k: {"model": v["model"]} for k, v in weapons["images"].items()},
        "projectiles": {k: {"model": v["model"] or None} for k, v in weapons["projectiles"].items()},
    }
    physics = {"items": {k: {} for k in weapons["items"]}}
    return weapons, presentation, physics


def write_package(folder):
    weapons, presentation, physics = fixture()
    assets = Path(folder) / "assets"
    for name in ("models", "textures", "sounds"):
        (assets / name).mkdir(parents=True)
    files = ["models/smg.json", "models/pistol.json", "models/launcher.json", "models/rocket.json",
             "textures/smg.png", "textures/pistol.png", "textures/shared.png", "textures/smoke.png",
             "textures/spark.png", "sounds/smg.wav", "sounds/boom.wav"]
    for name in files:
        (assets / name).write_bytes(name.encode())
    closure.write(assets / "weapons.json", weapons)
    closure.write(assets / "presentation.json", presentation)
    closure.write(assets / "item-physics.json", physics)
    return assets


class Closure(unittest.TestCase):
    def select(self, *roots):
        weapons, presentation, physics = fixture()
        return closure.Closure(weapons, presentation, physics).compute(roots)

    def test_an_item_keeps_what_it_names_and_what_those_name(self):
        selection = self.select("Pistol")
        self.assertEqual(selection.kept("items"), {f"{NS}:weapon/pistol"})
        self.assertEqual(selection.kept("images"), {f"{NS}:image/pistol", f"{NS}:image/pistolreload"})
        self.assertEqual(selection.kept("projectiles"), {f"{NS}:projectile/bullet"})
        self.assertEqual(selection.kept("damage_types"), {"fwdamage"})
        self.assertEqual(selection.models, {"add-ons/weapon_fw/shapes/pistol.dts"})
        self.assertEqual(selection.textures, {"add-ons/weapon_fw/pistol.png"})
        self.assertEqual(selection.kept("sounds"), set())

    def test_a_second_weapon_shares_the_first_ones_framework_data(self):
        both = self.select("smg", "pistol")
        self.assertEqual(both.kept("projectiles"), {f"{NS}:projectile/bullet"})
        self.assertEqual(both.kept("sounds"), {f"{NS}:sound/smgloop"})
        self.assertEqual(len(both.models), 2)

    def test_effects_definitions_and_textures_come_along_by_reference(self):
        selection = self.select(f"{NS}:weapon/launcher")
        self.assertEqual(selection.kept("explosions"), {"fwboom"})
        self.assertEqual(selection.kept("fx.explosions"), {f"{NS}:explosion/fwboom"})
        self.assertEqual(selection.kept("fx.emitters"), {f"{NS}:emitter/spark"})
        self.assertEqual(selection.kept("fx.particles"), {f"{NS}:particle/spark"})
        self.assertEqual(selection.kept("sounds"), {f"{NS}:sound/boom"})
        self.assertEqual(selection.kept("definitions"), {"FwBoom", "FwBase"})
        self.assertIn("add-ons/weapon_fw/spark.png", selection.textures)
        self.assertNotIn("add-ons/weapon_fw/smoke.png", selection.textures)
        self.assertEqual(selection.models, {"add-ons/weapon_fw/shapes/launcher.dts",
                                            "add-ons/weapon_fw/shapes/rocket.dts"})

    def test_what_the_add_on_does_not_define_is_reported_not_guessed(self):
        report = self.select("launcher").report()
        self.assertEqual(list(report["dangling"]), [f"{NS}:sound/absent"])
        self.assertEqual(self.select("pistol").report()["dangling"], {})

    def test_an_unknown_item_is_an_error(self):
        weapons, presentation, _ = fixture()
        with self.assertRaises(KeyError):
            closure.Closure(weapons, presentation).compute(["nothing"])


class Prune(unittest.TestCase):
    def test_dry_run_changes_nothing(self):
        with tempfile.TemporaryDirectory() as folder:
            assets = write_package(folder)
            before = (assets / "weapons.json").read_bytes()
            report = closure.prune_package(folder, ["pistol"])
            self.assertEqual((assets / "weapons.json").read_bytes(), before)
            self.assertIn("textures/smg.png", report["files_removed"])
            self.assertTrue((assets / "textures/smg.png").is_file())

    def test_applying_cuts_records_files_and_hashes_to_the_subset(self):
        with tempfile.TemporaryDirectory() as folder:
            assets = write_package(folder)
            closure.prune_package(folder, ["pistol"], apply=True)
            weapons = closure.read(assets / "weapons.json")
            self.assertEqual(list(weapons["items"]), [f"{NS}:weapon/pistol"])
            self.assertEqual(weapons["resources"], [])
            self.assertEqual(weapons["effects"]["emitters"], [])
            presentation = closure.read(assets / "presentation.json")
            self.assertEqual(list(presentation["models"]), ["add-ons/weapon_fw/shapes/pistol.dts"])
            self.assertEqual(presentation["weapons_sha256"], closure.sha256_file(assets / "weapons.json"))
            self.assertEqual(presentation["item_physics_sha256"], closure.sha256_file(assets / "item-physics.json"))
            self.assertEqual(list(closure.read(assets / "item-physics.json")["items"]), [f"{NS}:weapon/pistol"])
            self.assertFalse((assets / "models/smg.json").exists())
            self.assertTrue((assets / "models/pistol.json").is_file())
            self.assertTrue((assets / "textures/pistol.png").is_file())

    def test_a_file_a_kept_record_also_uses_survives(self):
        with tempfile.TemporaryDirectory() as folder:
            assets = write_package(folder)
            closure.prune_package(folder, ["launcher"], apply=True)
            self.assertTrue((assets / "textures/shared.png").is_file())
            self.assertTrue((assets / "models/rocket.json").is_file())
            self.assertFalse((assets / "textures/pistol.png").exists())

    def test_pruning_is_repeatable(self):
        with tempfile.TemporaryDirectory() as folder:
            assets = write_package(folder)
            closure.prune_package(folder, ["smg", "pistol"], apply=True)
            first = {p.name: p.read_bytes() for p in assets.rglob("*") if p.is_file()}
            closure.prune_package(folder, ["smg", "pistol"], apply=True)
            second = {p.name: p.read_bytes() for p in assets.rglob("*") if p.is_file()}
            self.assertEqual(first, second)


if __name__ == "__main__":
    unittest.main()
