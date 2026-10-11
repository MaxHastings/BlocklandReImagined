"""Build a private native P90 Add-On from the owner's two original ZIPs.

No original assets or scripts belong in the repository. This recipe uses the
released importer, then installs only the P90 and its required Rallypack data.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import struct
import tempfile
from rallypack_bot_use import describe, state_ref

NS = "weapon_rallypack_p90"
RULES = NS + "-rules"
RECIPE = Path(__file__).resolve().parent / "rallypack_p90"


def read(path):
    return json.loads(path.read_text(encoding="utf-8"))


def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def remap(data):
    return json.loads(json.dumps(data).replace("weapon_rallypack:", NS + ":"))


def build(args):
    output = args.output.resolve()
    if output.exists():
        raise SystemExit(f"Output must be a fresh directory: {output}")
    output.mkdir(parents=True)
    with tempfile.TemporaryDirectory(prefix="bri-p90-") as temporary:
        work = Path(temporary)
        refs = work / "reference" / "Add-Ons"
        refs.mkdir(parents=True)
        shutil.copyfile(args.rallypack, refs / "Weapon_Rallypack.zip")
        for source, name in [(args.rallypack, "rally"), (args.p90, "p90")]:
            result = subprocess.run([
                str(args.importer), str(source), str(work / name),
                "--reference", str(refs.parent), "--installed", str(args.content),
            ], capture_output=True, text=True, encoding="utf-8", errors="replace")
            (output / (name + "-import.log")).write_text(result.stdout + result.stderr, encoding="utf-8")
            if result.returncode:
                raise SystemExit(f"Import failed; see {output / (name + '-import.log')}")
        p90 = work / "p90"
        rally = work / "rally"
        native = output / NS
        assets = native / "assets"
        shutil.copytree(p90, native)
        # The original reports describe the unported input, not this native result.
        for name in ["IMPORT-REPORT.md", "import-report.json"]:
            shutil.move(str(native / name), str(output / ("p90-source-" + name)))
        weapons = remap(read(assets / "weapons.json"))
        framework = remap(read(rally / "assets/weapons.json"))
        presentation = read(assets / "presentation.json")
        other_presentation = read(rally / "assets/presentation.json")
        projectile = NS + ":projectile/r_assaultrifleprojectile"
        recoil = NS + ":projectile/r_shotgunrecoilprojectile"
        weapons["projectiles"] = {key: framework["projectiles"][key] for key in [projectile, recoil]}
        weapons["damage_types"] = {key: value for key, value in framework["damage_types"].items()
                                   if key.lower() in ["rwepdamage", "rwepdamageh", "$damagetype::rwepdamage", "$damagetype::rwepdamageh"]}
        # Keep the original omitted projectile shape empty rather than inventing art.
        if not any(r["path"].lower() == weapons["projectiles"][projectile]["model"].lower()
                   for r in framework["resources"]):
            weapons["projectiles"][projectile]["model"] = ""
        selected_sounds = [NS + ":sound/" + name for name in [
            "r_submachinereload1sound", "r_submachinereload2sound", "r_submachinereload3sound", "rdistsound_pistol"]]
        for key in selected_sounds:
            sound = framework["sounds"][key]
            weapons["sounds"][key] = sound
            dest = assets / sound["file"]
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(rally / "assets" / sound["file"], dest)
        needed_defs = {"r_assaultrifleshelldebris", "r_assaultrifleprojectile", "r_shotgunrecoilprojectile"}
        weapons["definitions"] += [d for d in framework["definitions"] if d["name"].lower() in needed_defs]
        wanted_emitters = {NS + ":emitter/r_submachinegunflashemitter", NS + ":emitter/r_submachinegunsmokeemitter"}
        emitters = [e for e in framework["effects"]["emitters"] if e["id"] in wanted_emitters]
        wanted_particles = {p for e in emitters for p in e["particles"]}
        effects = {"particles": [p for p in framework["effects"]["particles"] if p["id"] in wanted_particles],
                   "emitters": emitters, "lights": [],
                   "explosions": [e for e in framework["effects"]["explosions"] if "recoil" in e["id"].lower()]}
        weapons["effects"] = effects
        weapons["explosions"] = {k: v for k, v in framework["explosions"].items() if "recoil" in k.lower()}
        model_key = "add-ons/weapon_rallypack/shapes/debris/shell_assaultrifle.dts"
        model = other_presentation["models"][model_key]
        presentation["models"][model_key] = model
        required_files = {model["file"]}
        for texture in model["textures"]:
            if texture in other_presentation["textures"]:
                entry = other_presentation["textures"][texture]
                presentation["textures"][texture] = entry
                required_files.add(entry["file"])
        for file in required_files:
            dest = assets / file
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(rally / "assets" / file, dest)
        weapons["resources"] += [r for r in framework["resources"] if r["native_file"] in required_files]
        # These exported solid weapon surfaces import as additive: black then
        # contributes zero light and vanishes. Use ordinary v20 decal surfaces.
        # Resolve material names the generic importer left as clear placeholders.
        for model in presentation["models"].values():
            shape_path = assets / model["file"]
            shape = read(shape_path)
            for index, material in enumerate(shape["materials"]):
                material["blend"] = "opaque"
                if model["textures"][index] != "placeholder:clear":
                    continue
                name = material["name"].split(".")[0].lower()
                source = args.vanilla / "base/data/shapes" / (name + ".png")
                if not source.is_file():
                    raise SystemExit(f"Required original stock texture missing: {source}")
                data = source.read_bytes()
                digest = hashlib.sha256(data).hexdigest()
                file = "textures/" + digest[:24] + ".png"
                (assets / file).write_bytes(data)
                texture = "base/data/shapes/" + name + ".png"
                width, height = struct.unpack(">II", data[16:24])
                presentation["textures"][texture] = dict(file=file, sha256=digest,
                    source=texture, width=width, height=height)
                model["textures"][index] = texture
            write(shape_path, shape)
            model["sha256"] = hashlib.sha256(shape_path.read_bytes()).hexdigest()
        for key, image in weapons["images"].items():
            # Firing is authoritative package policy, including the authored spray pattern.
            describe(image, projectile, {"package": RULES, "loaded": state_ref("rounds"),
                                         "spare": [state_ref("spares")]})
            image["command"] = RULES + ":shot"
            image["commands"] = {"mount": RULES + ":mount", "unmount": RULES + ":unmount",
                                 "light": RULES + ":reload", "jet": RULES + ":aim",
                                 "states": {"onready": RULES + ":ready", "onpreammocheck": RULES + ":check",
                                            "onreloadfinish": RULES + ":finish", "ondryfire": RULES + ":dry"}}
            for state in image["states"]:
                if state["name"].lower() == "ready":
                    # Do not repeatedly enter the dry/reload cycle while idle.
                    state["no_ammo"] = None
                if state["name"] == "PreAmmoCheck":
                    # Rhai effects commit after this weapon step, unlike synchronous Torque callbacks.
                    state["ticks"] = max(1, state["ticks"])
                if state["name"] in ["Fire", "Smoke", "Reload", "PreAmmoCheck", "AmmoCheck"] and "reloading" not in key:
                    state["sound"] = NS + ":sound/r_p90soundloop"
                if state["script"] == "onMagOut":
                    state["arm"], state["gesture"] = "wrench", "shiftTo"
                if state["script"] == "onMagIn":
                    state["arm"], state["gesture"] = "shiftLeft", "plant"
                if state["script"] == "onCharge":
                    state["arm"] = "shiftRight"
            if "aiming" in key:
                image["source_rotation_degrees"] = [0.0, -20.0, 0.0]
                presentation["images"][key]["source_rotation_degrees"] = [0.0, -20.0, 0.0]
                presentation["images"][key]["eye_rotation_degrees"] = [0.0, -20.0, 0.0]
        weapons["items"][NS + ":weapon/r_p90item"]["ui_name"] = "P90 (Rallypack)"
        weapons["diagnostics"] = []
        weapons.pop("external_projectiles", None)
        write(assets / "weapons.json", weapons)
        presentation["weapons_sha256"] = hashlib.sha256((assets / "weapons.json").read_bytes()).hexdigest()
        write(assets / "presentation.json", presentation)
        manifest = read(native / "package.json")
        manifest.update(name="Rallypack P90", version="0.1.0", dependencies={}, companions=[RULES],
                        description="Native P90 with its original Rallypack assets and host weapon rules. For v0.2.8.")
        manifest["provenance"]["rallypack_sha256"] = hashlib.sha256(args.rallypack.read_bytes()).hexdigest()
        write(native / "package.json", manifest)
        rules = output / RULES
        shutil.copytree(RECIPE, rules)
        write(rules / "package.json", {
            "schema_version": 1, "api": 1, "id": RULES, "version": "0.1.0", "name": "Rallypack P90 Rules",
            "description": "Authoritative P90 ammo, reload, recoil and aiming; enabled with Rallypack P90.",
            "authors": [], "license": "proprietary", "dependencies": {NS: "*"},
            "capabilities": ["player", "damage", "effects", "chat"],
            "provides": [{"kind": "behaviour", "id": RULES + ":behaviour/main", "file": "behaviour.json"},
                         {"kind": "script", "id": RULES + ":script/main", "file": "p90.rhai"}]
        })
        write(output / "BUILD.json", {"target_game": "v0.2.8", "p90_sha256": hashlib.sha256(args.p90.read_bytes()).hexdigest(),
                                     "rallypack_sha256": hashlib.sha256(args.rallypack.read_bytes()).hexdigest()})
    print(f"Built {output}; enable {NS}, which enables its host rules companion.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ["p90", "rallypack", "importer", "content", "vanilla", "output"]:
        parser.add_argument("--" + key, type=Path, required=True)
    build(parser.parse_args())
