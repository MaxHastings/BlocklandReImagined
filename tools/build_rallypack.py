"""Package privately owned Rallypack assets and native host policy for v0.2.8."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import struct
import subprocess
import zipfile
from rallypack_bot_use import describe

NS = "weapon_rallypack"
RULES = NS + "-rules"


def read(path):
    return json.loads(path.read_text(encoding="utf-8"))


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def clean(value):
    return str(value).strip().strip('"')


def number(fields, key, default):
    try:
        return float(clean(fields.get(key, default)))
    except ValueError:
        return float(default)


def yes(fields, key):
    return clean(fields.get(key, "")).lower() in ("true", "1")


def literal(value):
    if isinstance(value, dict):
        return "#{" + ",".join(json.dumps(k) + ":" + literal(v) for k, v in value.items()) + "}"
    if isinstance(value, list):
        return "[" + ",".join(map(literal, value)) + "]"
    return json.dumps(value)


def build(args):
    out = args.output.resolve()
    if out.exists():
        raise SystemExit(f"Fresh output required: {out}")
    out.mkdir(parents=True)
    native = out / NS
    result = subprocess.run([str(args.importer), str(args.source), str(native),
        "--reference", str(args.vanilla), "--installed", str(args.content)],
        capture_output=True, text=True, encoding="utf-8", errors="replace")
    (out / "import.log").write_text(result.stdout + result.stderr, encoding="utf-8")
    if result.returncode:
        raise SystemExit("Import failed: " + str(out / "import.log"))
    for name in ("IMPORT-REPORT.md", "import-report.json"):
        shutil.move(str(native / name), str(out / ("source-" + name)))
    assets = native / "assets"
    weapons = read(assets / "weapons.json")
    presentation = read(assets / "presentation.json")
    definitions = {d["name"].lower(): d for d in weapons["definitions"]}

    def fields(name):
        d = definitions.get(name.lower(), {})
        parent = d.get("parent") or ""
        f = fields(parent) if parent.lower() in definitions else {}
        f.update(d.get("fields", {}))
        return f

    def image_ref(name):
        value = NS + ":image/" + clean(name).lower()
        return value if value in weapons["images"] else ""

    def projectile_ref(name):
        value = NS + ":projectile/" + clean(name).lower()
        return value if value in weapons["projectiles"] else ""

    def sound_ref(name):
        value = NS + ":sound/" + clean(name).lower()
        return value if value in weapons["sounds"] else ""

    with zipfile.ZipFile(args.source) as archive:
        files = {name.lower(): name for name in archive.namelist()}
        # Solid gun surfaces were exported with contradictory blend flags.
        # Keep genuine translucent effects; only weapon/debris shapes get this override.
        item_models = {i["model"].lower() for i in weapons["items"].values()}
        item_models.update(i["model"].lower() for i in weapons["images"].values())
        item_models.update(clean(d["fields"].get("shapefile", "")).lower().replace("./", "add-ons/weapon_rallypack/", 1)
            for d in weapons["definitions"] if d["class"].lower() == "debrisdata")
        item_models.update(p["model"].lower() for p in weapons["projectiles"].values()
            if "recoil" not in p["name"].lower())
        item_models.add("add-ons/weapon_rallypack/shapes/shield/shieldplayer.dts")
        unresolved = []
        for key, model in presentation["models"].items():
            path = assets / model["file"]
            shape = read(path)
            for index, material in enumerate(shape["materials"]):
                if key in item_models:
                    material["blend"] = "opaque"
                if model["textures"][index] != "placeholder:clear":
                    continue
                name = material["name"].split(".")[0].lower()
                relative = model["source"].replace("\\", "/").lower().split("weapon_rallypack/", 1)[-1]
                own = [(str(Path(relative).parent / (name + ".png"))).replace("\\", "/"), name + ".png"]
                found = next((files[c] for c in own if c in files), None)
                stock = args.vanilla / "base/data/shapes" / (name + ".png")
                if found:
                    data = archive.read(found)
                    texture = "add-ons/weapon_rallypack/" + found.lower()
                elif stock.is_file():
                    data = stock.read_bytes()
                    texture = "base/data/shapes/" + name + ".png"
                else:
                    if "/sese/" not in key:
                        unresolved.append(f"{key}: {material['name']}")
                    continue
                digest = hashlib.sha256(data).hexdigest()
                file = "textures/" + digest[:24] + ".png"
                (assets / file).write_bytes(data)
                width, height = struct.unpack(">II", data[16:24])
                presentation["textures"][texture] = dict(file=file, sha256=digest,
                    source=texture, width=width, height=height)
                model["textures"][index] = texture
            write(path, shape)
            model["sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()

    # The importer sees every definition, including the disabled debug script.
    weapons["items"].pop(NS + ":weapon/seseitem", None)
    presentation["items"].pop(NS + ":weapon/seseitem", None)
    physics = read(assets / "item-physics.json")
    physics["items"].pop(NS + ":weapon/seseitem", None)
    write(assets / "item-physics.json", physics)
    presentation["item_physics_sha256"] = hashlib.sha256((assets / "item-physics.json").read_bytes()).hexdigest()
    for key in list(weapons["images"]):
        if ":image/sese" in key:
            weapons["images"].pop(key)
            presentation["images"].pop(key, None)
    configs = {}
    banks = {}
    for key, image in weapons["images"].items():
        f = fields(image["name"])
        base = image_ref(f.get("r_inheritimage", image["name"])) or key
        root = fields(weapons["images"][base]["name"])
        size = int(number(root, "r_magsize", 1))
        kind = "magazine" if yes(root, "r_usesmagazines") else "reserve"
        if "shotgun" in image["name"].lower():
            category = "shotgun"
        elif yes(f, "r_sniper_trackaim"):
            category = "sniper"
        elif "bow" in image["name"].lower():
            category = "bow"
        elif "m67" in image["name"].lower():
            category = "grenade"
        else:
            category = "gun"
        ammo = clean(root.get("r_ammoname", weapons["images"][base]["name"]))
        bank = "ammo_" + re.sub(r"[^a-z0-9_]", "_", ammo.lower())
        plus = not yes(root, "r_disableplusone") and kind == "magazine"
        projectile = image.get("projectile") or projectile_ref(f.get("projectile", ""))
        if category == "grenade":
            projectile = projectile_ref("R_M67Projectile")
        if category == "bow":
            projectile = projectile_ref("R_arrowProjectile1")
        # Source launcher images dispatch a special projectile from their function.
        if "launcher" in image["name"].lower() and "rocket" not in image["name"].lower():
            if "grenadier" in image["name"].lower():
                projectile = projectile_ref("R_AKgrenadierProjectile" if "ak47" in image["name"].lower() else "R_GrenadierProjectile") or projectile
        if "rocketlauncher" in image["name"].lower():
            projectile = projectile_ref("HATLauncherProjectile" if "opfor" not in image["name"].lower() else "HATFORProjectile") or projectile
        mag = kind == "magazine"
        reserve = 32
        if "rocket" in ammo.lower():
            reserve = 1
        if "grenade" in ammo.lower():
            reserve = 6
        banks.setdefault(bank, dict(n=size, mags=[size, size] if mag else [], reserve=reserve))
        loop_sound = sound_ref(f.get("r_weaponloopsound", ""))
        tail_sound = loop_sound.replace("soundloop", "soundtail")
        if tail_sound not in weapons["sounds"]:
            tail_sound = ""
        cfg = dict(base=base, bank=bank, size=size, kind=kind, category=category,
            reload=image_ref(root.get("r_reloadimage", "")),
            ads=image_ref(f.get("r_adsimage", root.get("r_adsimage", ""))),
            projectile=projectile or "", speed=float(weapons["projectiles"].get(projectile, {}).get("speed", 100.0)),
            plus=plus, reloading=("reload" in image["name"].lower()),
            aimed=any(s in image["name"].lower() for s in ("aim", "scope", "ads")),
            movement=number(f, "r_movepenalty", 0.6 if any(s in image["name"].lower() for s in ("aim", "scope", "ads")) else 1.0),
            cm=number(f, "climbmodifier", 0.85), bm=number(f, "boundmodifier", 0.6),
            seed=[int(x) for x in str(abs(int(number(f, "sprayseed", 433987))))],
            pellets=int(number(f, "shotnum", 9)) if category == "shotgun" else 1,
            spread=number(f, "shotspread", 0.05) if category == "shotgun" else number(f, "r_sniper_basespread", 0.0),
            loop_sound=loop_sound, tail=tail_sound,
            far=sound_ref(f.get("r_farshotsound", "")))
        configs[key] = cfg
        image["projectile"] = None
        # The command intercepts native firing; this reference describes the
        # actual package-launched shot for the existing bot executor.
        if projectile and category not in ("bow", "grenade"):
            describe(image, projectile)
        image["command"] = RULES + ":shot"
        commands = {"mount": RULES + ":mount", "unmount": RULES + ":unmount",
            "light": RULES + ":reload", "jet": RULES + ":aim", "states": {}}
        callbacks = {
            "onready": "ready", "onstopfire": "ready", "onpreammocheck": "check", "preammocheck": "check",
            "prereserveammocheck": "reservecheck", "checkreserveammo": "reservecheck", "prehasrocket": "check",
            "onreloadfinish": "finish", "finishreload": "finish", "onchangeimage": "finish",
            "finishgrenadereload": "finish", "onloadone": "loadone", "loadone": "loadone",
            "oninsertrocket": "loadone", "m203insertgrenade": "loadone", "onreload": "inline_reload",
            "onprecheck1": "fullcheck", "onprecheckfirst": "boltcheck", "onprecheck2": "chambercheck",
            "ondryfire": "dry", "onreleasepinfinished": "prime", "onthrow": "shot", "ondetonate": "detonate",
            "startcook": "cook", "ondropnotif": "supply", "onrecharge": "charge", "oncharge": "charge",
            "oncylempty": "empty", "onloadstripper": "stripper"}
        for state in image["states"]:
            if state["name"].lower() == "ready":
                state["no_ammo"] = None
            script = state["script"].lower()
            if script in callbacks:
                commands["states"][script] = RULES + ":" + callbacks[script]
            if "check" in script:
                state["ticks"] = max(1, state["ticks"])
            if loop_sound and not cfg["reloading"] and state["name"].lower() in (
                    "fire", "smoke", "reload", "preammocheck", "ammocheck"):
                state["sound"] = loop_sound
            action = {"onmagout": ("wrench", "shiftTo"), "onmagin": ("shiftLeft", "plant"),
                "oncharge": ("shiftRight", "plant"), "onrackbolt": ("shiftRight", "plant"),
                "onclosebolt": ("", "plant"), "opencover": ("shiftLeft", "plant"),
                "placebelt": ("shiftLeft", "plant"), "placemag": ("shiftLeft", "plant"),
                "rack": ("shiftRight", "plant")}.get(script)
            if action:
                state["arm"], state["gesture"] = action
        image["commands"] = commands
        if "ADSRollVector" in f.get("rotation", ""):
            image["source_rotation_degrees"] = [0.0, -20.0, 0.0]
            presentation["images"][key]["source_rotation_degrees"] = [0.0, -20.0, 0.0]
        if key in presentation["images"]:
            # Scope images use native zoom with their original eye offset/model.
            if "scope" in image["name"].lower():
                image["crosshair"] = False
        if len(commands["states"]) > 16:
            raise SystemExit("Too many callbacks: " + key)

    # Native held secondary image replaces the source's invisible AIPlayer
    # mounting wrapper; use that wrapper's authored mount0 offset.
    shield_id = NS + ":image/native_shield"
    blank = weapons["images"][NS + ":image/r_shielddropimagenooffset"]
    blank["commands"] = {}
    blank["command"] = None
    blank["states"] = [dict(name="Ready", ticks=0, allow_change=True, sequence="")]
    shield_model = "add-ons/weapon_rallypack/shapes/shield/shieldplayer.dts"
    weapons["images"][shield_id] = dict(id=shield_id, name="NativeRallyShield", model=shield_model,
        mount_point=1, offset=[0.38822001218795776,-1.0914299488067627,-0.2697409987449646],
        eye_offset=[-0.5,-0.9,-0.4], color=[1.0]*4, color_shift=False, arm_ready=False,
        states=[dict(name="Ready", ticks=0, allow_change=True, sequence="")])
    presentation["images"][shield_id] = dict(model=shield_model, mount_point=1,
        offset=weapons["images"][shield_id]["offset"],eye_offset=[-0.5,-0.9,-0.4],
        source_rotation_degrees=[0.0]*3, eye_rotation_degrees=[0.0]*3, tint=[1.0]*4,
        evidence=dict(path="native/build_rallypack.py", line=1,
            sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest()))
    # This source armour effect uses a base sound outside the weapons pack.
    source_sound = args.vanilla / "base/data/sound/breakBrick.wav"
    sound_id = NS + ":sound/native_brickbreak"
    data = source_sound.read_bytes()
    sound_file = "sounds/" + hashlib.sha256(data).hexdigest()[:24] + ".wav"
    (assets / sound_file).write_bytes(data)
    weapons["sounds"][sound_id] = dict(file=sound_file,volume=1.0,looping=False,local=False)
    for explosion in weapons["effects"]["explosions"]:
        if explosion.get("sound", "").lower() == "brickbreaksound":
            explosion["sound"] = sound_id
    for explosion in weapons["explosions"].values():
        if explosion.get("sound", "").lower() == "brickbreaksound":
            explosion["sound"] = sound_id
    # v0.2.8 also discovers body art from this directory independently of
    # manifest provides. These replaced wrappers must not load as player art.
    for path in (assets / "archetypes").glob("*.json"):
        archetype = read(path)
        archetype.pop("model", None)
        write(path, archetype)

    # Ordinary bullets are deliberately model-less in the original. Its old
    # missing root bullet.dts reference is not a visible stand-in weapon.
    for key, projectile in weapons["projectiles"].items():
        f = fields(projectile["name"])
        if projectile["explosion"]["effect"] and "recoil" not in key and (
                "grenadier" in key or "m67" in key or "hatlauncherprojectile" in key or "hatforprojectile" in key
                or "oicw" in key):
            # The source calculates blast damage in rex_doExplosion, outside
            # the ordinary datablock. Translate it to the native blast primitive.
            explosion_name = clean(f.get("explosion", ""))
            ef = fields(explosion_name)
            radius = number(f, "rex_radius", number(ef, "camshakeradius", 30) * 0.8)
            projectile["explosion"]["radius"] = radius
            projectile["explosion"]["damage"] = number(f, "rex_damage", 130)
            if "hatlauncherprojectile" in key or "hatforprojectile" in key:
                projectile["arm_ticks"] = 36
        model = projectile["model"].lower()
        if model and model not in presentation["models"]:
            projectile["model"] = ""
        presentation["projectiles"][key] = dict(model=projectile["model"].lower() or None, tint=[1.0]*4)
    weapons["diagnostics"] = []
    weapons.pop("external_projectiles", None)
    # Independent ports can coexist without colliding on Torque's globals.
    types = weapons["damage_types"]
    for projectile in weapons["projectiles"].values():
        for key in ("damage_type", "radius_damage_type"):
            name = projectile[key].replace("$DamageType::", "")
            if name.lower() in types:
                projectile[key] = NS + ":" + types[name.lower()]["name"]
    weapons["damage_types"] = {}
    for value in types.values():
        value["name"] = NS + ":" + value["name"]
        weapons["damage_types"][value["name"].lower()] = value
    write(assets / "weapons.json", weapons)
    presentation["weapons_sha256"] = hashlib.sha256((assets / "weapons.json").read_bytes()).hexdigest()
    write(assets / "presentation.json", presentation)
    manifest = read(native / "package.json")
    # The source's temporary player types are implementation details, replaced
    # by native image mounting and policy, not selectable player archetypes.
    manifest["provides"] = [p for p in manifest["provides"] if p["kind"] == "weapons"]
    manifest.update(name="Rallypack Weapons", version="0.1.0", dependencies={}, companions=[RULES],
        description="Native v0.2.8 playtest port: Rallypack weapons, sounds, models, effects and shared host rules.")
    write(native / "package.json", manifest)
    rules = out / RULES
    rules.mkdir()
    commands = sorted({c.rsplit(":", 1)[-1] for im in weapons["images"].values() if "commands" in im
        for c in list(im["commands"].get("states", {}).values()) + [im.get("command")] +
        [v for k, v in im["commands"].items() if k != "states"] if c})
    state = {"gun": dict(default={}, persist=False)}
    state.update({k: dict(default=v, persist=False, visible="owner") for k, v in banks.items()})
    write(rules / "behaviour.json", dict(schema_version=1, script="rallypack.rhai",
        commands=[dict(name=c, tool_only=True, **({"while_dead": True} if c == "unmount" else {})) for c in commands],
        on_loadout=True, on_death=True, on_drop=True, on_pickup=True, on_damage=True, tick_interval=1,
        state=dict(player=state)))
    config_script = "fn config(image) { switch image {\n" + "\n".join(
        json.dumps(key) + " => " + literal(c) + "," for key, c in configs.items()) + "\n_ => () } }\n"
    reset_script = "fn reset_ammo(p) {\n" + "\n".join(
        f"set_player(p, {json.dumps(k)}, {literal(v)});" for k, v in banks.items()) + "\n}\n"
    item_script = "fn item_config(item) { switch item {\n" + "\n".join(
        json.dumps(k) + " => config(" + json.dumps(v["image"]) + ")," for k, v in weapons["items"].items()
        if v["image"] in configs) + "\n_ => () } }\n"
    template = (Path(__file__).parent / "rallypack/rallypack.rhai").read_text(encoding="utf-8")
    (rules / "rallypack.rhai").write_text(config_script + reset_script + item_script + template, encoding="utf-8")
    write(rules / "package.json", dict(schema_version=1, api=1, id=RULES, version="0.1.0",
        name="Rallypack Weapons Rules", description="Shared native host weapon policy for Rallypack.",
        authors=[], license="proprietary", dependencies={NS: "*"},
        capabilities=["player", "damage", "effects", "chat"], provides=[
            dict(kind="behaviour", id=RULES + ":behaviour/main", file="behaviour.json"),
            dict(kind="script", id=RULES + ":script/main", file="rallypack.rhai")]))
    write(out / "BUILD.json", dict(target_game="v0.2.8", source_sha256=hashlib.sha256(args.source.read_bytes()).hexdigest(),
        items=len(weapons["items"]), images=len(configs), unresolved_materials=unresolved))
    print(f"Built {out}: {len(weapons['items'])} items, {len(configs)} images; unresolved materials: {unresolved}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ("source", "importer", "content", "vanilla", "output"):
        parser.add_argument("--" + key, type=Path, required=True)
    build(parser.parse_args())
