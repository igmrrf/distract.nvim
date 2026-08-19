"""Reading the `.rgba` sidecar `import_sprite` writes.

Mirrors `engine/src/bin/import_sprite/rgba_sidecar.rs` and
`lua/distract/native_sprite.lua`; a third reader exists here only so this tooling
can assert on imported pixels without a Rust or Neovim harness.
"""

import struct

HEADER_SIZE = 17
MAGIC = b"DRGB"
VERSION = 1
BYTES_PER_PIXEL = 4


def read(path):
    with open(path, "rb") as handle:
        blob = handle.read()

    if len(blob) < HEADER_SIZE:
        raise ValueError(f"{path}: truncated header")
    if blob[0:4] != MAGIC:
        raise ValueError(f"{path}: bad magic {blob[0:4]!r}")
    if blob[4] != VERSION:
        raise ValueError(f"{path}: unsupported version {blob[4]}")

    width, height, count = (struct.unpack("<I", blob[offset:offset + 4])[0] for offset in (5, 9, 13))
    frame_bytes = width * height * BYTES_PER_PIXEL
    expected = HEADER_SIZE + count * frame_bytes
    if len(blob) != expected:
        raise ValueError(f"{path}: declares {expected} bytes, has {len(blob)}")

    return {"width": width, "height": height, "count": count, "blob": blob, "frame_bytes": frame_bytes}


def frame_alphas(sidecar, index):
    start = HEADER_SIZE + index * sidecar["frame_bytes"]
    return sidecar["blob"][start + 3:start + sidecar["frame_bytes"]:BYTES_PER_PIXEL]


def is_blank(sidecar, index):
    return not any(frame_alphas(sidecar, index))
