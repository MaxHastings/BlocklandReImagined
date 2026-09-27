"""Independent original/native brick material audit; never writes original content.

python tools/verify_brick_materials.py INSTALL BUNDLE --report REPORT
Evidence paths in the manifest resolve from the conversion working directory.
"""
import argparse
from collections import Counter
import hashlib
import io
import json
from pathlib import Path
import re
import zipfile

from PIL import Image


def digest(data):
    return hashlib.sha256(data).hexdigest()


def inside(root, relative):
    assert relative and "\\" not in relative and ":" not in relative
    assert all(p not in ("", ".", "..") for p in relative.split("/"))
    path = (root / relative).resolve()
    assert path.is_relative_to(root.resolve()), relative
    return path


def verify(install, bundle):
    manifest_bytes = (bundle / "brick-materials.json").read_bytes()
    data = json.loads(manifest_bytes)
    assert data["schema_version"] == 1
    assert set(data["surfaces"]) == {"top", "side", "bottom_edge", "bottom_loop", "ramp"}
    evidence = [Path(s["path"]).read_bytes() for s in data["evidence"]]
    for source, raw in zip(data["evidence"], evidence):
        assert digest(raw) == source["sha256"], source["path"]
    defaults = {}
    for line, text in enumerate(evidence[0].decode().splitlines(), 1):
        match = re.fullmatch(r"\s*\$AddOn__(Print_\w+)\s*=\s*1\s*;\s*", text.split("//")[0])
        if match:
            assert match[1] not in defaults
            defaults[match[1]] = line
    inventory = {p["name"]: p for p in json.loads(evidence[1])["packages"]}
    assert set(defaults) == {p["name"] for p in data["packages"]}
    originals = {}
    package_counts = {}
    for package in data["packages"]:
        name = package["name"]
        raw = inside(install, package["archive"]).read_bytes()
        assert digest(raw) == package["archive_sha256"] == inventory[name]["archive_sha256"]
        assert package["default_list_line"] == defaults[name] == inventory[name]["default_list_line"]
        with zipfile.ZipFile(io.BytesIO(raw)) as archive:
            names = [n for n in archive.namelist() if not n.endswith("/")]
            assert len({n.lower() for n in names}) == len(names)
            originals[package["archive"]] = {n: archive.read(n) for n in names}
            original_prints = {n for n in names if n.lower().startswith("prints/") and n.lower().endswith(".png")}
            native_prints = [p for p in data["prints"] if p["package"] == name]
            assert original_prints == {p["diffuse"]["source"]["path"] for p in native_prints}
            assert {n for n in names if n.lower().startswith("icons/") and n.lower().endswith(".png")} == {p["icon"]["source"]["path"] for p in native_prints}
            package_counts[name] = len(native_prints)
    output_paths = set()
    checked_images = []
    images = list(data["surfaces"].values()) + [p[k] for p in data["prints"] for k in ("diffuse", "icon")]
    for item in images:
        assert item["path"] not in output_paths
        output_paths.add(item["path"])
        raw = inside(bundle, item["path"]).read_bytes()
        source = item["source"]
        original = originals[source["archive"]][source["path"]] if source["archive"] else inside(install, source["path"]).read_bytes()
        assert raw == original
        assert digest(raw) == item["sha256"] == source["sha256"]
        with Image.open(io.BytesIO(raw)) as image:
            image.load()
            assert image.format == "PNG" and image.size == (item["width"], item["height"])
            checked_images.append({"path": item["path"], "width": image.width, "height": image.height, "sha256": digest(raw)})
    assert {p.relative_to(bundle).as_posix() for p in bundle.rglob("*") if p.is_file()} == output_paths | {"brick-materials.json"}
    ids, aliases = set(), set()
    for p in data["prints"]:
        assert p["id"] == f'print/{p["package"].lower()}/{p["name"].lower()}'
        assert p["id"] not in ids
        ids.add(p["id"])
        assert p["aspect"] == p["package"].split("_")[1]
        assert p["aliases"] == [f'{p["aspect"]}/{p["name"]}']
        assert p["diffuse"]["source"]["path"] == f'prints/{p["name"]}.png'
        assert p["icon"]["source"]["path"] == f'icons/{p["name"]}.png'
        for alias in p["aliases"]:
            assert alias.lower() not in aliases
            aliases.add(alias.lower())
    return {"schema_version": 1, "passed": True, "manifest_sha256": digest(manifest_bytes), "surfaces": len(data["surfaces"]), "prints": len(data["prints"]), "icons": len(data["prints"]), "image_outputs": len(images), "package_counts": package_counts, "aspect_counts": dict(Counter(p["aspect"] for p in data["prints"])), "byte_identical_images": True, "evidence_inputs": len(evidence), "excluded_installed_packages": data["excluded_installed_packages"], "warnings": data["warnings"], "images": checked_images, "scope": "Every print/icon in packages proven default-enabled by the recovered stock list and matching the checked installation inventory. This does not independently certify historical distribution byte authenticity or prove absence of other shipped-but-disabled packs."}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("install", type=Path)
    parser.add_argument("bundle", type=Path)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    report = verify(args.install.resolve(), args.bundle.resolve())
    assert not args.report.resolve().is_relative_to(args.install.resolve()), "Report must not modify original installation"
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k != "images"}, indent=2))


if __name__ == "__main__":
    main()
