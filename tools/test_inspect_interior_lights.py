import struct
import unittest

import inspect_interior_lights as inspect


def fixture():
    chunks = [
        ("animated lights", struct.pack("<IIHHI", 0, 0, 1, 3, 250)),
        ("light states", struct.pack("<BBBIIH", 255, 128, 0, 125, 0, 1)),
        ("light state data", struct.pack("<IIH", 17, 1, 2)),
        ("light state buffer", b"\x01\x02"),
        ("name buffer", b"TV\0"),
    ]
    data = b""
    sections = []
    for label, chunk in chunks:
        sections.append({"label": label, "offset": len(data), "length": len(chunk)})
        data += chunk
    return data, {"preserved_sections": sections}


class InteriorLightInspection(unittest.TestCase):
    def test_state_and_surface_references(self):
        data, provenance = fixture()
        report = inspect.inspect(data, provenance)
        light = report["lights"][0]
        self.assertEqual(light["name_bytes"], [84, 86])
        self.assertEqual((light["flags"], light["duration_ms"]), (3, 250))
        self.assertEqual(light["states"][0]["color"], [255, 128, 0])
        self.assertEqual(light["states"][0]["surfaces"],
                         [{"surface": 17, "mask_offset": 1, "instance_state": 2}])

    def test_rejects_truncated_sections_and_out_of_range_references(self):
        data, provenance = fixture()
        with self.assertRaises(ValueError):
            inspect.inspect(data[:-1], provenance)
        corrupt = bytearray(data)
        record = provenance["preserved_sections"][2]["offset"]
        struct.pack_into("<I", corrupt, record + 4, 2)
        with self.assertRaises(ValueError):
            inspect.inspect(corrupt, provenance)
        with self.assertRaises(ValueError):
            inspect.inspect(data, provenance, 1)


if __name__ == "__main__":
    unittest.main()
