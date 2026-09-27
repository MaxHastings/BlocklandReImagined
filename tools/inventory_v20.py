"""Read-only content census; does not execute scripts or extract archives.

This is an inventory, not a format validator or a TorqueScript parser.
Only base/, Add-Ons/, and saves/ are read. User configuration is excluded.
"""
import argparse
import collections
import json
from pathlib import Path, PurePosixPath
import re
import struct
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    rows, errors = [], []
    text_extensions = {".cs", ".gui", ".mis", ".bls"}

    def inspect(source, virtual_path, size, stream):
        ext = PurePosixPath(virtual_path).suffix.lower()
        row = dict(source=source, path=virtual_path, extension=ext, bytes=size)
        # Bounded reads; binary header values are candidates, not validation.
        data = stream.read(min(size, 2 * 1024 * 1024))
        if ext in {".dts", ".dsq", ".dif", ".dso"} and len(data) >= 4:
            word = struct.unpack_from("<I", data)[0]
            row["header_version_candidate"] = word & 255 if ext == ".dts" else word
        elif ext == ".ter" and data:
            row["header_version_candidate"] = data[0]
        if ext in text_extensions:
            txt = data.decode("utf-8-sig", errors="replace")
            row["scan_truncated"] = size > len(data)
            row["declared_classes"] = sorted(set(re.findall(
                r"\b(?:new|datablock)\s+(\w+)\s*\(", txt, re.I)))
            if ext == ".bls":
                row["extension_records"] = dict(collections.Counter(
                    re.findall(r"^\+-([^\s]+)", txt, re.M)))
            if ext == ".mis":
                row["asset_references"] = sorted(set(re.findall(
                    r'"([^"\r\n]+\.(?:dif|dts|ter|dml|png|jpg|wav))"', txt, re.I)))
        rows.append(row)

    for folder in ("base", "Add-Ons", "saves"):
        for path in sorted((args.root / folder).rglob("*")):
            if not path.is_file():
                continue
            rel = path.relative_to(args.root).as_posix()
            try:
                if path.suffix.lower() == ".zip":
                    with zipfile.ZipFile(path) as archive:
                        for member in archive.infolist():
                            if member.is_dir():
                                continue
                            entry = member.filename.replace("\\", "/")
                            # Most Blockland add-ons store paths relative to the ZIP root.
                            virtual = f"Add-Ons/{path.stem}/{entry}"
                            try:
                                with archive.open(member) as stream:
                                    inspect(rel, virtual, member.file_size, stream)
                            except (OSError, ValueError, RuntimeError, zipfile.BadZipFile) as exc:
                                errors.append(dict(path=rel, member=entry, error=str(exc)))
                else:
                    with path.open("rb") as stream:
                        inspect("loose", rel, path.stat().st_size, stream)
            except (OSError, ValueError, RuntimeError, zipfile.BadZipFile) as exc:
                errors.append(dict(path=rel, error=str(exc)))

    virtual_sources = collections.defaultdict(list)
    versions = collections.defaultdict(collections.Counter)
    for row in rows:
        virtual_sources[row["path"].casefold()].append(row["source"])
        if "header_version_candidate" in row:
            versions[row["extension"]][str(row["header_version_candidate"])] += 1
    result = dict(
        root=str(args.root.resolve()),
        scope=["base", "Add-Ons", "saves"],
        note="Physical loose files plus ZIP members; duplicates counted separately. Header reads and regex scans do not establish compatibility.",
        entry_count=len(rows),
        extensions=dict(collections.Counter(r["extension"] for r in rows).most_common()),
        header_versions=dict(versions),
        duplicate_virtual_paths={k: v for k, v in virtual_sources.items() if len(v) > 1},
        errors=errors,
        entries=rows,
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps({k: v for k, v in result.items() if k not in {"entries", "duplicate_virtual_paths"}}, indent=2))
    print("Duplicate virtual paths:", len(result["duplicate_virtual_paths"]))


if __name__ == "__main__":
    main()
