# The codex-pets sprite layout

Reference for importing sheets from [codex-pets.net](https://codex-pets.net).
Established 2026-08-19 by reading the site's own application bundle and then
checking it against real pixels.

The headline: **actions are fixed per sprite version, not per pet.** A pet
described as throwing a Kamehameha occupies the same rows as a cat. What differs
between pets is the artwork, not the action set.

---

## Where the mapping comes from

Nothing per-pet publishes it. The API response
(`GET https://codex-pets.net/api/pets/<id>`) gives geometry but no animation
data:

```json
"validationReport": {
  "atlasSize": "1536x2288",
  "cellSize":  "192x208",
  "statesDetected": 11
}
```

The downloadable `<id>.codex-pet.zip` holds only `pet.json` (id, display name,
description, `spritesheetPath`, `kind`) and `spritesheet.webp` — no row or frame
metadata either.

The row → action mapping lives only in the site's frontend bundle, as the
constants `Ka` (v1 rows), `Vb` (the two extra v2 rows), `Id` / `Yb` (the combined
tables), `ef` (rows per version) and `Ce` / `Se` / `Cr` (cell width, cell height,
columns). It is transcribed into `tools/codex_pets/pet_layout.py`.

## Geometry

Constant across every sheet: **cell 192×208, 8 columns**.
`spriteVersionNumber` selects the layout; `kind` (`object` / `person` /
`creature`) has no effect on it.

| Version | Atlas | Grid | Rows | Frames |
|---|---|---|---|---|
| v1 | 1536×1872 | 8×9 | 9 | 57 |
| v2 | 1536×2288 | 8×11 | 11 | 74 |

## Rows

| Row | Action | v1 frames | v2 frames |
|---|---|---|---|
| 0 | `idle` | 6 | 7 |
| 1 | `running-right` | 8 | 8 |
| 2 | `running-left` | 8 | 8 |
| 3 | `waving` | 4 | 4 |
| 4 | `jumping` | 5 | 5 |
| 5 | `failed` | 8 | 8 |
| 6 | `waiting` | 6 | 6 |
| 7 | `running` | 6 | 6 |
| 8 | `review` | 6 | 6 |
| 9 | `look-right-side` | — | 8 |
| 10 | `look-left-side` | — | 8 |

```
v1 --row-counts 6,8,8,4,5,8,6,6,6
v2 --row-counts 7,8,8,4,5,8,6,6,6,8,8
```

v2's 7th idle cell is labelled "Neutral look" by the site and does not exist on
v1 sheets.

### Rows 9 and 10 are not an animation

They are a **16-entry look-direction lookup**, addressed as
`row = 9 + index // 8, column = index % 8`, running Up, Up-slight-right,
Up-right, … back around to Up-slight-left. Importing them as a looping state
would animate a pet through every compass direction. `pet_layout.py` marks them
directional and imports the frames without naming them as a state.

## Verified, not assumed

`tools/codex_pets/verify_layout.py` imports every scraped sheet claiming all 8
cells per row, then counts non-empty cells per row from the resulting `.rgba`
sidecar. Result over the 15 sheets in `assets/codex_pets/sheets/`:

```
15 sheets, 0 disagreeing with the published layout
```

Every v1 sheet measured `6,8,8,4,5,8,6,6,6`; every v2 sheet measured
`7,8,8,4,5,8,6,6,6,8,8`.

Also worth knowing: **every frame of every sheet is already alpha-cutout**, so
they all take the `is_already_cutout` pass-through path in the importer. Their
antialiased edges survive only because of that check.

## Importing one

Use the tooling, which fills in the grid and action names for you:

```bash
python3 tools/codex_pets/scrape_pets.py            # or point at existing sheets
python3 tools/codex_pets/import_pets.py
```

Or by hand — see [`importing-assets.md`](importing-assets.md) for the full flag
reference:

```bash
cargo run --manifest-path engine/Cargo.toml --bin import_sprite -- \
  --spritesheet assets/codex_pets/sheets/<id>.webp \
  --cell 192x208 \
  --row-counts 7,8,8,4,5,8,6,6,6,8,8 \
  --states idle:0-6,running-right:7-14,running-left:15-22,waving:23-26,jumping:27-31,failed:32-39,waiting:40-45,running:46-51,review:52-57 \
  --name <name> --out assets/<name>
```

## Known-bad reference

The atlas addendum's example command
(`--row-counts 7,8,8,4,5,8,6,6,6,6,8`, summing to 72) contradicts its own row
table and drops two real frames from row 9. The correct v2 value is
`7,8,8,4,5,8,6,6,6,8,8`, summing to 74.

## Licensing

These sheets are community uploads with no stated licence, and many depict
third-party characters. The site states it does not claim rights to them and
points takedown requests at its issue tracker. They are here as local test
material for the import pipeline. Do not ship them as plugin assets or
redistribute them.
