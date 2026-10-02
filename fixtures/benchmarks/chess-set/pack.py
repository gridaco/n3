"""Reproduce the pinned Chess Set GLB using Python's standard library only."""

import argparse
import hashlib
import json
from pathlib import Path
import struct


ROOT = Path(__file__).resolve().parent


def pack():
    provenance = json.loads((ROOT / "provenance.json").read_text())
    resources = {}
    for record in provenance["files"]:
        path = ROOT / record["path"]
        path.resolve().relative_to((ROOT / "source").resolve())
        content = path.read_bytes()
        if (len(content) != record["bytes"]
                or hashlib.sha256(content).hexdigest() != record["sha256"]):
            raise ValueError(f"Source integrity mismatch: {record['path']}")
        resources[path.relative_to(ROOT / "source").as_posix()] = content

    document = json.loads(resources["chess_set_1k.gltf"])
    # This recipe deliberately covers this pinned export, not arbitrary glTF.
    if len(document["buffers"]) != 1:
        raise ValueError("Expected one upstream geometry buffer")
    buffer = document["buffers"][0]
    binary = bytearray(resources[buffer.pop("uri")])
    if len(binary) != buffer["byteLength"]:
        raise ValueError("Geometry buffer length mismatch")
    if any(view["buffer"] != 0 for view in document["bufferViews"]):
        raise ValueError("Unexpected geometry buffer reference")

    for image in document["images"]:
        if image.get("mimeType") != "image/jpeg" or "bufferView" in image:
            raise ValueError("Expected external JPEG images")
        content = resources[image.pop("uri")]
        binary.extend(b"\0" * (-len(binary) % 4))
        image["bufferView"] = len(document["bufferViews"])
        document["bufferViews"].append({
            "buffer": 0, "byteOffset": len(binary), "byteLength": len(content),
        })
        binary.extend(content)

    buffer["byteLength"] = len(binary)
    binary.extend(b"\0" * (-len(binary) % 4))
    payload = json.dumps(document, ensure_ascii=True, separators=(",", ":")).encode("utf-8")
    payload += b" " * (-len(payload) % 4)
    result = (struct.pack("<III", 0x46546C67, 2, 28 + len(payload) + len(binary))
              + struct.pack("<I4s", len(payload), b"JSON") + payload
              + struct.pack("<I4s", len(binary), b"BIN\0") + binary)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="explicitly regenerate the pinned GLB")
    args = parser.parse_args()
    provenance = json.loads((ROOT / "provenance.json").read_text())
    expected = provenance["derived"]
    content = pack()
    if (len(content) != expected["bytes"]
            or hashlib.sha256(content).hexdigest() != expected["sha256"]):
        raise ValueError("Packed output differs from the pinned derivative")
    output = ROOT / expected["path"]
    if args.write:
        output.write_bytes(content)
    elif output.read_bytes() != content:
        raise ValueError("Stored GLB differs from reproducible output")
    print(f"Verified 11 upstream files and reproducible GLB: {len(content):,} bytes")


if __name__ == "__main__":
    main()
