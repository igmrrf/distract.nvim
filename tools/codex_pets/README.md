# codex-pets import tooling

Development-only scripts for exercising the `import_sprite` CLI against real
third-party spritesheets, from [codex-pets.net](https://codex-pets.net) or from
the [`legeling/awesome-codex-pet`](https://github.com/legeling/awesome-codex-pet)
gallery. Nothing here ships to plugin users and nothing here is imported by the
plugin or the engine — it is a test harness.

**None of the artwork is redistributable.** The site states no licence at all,
and every gallery pet checked is fan art of an existing character under a
non-commercial licence — the gallery records one per pet in `submission.json`, and
the scraper copies it into the catalogue so it travels with the material. This is
why no codex-pets asset ships as a built-in: the import pipeline is the feature,
the artwork is not.

Python 3 with the standard library only. No dependencies.

The two sources differ in one way that matters. The site publishes each pet's
atlas geometry in its API response, so the recorded grid is read. The gallery's
`pet.json` carries no atlas metadata at all — an id, a display name and the
spritesheet's filename — so the grid is derived from the sheet's own WebP header
against the fixed 192x208 cell, and the row count is what identifies the sprite
version. A sheet that is not a whole grid, or whose row count matches no version,
is skipped with the reason rather than imported against a guessed row mapping.
Everything downstream (`pet_layout.py`, `verify_layout.py`, `import_pets.py`) is
unchanged and does not know which source a sheet came from.

For what the layout means, read
[`../../docs/codex-pets-sprite-layout.md`](../../docs/codex-pets-sprite-layout.md).
For the importer itself, read
[`../../docs/importing-assets.md`](../../docs/importing-assets.md).

## Usage

Run from the repository root.

```bash
# 1. download sheets (default search terms, or pass your own)
python3 tools/codex_pets/scrape_pets.py
python3 tools/codex_pets/scrape_pets.py goku naruto cat dog

# 1b. or from the community gallery, which needs no search API. Terms are
#     substring filters on the pet's slug; --limit caps the download.
python3 tools/codex_pets/scrape_pets.py --source awesome --limit 6
python3 tools/codex_pets/scrape_pets.py --source awesome --limit 3 cat

# 2. check the published layout against the real pixels
python3 tools/codex_pets/verify_layout.py

# 3. import every sheet with its real grid and action names
python3 tools/codex_pets/import_pets.py
```

## Files

| File | Role |
|---|---|
| `pet_layout.py` | The row → action table for each `spriteVersionNumber`, transcribed from the site's own bundle. The single source of truth here. |
| `scrape_pets.py` | Queries `/api/pets?q=<term>`, records each pet's published geometry, downloads its sheet. |
| `verify_layout.py` | Imports every cell, counts non-empty cells per row, asserts they match `pet_layout.py`. |
| `import_pets.py` | Imports every sheet properly and reports frame counts, cutout path and blank frames. |
| `sidecar.py` | Reads the `.rgba` sidecar. Third implementation of that format, alongside the Rust writer and the Lua runtime reader — keep all three byte-compatible. |
| `paths.py` | Where things live. Override the root with `CODEX_PETS_WORK`. |

## Where output goes

| Path | Contents | Size |
|---|---|---|
| `assets/codex_pets/sheets/` | Downloaded `.webp` sheets plus `catalogue.json` | ~34 MB / 15 sheets |
| `assets/codex_pets/imported/` | Per-pet `_sheet.png`, `_frames.rgba`, manifest scaffold | ~202 MB / 15 pets |
| `assets/codex_pets/verify/` | Throwaway output from `verify_layout.py` | ~250 MB |

`imported/` and `verify/` are **derived** — regenerable from `sheets/` by re-running
the scripts. The `.rgba` sidecar is uncompressed by design (~11.5 MB for a
74-frame v2 pet), so these directories are large. Think before committing them.

## Licensing

The downloaded sheets are community uploads with no stated licence and many
depict third-party characters. Local test material only — not plugin assets, not
for redistribution.
