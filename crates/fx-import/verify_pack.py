"""Independent stdlib verifier: native pack hashes, image dimensions, primary originals.
Never writes the source installation. Usage: verify_pack.py PACK ORIGINAL CORE REPORT
"""
import hashlib
import json
import pathlib
import struct
import sys
import zipfile


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read(path):
    assert path.is_file() and path.stat().st_size <= 32 * 1024 * 1024, path
    data = path.read_bytes()
    assert len(data) <= 32 * 1024 * 1024, path
    return data


def dimensions(data):
    if data.startswith(b"\x89PNG\r\n\x1a\n"):
        assert data[12:16] == b"IHDR"
        return struct.unpack(">II", data[16:24])
    assert data[:2] == b"\xff\xd8", "Expected original PNG/JPEG"
    i = 2
    while i < len(data):
        assert data[i] == 255
        while data[i] == 255:
            i += 1
        marker = data[i]
        i += 1
        if marker in (0xD8, 0xD9):
            continue
        length = struct.unpack(">H", data[i:i + 2])[0]
        if marker in (0xC0, 0xC1, 0xC2):
            height, width = struct.unpack(">HH", data[i + 3:i + 7])
            return width, height
        i += length
    raise AssertionError("JPEG has no supported SOF")


def primary(root, virtual):
    parts = virtual.split("/")
    assert all(p and p not in (".", "..") and ":" not in p and "\\" not in p for p in parts)
    if parts[0].lower() == "add-ons":
        matches = [p for p in (root / "Add-Ons").iterdir() if p.name.lower() == parts[1].lower() + ".zip"]
        assert len(matches) == 1
        path = matches[0].resolve()
        assert path.is_relative_to(root)
        with zipfile.ZipFile(path) as archive:
            names = [n for n in archive.namelist() if n.lower() == "/".join(parts[2:]).lower()]
            assert len(names) == 1
            assert archive.getinfo(names[0]).file_size <= 32 * 1024 * 1024
            return archive.read(names[0])
    path = root
    for part in parts:
        matches = [p for p in path.iterdir() if p.name.lower() == part.lower()]
        assert len(matches) == 1, virtual
        path = matches[0]
    path = path.resolve()
    assert path.is_relative_to(root)
    return read(path)


def main():
    pack, original, core, output = map(pathlib.Path, sys.argv[1:])
    pack, original = pack.resolve(), original.resolve()
    assert not output.resolve().is_relative_to(original)
    manifest = json.loads(read(pack / "manifest.json"))
    proof = json.loads(read(pack / "source-proof.json"))
    library_bytes = read(pack / "effects.json")
    library = json.loads(library_bytes)
    assert manifest["schema_version"] == library["schema_version"] == 1
    assert digest(library_bytes) == manifest["library_sha256"]
    texture_proof = {t["id"]: t for t in proof["texture_bindings"]}
    image_checks = []
    for id_, file in library["textures"].items():
        assert "/" not in file and "\\" not in file and ":" not in file
        path = (pack / file).resolve()
        assert path.is_relative_to(pack)
        data = read(path)
        record = manifest["textures"][id_]
        source = texture_proof[id_]
        assert digest(data) == record["sha256"] == source["sha256"]
        assert data == primary(original, source["source"])
        assert dimensions(data) == (record["width"], record["height"])
        image_checks.append({"id": id_, "bytes": len(data), "sha256": digest(data), "dimensions": dimensions(data)})
    ids = set()
    for kind in ("lights", "particles", "emitters"):
        for item in library[kind]:
            assert item["id"] not in ids
            ids.add(item["id"])
    particles = {p["id"] for p in library["particles"]}
    emitters = {e["id"] for e in library["emitters"]}
    lights = {l["id"] for l in library["lights"]}
    for particle in library["particles"]:
        assert particle["texture"] in library["textures"]
    for emitter in library["emitters"]:
        assert set(emitter["particles"]) <= particles
    for light in library["lights"]:
        if light["flare"]:
            assert light["flare"]["texture"] in library["textures"]
    composites = {c["id"] for c in manifest["composites"]}
    assert len(composites) == len(manifest["composites"])
    for composite in manifest["composites"]:
        assert set(composite["emitters"]) <= emitters
        assert composite["light"] is None or composite["light"] in lights
        assert composite["burst"] is None or composite["burst"][0] in emitters
    for binding in manifest["bindings"]:
        assert binding["resource"] in ids | composites
    for source in proof["sources"]:
        data = read(core) if source["path"] == "core/allGameScripts-Vanilla.cs" else primary(original, source["path"])
        assert digest(data) == source["sha256"], source["path"]
    result = {"verified": True, "schema": 1, "lights": len(lights), "particles": len(particles), "emitters": len(emitters), "textures": len(image_checks), "composites": len(composites), "bindings": len(manifest["bindings"]), "verified_sources": len(proof["sources"]), "images": image_checks, "unresolved": manifest["unresolved"]}
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in result.items() if k not in ("images", "unresolved")}, indent=2))


if __name__ == "__main__":
    main()
