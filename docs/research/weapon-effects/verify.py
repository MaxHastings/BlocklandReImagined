"""Independent stdlib check of additive FX conversion; original inputs read only."""
import hashlib
import json
import math
from pathlib import Path
import sys
import zipfile

LIMIT = 32 << 20


def read(path):
    with path.open("rb") as stream:
        data = stream.read(LIMIT + 1)
    assert len(data) <= LIMIT
    return data


def sha(data):
    return hashlib.sha256(data).hexdigest()


def original(root, name):
    parts = name.split("/")
    assert all(p not in ("", ".", "..") and ":" not in p and "\\" not in p for p in parts)
    if parts[0].lower() == "add-ons":
        candidates = [p for p in (root / "Add-Ons").iterdir() if p.name.lower() == (parts[1] + ".zip").lower()]
        assert len(candidates) == 1 and candidates[0].resolve().is_relative_to(root)
        with zipfile.ZipFile(candidates[0]) as archive:
            matches = [p for p in archive.infolist() if p.filename.lower() == "/".join(parts[2:]).lower()]
            assert len(matches) == 1 and matches[0].file_size <= LIMIT
            return archive.read(matches[0])
    path = root
    for part in parts:
        matches = [p for p in path.iterdir() if p.name.lower() == part.lower()]
        assert len(matches) == 1
        path = matches[0]
    assert path.resolve().is_relative_to(root)
    return read(path)


def close(a, b):
    assert math.isclose(a, b, rel_tol=2e-6, abs_tol=1e-7), (a, b)


def main():
    root, base, output, weapons_path, core, report_path = map(Path, sys.argv[1:])
    root = root.resolve()
    assert not report_path.resolve().is_relative_to(root), "Original installation is read-only"
    load = lambda p: json.loads(read(p))
    proof = load(output / "weapon-source-proof.json")
    weapons = load(weapons_path)
    assert sha(read(weapons_path)) == proof["weapons_sha256"]
    for name, digest in proof["sources"].items():
        data = read(core) if name == "base/server/scripts/allGameScripts.cs (recovered)" else original(root, name)
        assert sha(data) == digest, name
    baseline = load(base / "effects.json")
    library = load(output / "effects.json")
    manifest = load(output / "manifest.json")
    assert sha(read(output / "effects.json")) == manifest["library_sha256"]
    for field in ("particles", "emitters", "lights"):
        converted = {v["id"]: v for v in library[field]}
        for old in baseline[field]:
            assert converted[old["id"]] == old, (field, old["id"])
    for name, record in manifest["textures"].items():
        assert Path(record["file"]).name == record["file"] and ":" not in record["file"]
        assert sha(read(output / record["file"])) == record["sha256"]
    for binding in load(base / "source-proof.json")["texture_bindings"]:
        assert sha(original(root, binding["source"])) == binding["sha256"]
        assert manifest["textures"][binding["id"]]["sha256"] == binding["sha256"]
    definitions = {d["name"].lower(): d for d in weapons["definitions"]}
    for emitter in library["emitters"]:
        if emitter["id"] not in proof["added_emitters"]:
            continue
        f = {k: v.strip().strip('"') for k, v in definitions[emitter["id"].split("/")[-1]]["fields"].items()}
        for native, source, default, factor in (
            ("period", "ejectionperiodms", 100, .001), ("period_variance", "periodvariancems", 0, .001),
            ("speed", "ejectionvelocity", 2, 1), ("speed_variance", "velocityvariance", 1, 1),
            ("offset", "ejectionoffset", 0, 1), ("lifetime", "lifetimems", 0, .001),
        ):
            close(emitter[native], float(f.get(source, default)) * factor)
        assert emitter["particles"] == ["v20/particle/" + v.lower() for v in f["particles"].split()]
    for particle in library["particles"]:
        if particle["id"] not in proof["added_particles"]:
            continue
        f = {k: v.strip().strip('"') for k, v in definitions[particle["id"].split("/")[-1]]["fields"].items()}
        lifetime = max(1, float(f.get("lifetimems", 1000)))
        close(particle["lifetime"], lifetime / 1000)
        close(particle["lifetime_variance"], min(float(f.get("lifetimevariancems", 0)), lifetime - 1) / 1000)
        for native, source, default in (("gravity", "gravitycoefficient", 0), ("drag", "dragcoefficient", 0),
                                         ("wind", "windcoefficient", 1), ("inherited_velocity", "inheritedvelfactor", 0)):
            close(particle[native], float(f.get(source, default)))
        for i, key in enumerate(particle["keys"]):
            close(key["size"], float(f.get(f"sizes[{i}]", 1)))
    report = {"verified_sources": len(proof["sources"]), "original_textures": len(manifest["textures"]),
              "added_particles": len(proof["added_particles"]), "added_emitters": len(proof["added_emitters"]),
              "added_composites": len(proof["added_composites"]), "baseline_preserved": True,
              "unresolved": manifest["unresolved"]}
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k != "unresolved"}, indent=2))


if __name__ == "__main__":
    main()
