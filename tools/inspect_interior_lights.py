"""Inspect preserved DIF animated-light records without interpreting their masks.

Inputs are the original DIF and the matching `*.source.json` provenance emitted
by bri-convert. This is an offline diagnostic; it does not alter either input.
"""
import argparse
import json
import struct
from pathlib import Path


def inspect(data, provenance, detail=0):
    def section(label, stride=1):
        matches = [s for s in provenance["preserved_sections"] if s["label"] == label]
        if detail < 0 or detail >= len(matches):
            raise ValueError(f"missing {label!r} section for detail {detail}")
        item = matches[detail]
        start, length = item["offset"], item["length"]
        if start < 0 or length < 0 or start + length > len(data) or length % stride:
            raise ValueError(f"invalid {label!r} section bounds")
        return data[start : start + length]

    def rows(label, fmt):
        return list(struct.iter_unpack(fmt, section(label, struct.calcsize(fmt))))

    lights = rows("animated lights", "<IIHHI")
    states = rows("light states", "<BBBIIH")
    records = rows("light state data", "<IIH")
    masks = section("light state buffer")
    names = section("name buffer")
    output = []
    for name_at, state_at, count, flags, duration_ms in lights:
        if name_at >= len(names) or state_at + count > len(states):
            raise ValueError("animated light references outside name or state section")
        end = names.find(b"\0", name_at)
        if end < 0:
            raise ValueError("animated light name is not terminated")
        light = {"name_bytes": list(names[name_at:end]), "flags": flags,
                 "duration_ms": duration_ms, "states": []}
        for red, green, blue, active_ms, first, size in states[state_at : state_at + count]:
            if first + size > len(records):
                raise ValueError("light state references outside state-data section")
            sources = []
            for surface, offset, index in records[first : first + size]:
                if offset != 0xFFFFFFFF and offset >= len(masks):
                    raise ValueError("light state mask offset is out of range")
                sources.append({"surface": surface, "mask_offset": None if offset == 0xFFFFFFFF else offset,
                                "instance_state": index})
            light["states"].append({"color": [red, green, blue], "active_ms": active_ms,
                                    "surfaces": sources})
        output.append(light)
    return {"detail": detail, "mask_bytes": len(masks), "lights": output}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("dif", type=Path)
    parser.add_argument("provenance", type=Path)
    parser.add_argument("--detail", type=int, default=0)
    args = parser.parse_args()
    print(json.dumps(inspect(args.dif.read_bytes(), json.loads(args.provenance.read_text()), args.detail), indent=2))


if __name__ == "__main__":
    main()
