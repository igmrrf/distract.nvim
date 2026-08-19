"""The codex-pets sprite layout, as its own runtime defines it.

Transcribed from the site's application bundle (`Ka`, `Vb`, `Id`, `ef`, and the
`Ce`/`Se`/`Cr` constants), which is the only place the row -> action mapping is
published: the per-pet API response and the downloadable `.codex-pet.zip` carry
no animation metadata at all. The mapping is fixed per `spriteVersionNumber`,
**not** per pet -- a pet described as throwing a Kamehameha uses the same rows as
a cat. `verify_layout.py` checks this table against real pixels.

Rows 9 and 10 (v2 only) are not animations. They are a 16-entry look-direction
lookup addressed as `row = 9 + index // 8, column = index % 8`, so they are
marked directional and are imported as frames without becoming a looping state.

See `docs/codex-pets-sprite-layout.md` for the prose version.
"""

CELL_WIDTH = 192
CELL_HEIGHT = 208
COLUMNS = 8

ROWS_PER_VERSION = {1: 9, 2: 11}

V1_ROWS = [
    ("idle", 6, False),
    ("running-right", 8, False),
    ("running-left", 8, False),
    ("waving", 4, False),
    ("jumping", 5, False),
    ("failed", 8, False),
    ("waiting", 6, False),
    ("running", 6, False),
    ("review", 6, False),
]

# v2 keeps every v1 row, adds the two directional rows, and uses a 7th idle cell
# (the site labels index 6 "Neutral look") that v1 sheets do not have.
V2_ROWS = (
    [("idle", 7, False)]
    + V1_ROWS[1:]
    + [("look-right-side", 8, True), ("look-left-side", 8, True)]
)

LAYOUTS = {1: V1_ROWS, 2: V2_ROWS}


def rows_for(sprite_version):
    if sprite_version not in LAYOUTS:
        raise ValueError(f"unknown spriteVersionNumber {sprite_version}")
    return LAYOUTS[sprite_version]


def version_for_rows(row_count):
    for version, rows in ROWS_PER_VERSION.items():
        if rows == row_count:
            return version
    raise ValueError(f"no sprite version has {row_count} rows")


def row_counts(sprite_version):
    return [frames for _, frames, _ in rows_for(sprite_version)]


def total_frames(sprite_version):
    return sum(row_counts(sprite_version))


def states_arg(sprite_version, include_directional=False):
    """`--states` value: `name:start-end` over the imported frame sequence."""
    parts = []
    cursor = 0
    for name, frames, directional in rows_for(sprite_version):
        start, end = cursor, cursor + frames - 1
        cursor += frames
        if directional and not include_directional:
            continue
        parts.append(f"{name}:{start}-{end}")
    return ",".join(parts)
