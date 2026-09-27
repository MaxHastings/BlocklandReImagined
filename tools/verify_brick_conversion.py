"""Independently compare native mesh corners, UVs, normals and colors to BLB data.

Read-only; explicitly applies the hash-scoped adaptation ledger to source copies.
Does not call the Rust parser. The native render remains a separate check.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import zipfile


def close(a, b):
    assert len(a) == len(b)
    assert all(math.isclose(x, y, abs_tol=2e-5, rel_tol=2e-6) for x, y in zip(a, b)), (a, b)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("conversion", type=Path)
    args = parser.parse_args()
    records = json.loads((args.conversion / "manifest.json").read_text())["records"]
    repairs = json.loads((Path(__file__).resolve().parents[1] / "crates/convert/data/blb-repairs.json").read_text())
    repaired = {r["sha256"]: r for r in repairs}
    checked_bricks = checked_quads = standard = 0
    normal_comparisons = []
    for row in records:
        if not row["output"] or not row["output"].endswith(".brick.json"):
            continue
        if "::" in row["source"]:
            archive, member = row["source"].split("::", 1)
            with zipfile.ZipFile(args.root / archive) as z:
                data = z.read(member)
        else:
            data = (args.root / row["source"]).read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        assert digest == row["source_sha256"]
        native = json.loads((args.conversion / row["output"]).read_text())
        provenance = json.loads((args.conversion / row["output"].replace(".brick.json", ".source.json")).read_text())
        assert provenance["original_text"].encode() == data
        lines = data.decode().splitlines()
        for edit in repaired.get(digest, {}).get("edits", []):
            assert lines[edit["line"]-1] == edit["before"]
            lines[edit["line"]-1] = edit["after"]
        lines = [s.split("//")[0].strip() for s in "\n".join(lines).splitlines()]
        lines = [s for s in lines if s and not (s.startswith("---") and "quads" in s.lower())]
        dims = list(map(int, lines[0].split()))
        assert native["footprint_studs"] == dims[:2] and native["height_plates"] == dims[2]
        if lines[1] == "BRICK":
            close(native["collision_boxes"][0]["size"], [dims[0]*0.5, dims[2]*0.2, dims[1]*0.5])
            standard += 1
        else:
            if lines[1] == "SPECIAL":
                assert native["attachment_rows"] == [s.lower() for s in lines[2:2+dims[1]*dims[2]]]
            markers = [i for i, s in enumerate(lines) if s.startswith("TEX:")]
            assert len(markers) == len(native["quads"])
            for index, start in enumerate(markers):
                quad = native["quads"][index]
                assert quad["surface"].replace("_", "") == lines[start][4:].lower()
                assert lines[start+1] == "POSITION:"
                positions = [list(map(float, s.split())) for s in lines[start+2:start+6]]
                assert lines[start+6] == "UV COORDS:"
                uv = [list(map(float, s.split())) for s in lines[start+7:start+11]]
                cursor = start+11
                colors = None
                if lines[cursor] == "COLORS:":
                    colors = [list(map(float, s.split())) for s in lines[cursor+1:cursor+5]]
                    cursor += 5
                assert lines[cursor] == "NORMALS:"
                normals = [list(map(float, s.split())) for s in lines[cursor+1:cursor+5]]
                for native_index, source_index in enumerate([0, 3, 2, 1]):
                    vertex = quad["vertices"][native_index]
                    x, y, z = positions[source_index]
                    close(vertex["position"], [x*0.5, z*0.2, -y*0.5])
                    close(vertex["uv"], uv[source_index])
                    x, y, z = normals[source_index]
                    length = math.sqrt(x*x+y*y+z*z)
                    close(vertex["normal"], [x/length, z/length, -y/length])
                    if colors:
                        close(quad["colors"][native_index], colors[source_index])
                    else:
                        assert quad["colors"] is None
                if quad["surface"] == "ramp":
                    v = quad["vertices"]
                    a = [v[1]["position"][j]-v[0]["position"][j] for j in range(3)]
                    b = [v[2]["position"][j]-v[0]["position"][j] for j in range(3)]
                    cross = [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
                    length = math.sqrt(sum(x*x for x in cross))
                    if length > 1e-7:
                        geometry = [x/length for x in cross]
                        n = v[0]["normal"]
                        scaled = [n[0]/0.5, n[1]/0.2, n[2]/0.5]
                        length = math.sqrt(sum(x*x for x in scaled))
                        scaled = [x/length for x in scaled]
                        normal_comparisons.append([sum(x*y for x,y in zip(geometry,n)), sum(x*y for x,y in zip(geometry,scaled))])
                checked_quads += 1
        checked_bricks += 1
    assert checked_bricks > 0
    print(json.dumps({"verified_bricks": checked_bricks, "standard_bricks": standard,
        "verified_authored_quads": checked_quads, "ramp_normal_samples": len(normal_comparisons),
        "mean_geometric_alignment_rotation": sum(v[0] for v in normal_comparisons)/len(normal_comparisons),
        "mean_geometric_alignment_inverse_scale": sum(v[1] for v in normal_comparisons)/len(normal_comparisons)}, indent=2))


if __name__ == "__main__":
    main()
