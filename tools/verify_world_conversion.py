"""Independent original-BLS/native-state comparison; does not call Rust readers."""
import hashlib
import json
import struct
import sys
from pathlib import Path


def f32(value):
    return struct.unpack("<f", struct.pack("<f", float(value)))[0]


def verify(root, catalog_path, output):
    catalog = json.loads(catalog_path.read_text(encoding="utf8"))
    names = {b["display_name"].lower(): b["id"] for b in catalog["bricks"]}
    report = json.loads((output / "report.json").read_text(encoding="utf8"))
    assert report["errors"] == 0
    totals = dict(saves=0, bricks=0, source_records=0, adapted_events=0, missing_bricks=0)
    for saved in report["saves"]:
        original = (root / saved["source"]).read_bytes()
        sha = hashlib.sha256(original).hexdigest()
        assert saved["sha256"] == sha
        assert (output / "provenance" / f"{sha}.source.bls").read_bytes() == original
        world = json.loads((output / saved["file"]).read_text(encoding="utf8"))
        lines = original.decode(world["source_encoding"]).splitlines()
        description = int(lines[1])
        assert world["description"] == lines[2:2 + description]
        palette_start = 2 + description
        expected_palette = [[f32(x) for x in line.split()] for line in lines[palette_start:palette_start + 64]]
        assert [[f32(x) for x in color] for color in world["palette"]] == expected_palette
        at = palette_start + 64
        assert len(world["bricks"]) == int(lines[at].split()[1])
        source_bricks = []
        for line_no, line in enumerate(lines[at + 1:], at + 2):
            if not line:
                continue
            if not line.startswith("+-"):
                source_bricks.append([])
            source_bricks[-1].append((line_no, line))
        for i, records in enumerate(source_bricks, 1):
            brick = world["bricks"][str(i)]
            assert [(r["line"], r["text"]) for r in brick["source_records"]] == records
            label, values = records[0][1].split('"', 1)
            fields = values[1:].split(" ")
            assert len(fields) == 12
            assert [f32(x) for x in brick["position"]] == [f32(fields[0]), f32(fields[2]), -f32(fields[1])]
            assert brick["quarter_turns"] == int(fields[3])
            assert brick["base_plate"] == bool(int(fields[4]))
            assert brick["color"] == int(fields[5])
            assert brick["color_effect"] == int(fields[7])
            assert brick["shape_effect"] == int(fields[8])
            assert [brick[k] for k in ("raycast", "colliding", "visible")] == [bool(int(x)) for x in fields[9:12]]
            assert brick["owner"] == 0
            if label.lower() in names:
                assert brick["definition"] == {"kind": "resolved", "value": names[label.lower()]}
            else:
                assert brick["definition"]["kind"] == "unresolved"
                totals["missing_bricks"] += 1
            if fields[6] not in ("", "/"):
                assert brick["print"]["value"]["name"] == fields[6]
            else:
                assert brick["print"] is None
            for _, record in records[1:]:
                if record.startswith("+-NTOBJECTNAME "):
                    assert brick["name"] == record.split(" ", 1)[1].strip()
                elif record.startswith("+-LIGHT "):
                    name, state = record[8:].split('"', 1)
                    assert brick["light"]["asset"]["value"]["name"] == name.strip()
                    assert brick["light"]["enabled"] == (bool(int(state)) if state.strip() else True)
                elif record.startswith("+-EMITTER "):
                    name, direction = record[10:].split('"', 1)
                    assert brick["emitter"]["direction"] == int(direction)
                    asset = brick["emitter"]["asset"]
                    assert asset is None if name.strip().upper() == "NONE" else asset["value"]["name"] == name.strip()
            totals["source_records"] += len(records)
            totals["adapted_events"] += len(brick["events"])
        totals["bricks"] += len(source_bricks)
        totals["saves"] += 1
    assert totals["bricks"] == report["bricks"]
    result = {"status": "passed", **totals, "scope": "source hashes/bytes, descriptions/palettes, every brick definition/transform/flag and preserved extension; light/emitter properties; does not verify visual or interactive behavior"}
    (output / "independent-verification.json").write_text(json.dumps(result, indent=2), encoding="utf8")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    verify(*(Path(p) for p in sys.argv[1:]))
