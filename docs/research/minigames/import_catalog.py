"""Read-only, bounded vanilla declaration audit. Never executes TorqueScript.

Outputs native selectable IDs separately from provenance. Not a runtime dependency.
"""
import argparse
import hashlib
import json
import re
import zipfile
from pathlib import Path

DECL = re.compile(r"datablock\s+(PlayerData|ItemData)\s*\(\s*(\w+)\s*(?::\s*(\w+))?\s*\)\s*\{(.*?)\};", re.S | re.I)
FIELD = re.compile(r"(\w+)\s*=\s*([^;]+);")
COMMENTS = re.compile(r'"(?:\\.|[^"\\])*"|/\*.*?\*/|//[^\r\n]*', re.S)

def clean(text):
    return COMMENTS.sub(lambda m: m[0] if m[0].startswith('"') else re.sub(r"[^\r\n]", " ", m[0]), text)

def digest(data):
    return hashlib.sha256(data).hexdigest()

def convert(reference, core_path, client_path, output):
    declarations = {}
    sources = []
    diagnostics = []
    for relative in ("base/server/scripts/allGameScripts-Vanilla.cs.dso", "base/client/scripts/allClientScripts-Vanilla.cs.dso", "base/server/scripts/game.cs.dso"):
        path = reference / relative
        if path.is_file():
            data = path.read_bytes()
            sources.append({"path": str(path), "sha256": digest(data), "bytes": len(data), "role": "original_bytecode_provenance_only"})
        else:
            diagnostics.append({"kind": "missing_original_bytecode", "path": relative})

    def scan(data, path):
        if len(data) > 8 * 1024 * 1024:
            raise ValueError(f"oversized script: {path}")
        sources.append({"path": path, "sha256": digest(data), "bytes": len(data)})
        text = clean(data.decode("utf-8", "replace"))
        for match in DECL.finditer(text):
            kind, name, parent, body = match.groups()
            fields = {k.lower(): v.strip().strip('"') for k, v in FIELD.findall(body)}
            key = name.lower()
            if key in declarations:
                diagnostics.append({"kind": "duplicate_declaration", "name": name, "source": path})
            declarations[key] = {"name": name, "kind": kind, "parent": parent.lower() if parent else None,
                "fields": fields, "source": path, "line": text[:match.start()].count("\n") + 1}

    scan(core_path.read_bytes(), str(core_path))
    addons = reference / "Add-Ons"
    for path in sorted(addons.glob("*.zip")):
        if not path.name.startswith(("Player_", "Item_", "Weapon_", "Vehicle_")):
            continue
        sources.append({"path": str(path), "sha256": digest(path.read_bytes()), "bytes": path.stat().st_size})
        with zipfile.ZipFile(path) as archive:
            for entry in sorted(archive.infolist(), key=lambda e: e.filename):
                if entry.filename.lower().endswith(".cs"):
                    if entry.file_size > 8 * 1024 * 1024:
                        raise ValueError(f"oversized ZIP member: {path}!{entry.filename}")
                    scan(archive.read(entry), f"{path.name}!{entry.filename}")

    def fields(key, seen=None):
        seen = set() if seen is None else seen
        if key in seen:
            raise ValueError(f"inheritance cycle: {key}")
        seen.add(key)
        d = declarations[key]
        merged = {}
        if d["parent"]:
            if d["parent"] in declarations:
                merged.update(fields(d["parent"], seen))
            else:
                diagnostics.append({"kind": "unresolved_parent", "name": d["name"], "parent": d["parent"]})
        merged.update(d["fields"])
        return merged

    catalog = {"schema_version": 1, "player_types": [], "items": {}}
    selectable = []
    for key, declaration in sorted(declarations.items()):
        f = fields(key)
        if not f.get("uiname", ""):
            continue
        is_player = declaration["kind"].lower() == "playerdata"
        native_id = f"v20.{'player' if is_player else 'weapon'}.{key}"
        if is_player:
            catalog["player_types"].append(native_id)
        else:
            sports = f.get("issportball", "0") == "1"
            if sports and not f.get("image"):
                raise ValueError(f"sports item missing image: {key}")
            catalog["items"][native_id] = f"v20.image.{f['image'].lower()}" if sports else None
        selectable.append({"id": native_id, "ui_name": f["uiname"], "source": declaration["source"],
            "line": declaration["line"], "parent": declaration["parent"], "authored_fields": f})

    client = client_path.read_bytes()
    sources.append({"path": str(client_path), "sha256": digest(client), "bytes": len(client)})
    text = client.decode("utf-8", "replace")
    block = text[text.index("function CreateMiniGameGui::onWake"):text.index("function CreateMiniGameGui::LoadDataBlocks")]
    defaults = dict(re.findall(r"\$MiniGame::([\w:]+)\s*=\s*([^;]+);", block))
    required = {"RespawnTime": "1", "VehicleRespawnTime": "5", "BrickRespawnTime": "30", "Points::KillPlayer": "1", "Points::KillSelf": "-1"}
    for key, value in required.items():
        if defaults.get(key) != value:
            raise ValueError(f"source default changed: {key}")
    evidence = {"schema_version": 1, "id": "v20.minigame.rules.1", "sources": sources,
        "gui_defaults": defaults, "selectable": selectable, "diagnostics": diagnostics,
        "lives": {"native": "Unlimited", "evidence": "No lives limit in core MiniGameSO lifecycle, GUI fields, death/respawn, or selected stock packages"},
        "notes": ["Constructor-only transitional defaults intentionally not published as player defaults",
            "Native runtime implements rules; this audit is not a Torque evaluator",
            "All installed Player/Item/Weapon/Vehicle stock packages considered, not just enabled client preferences"]}
    output.mkdir(parents=True, exist_ok=False)
    for name, data in [("catalog.json", catalog), ("evidence.json", evidence)]:
        (output / name).write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({"output": str(output), "players": len(catalog["player_types"]), "items": len(catalog["items"]), "diagnostics": diagnostics,
        "catalog_sha256": digest((output / "catalog.json").read_bytes())}, indent=2))

if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--reference", type=Path, required=True)
    p.add_argument("--core", type=Path, required=True)
    p.add_argument("--client", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    a = p.parse_args()
    convert(a.reference, a.core, a.client, a.output)
