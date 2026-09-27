"""Reconstruct TER bytes from native JSON + provenance and compare to originals.

Read-only audit. Does not share the Rust reader or execute source scripts.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import zipfile
import os
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("conversion", type=Path)
    args = parser.parse_args()
    manifest = json.loads((args.conversion / "manifest.json").read_text())
    verified = []
    for row in manifest["records"]:
        if row["error"] or not row["output"].endswith(".terrain.json"):
            continue
        terrain = json.loads((args.conversion / row["output"]).read_text())
        source_path = row["output"].replace(".terrain.json", ".source.json")
        source = json.loads((args.conversion / source_path).read_text())
        out = bytearray([source["source_version"]])
        values = [v * 32 for v in terrain["elevations"]]
        assert all(v == int(v) and 0 <= v <= 65535 for v in values)
        out.extend(struct.pack("<65536H", *(int(v) for v in values)))
        out.extend(source["original_material_flags"])
        assert terrain["primary_layers"] == [v & 7 for v in source["original_material_flags"]]
        by_slot = {layer["slot"]: layer for layer in terrain["layers"]}
        for slot in range(8):
            name = by_slot[slot]["material"].encode("utf-8") if slot in by_slot else b""
            out.append(len(name))
            out.extend(name)
        for slot in sorted(by_slot):
            out.extend(by_slot[slot]["weights"])
        for field in ["texture_authoring_script", "height_authoring_script"]:
            out.extend(struct.pack("<I", len(source[field])))
            out.extend(source[field])
        if "::" in row["source"]:
            archive_path, member = row["source"].split("::", 1)
            physical = args.root / archive_path
            with physical.open("rb") as stream:
                rar = stream.read(6) == b"Rar!\x1a\x07"
            if rar:
                original = subprocess.check_output([os.environ.get("BRI_7Z", "7z"), "e", "-so", "-y", "-spd", "--", str(physical), member], timeout=30)
            else:
                with zipfile.ZipFile(physical) as archive:
                    original = archive.read(member)
        else:
            original = (args.root / row["source"]).read_bytes()
        assert out == original, f"Byte reconstruction differs: {row['source']}"
        assert hashlib.sha256(original).hexdigest() == row["source_sha256"]
        verified.append(row["virtual_path"])
    assert verified, "No converted terrain verified"
    print(json.dumps({"byte_identical_reconstructions": len(verified), "verified": verified,
                      "conversion_scan_errors": manifest["scan_errors"]}, indent=2))


if __name__ == "__main__":
    main()
