"""The `legeling/awesome-codex-pet` gallery as a second source of test material.

A 198-pet community gallery using the same sprite layout as codex-pets.net, so
`pet_layout.py`, `import_pets.py` and `verify_layout.py` need no change. It is a
second source because it is reachable without the site's search API and because
its pets carry an explicit per-pet licence.

**Nothing here is redistributable.** Every pet checked is fan art of an existing
character under a non-commercial licence (`CC BY-NC 4.0` and similar), which is
why no codex-pets asset ships as a built-in — see `HANDOFF.md`. Sheets are
downloaded into a gitignored working directory as local test material only, and
the licence each pet declares is recorded in the catalogue so it travels with the
material.

Its `pet.json` carries no atlas metadata at all — only an id, a display name and
the spritesheet's filename — so unlike the site's API there is no published grid
to read. The geometry is derived instead: the cell size is fixed at 192x208 with
eight columns, and the row count is what identifies the sprite version.
"""

import json
import struct
import urllib.request

import pet_layout

CONTENTS_API = "https://api.github.com/repos/legeling/awesome-codex-pet/contents/pets"
RAW_BASE = "https://raw.githubusercontent.com/legeling/awesome-codex-pet/main/pets"
USER_AGENT = "distract.nvim-import-sprite-test/1.0"
TIMEOUT_SECONDS = 60


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as response:
        return response.read()


def webp_size(data):
    """The canvas size of a WebP file, from its header.

    Three encodings have to be handled because the gallery's sheets are not all
    produced by the same tool: extended (`VP8X`), lossy (`VP8 `) and lossless
    (`VP8L`). Reading the header rather than decoding the image keeps this
    dependency-free — the alternative is decoding two megabytes of pixels to
    learn two integers.

    Raises `ValueError` when the file is not a WebP this can measure, which is a
    reason to skip the pet rather than to guess its grid.
    """
    if len(data) < 30 or data[0:4] != b"RIFF" or data[8:12] != b"WEBP":
        raise ValueError("not a RIFF/WEBP file")

    chunk = data[12:16]
    if chunk == b"VP8X":
        width = int.from_bytes(data[24:27], "little") + 1
        height = int.from_bytes(data[27:30], "little") + 1
        return width, height

    if chunk == b"VP8 ":
        # The keyframe header carries the size 14 bytes into the chunk payload,
        # as two 16-bit values whose top two bits are the scale.
        (width_bits, height_bits) = struct.unpack_from("<HH", data, 26)
        return width_bits & 0x3FFF, height_bits & 0x3FFF

    if chunk == b"VP8L":
        bits = int.from_bytes(data[21:25], "little")
        return (bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1

    raise ValueError(f"unsupported WebP chunk {chunk!r}")


def grid_of(sheet_bytes):
    """The pet's atlas geometry, or a reason it cannot be used.

    Returns `(columns, rows, sprite_version, width, height)`.
    """
    width, height = webp_size(sheet_bytes)
    if width % pet_layout.CELL_WIDTH or height % pet_layout.CELL_HEIGHT:
        raise ValueError(
            f"{width}x{height} is not a whole "
            f"{pet_layout.CELL_WIDTH}x{pet_layout.CELL_HEIGHT} grid"
        )

    columns = width // pet_layout.CELL_WIDTH
    rows = height // pet_layout.CELL_HEIGHT
    if columns != pet_layout.COLUMNS:
        raise ValueError(f"{columns} columns, expected {pet_layout.COLUMNS}")

    # Raises for a row count no sprite version has, which is the whole point: an
    # unrecognised layout must not be imported against a guessed row mapping.
    sprite_version = pet_layout.version_for_rows(rows)
    return columns, rows, sprite_version, width, height


def list_pets():
    """Every pet directory in the gallery, newest listing order preserved."""
    listing = json.loads(fetch(CONTENTS_API))
    return [entry["name"] for entry in listing if entry.get("type") == "dir"]


def describe(slug):
    """A pet's licence and attribution, or an empty description.

    `submission.json` is where the licence lives; a pet without one is still
    usable as local test material but is recorded as unlicensed so nobody can
    mistake it for material that may be redistributed.
    """
    try:
        submission = json.loads(fetch(f"{RAW_BASE}/{slug}/submission.json"))
    except Exception:
        return {"license": None, "author": None, "source_type": None, "name": None}

    return {
        "license": submission.get("license"),
        "author": submission.get("author"),
        "source_type": submission.get("source_type"),
        "name": submission.get("name"),
    }


def matches(slug, terms):
    if not terms:
        return True
    return any(term.lower() in slug.lower() for term in terms)
