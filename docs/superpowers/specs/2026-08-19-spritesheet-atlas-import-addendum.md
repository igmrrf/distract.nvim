# Sprite import pipeline — pre-packed atlas addendum

Addendum to `docs/superpowers/specs/2026-08-16-sprite-import-pipeline-design.md`
and `docs/superpowers/plans/2026-08-16-sprite-import-pipeline.md`. Written
2026-08-19 against `fix/assets` after researching import of a third-party
sprite pack (`spritesheet_dog.webp`, dropped at repo root, untracked) against
the in-progress `import_sprite` CLI (Tasks 1–6 of the base plan are done as of
this writing: `decode.rs`, `background.rs`, `pack.rs`, `rgba_sidecar.rs`,
`manifest_scaffold.rs` all exist; Task 7's `run()` wiring and Tasks 8–12's Lua
side are not yet landed).

**Do not implement this addendum before Task 7.** It adds a second input path
that reuses Task 3/4/5/6's functions (`remove_background`,
`pad_to_common_canvas`, `grid_dimensions`, `pack_spritesheet`,
`write_rgba_sidecar`, `parse_states_arg`, `render_manifest`) — those must
exist and be wired through `run()` first. Land this as Task 13/14 after the
base plan's Task 12 (or in parallel once Task 7 merges, whichever the
implementing agent prefers) — it does not touch Tasks 8–12's Lua runtime
work (`backends.lua`, `native_sprite.lua`, `sprite_sources.lua`) at all; those
are orthogonal (runtime consumption of `.rgba`, not import-side decoding).

---

## Why

The base pipeline's two input modes (`--gif`, `--frames`) both assume **one
contiguous sequence of full-frame images** — a walk cycle, one subject per
frame, same logical canvas throughout. State boundaries within that sequence
are sliced by frame index (`--states walk:0-31`).

A common real-world sprite source doesn't look like that: it's a **single
pre-packed 2D atlas image**, one grid cell per pose, multiple unrelated
animations arranged as separate rows, each row a different length. Concretely,
`spritesheet_dog.webp` (1536×2288, confirmed via `sips`/pixel inspection):

- **Grid: 8 columns × 11 rows, cell size 192×208px** (`1536 / 8 = 192`,
  `2288 / 11 = 208` — both exact).
- **11 animation states, uneven frame counts per row**, trailing cells in
  short rows left fully transparent (not part of any animation):

  | Row | Frame count | Likely state |
  |---|---|---|
  | 0 | 7 | idle / face variants |
  | 1 | 8 | run cycle |
  | 2 | 8 | run cycle (variant) |
  | 3 | 4 | wave / paw gesture |
  | 4 | 5 | run, mouth open |
  | 5 | 8 | sit + sleep set (happy/worried/shy/sleeping/big-sleepy/worried/shy/happy) |
  | 6 | 6 | sit variations |
  | 7 | 6 | stand variations |
  | 8 | 6 | stand variations (2) |
  | 9 | 8 | stand, eyes-wide |
  | 10 | 8 | stand (final) |

  (72 real frames total; row/state naming is the importing author's call per
  the base spec's existing precedent — §2.4 of the base spec already treats
  `physics`/`transitions` as non-derivable placeholders, same principle
  applies to state names here.)
- **Already alpha-cutout.** Corners and inter-sprite space are `RGBA(0,0,0,0)`,
  not a solid opaque background color to flood-fill away. Frame edges already
  carry their own (slightly fringed) antialiasing baked into the alpha
  channel.

Two consequences fall out of this shape, addressed as two separate tasks
below.

## Non-goals

- Automatic grid/cell-size detection from pixel content. The grid dimensions
  and per-row frame counts are CLI input, same philosophy as `--states`
  already being author-supplied rather than inferred.
- Re-packing multiple *separate* atlas files into one sheet. One atlas file
  in, one asset out — matches the base spec's "one source, one asset, one
  run" non-goal.
- Video or multi-file atlas sources (e.g. one file per row). Single static
  image only.

---

## Task 13: `decode_spritesheet_grid` — slice a pre-packed atlas into frames

**Files:**
- Modify: `engine/src/bin/import_sprite/decode.rs`
- Modify: `engine/src/bin/import_sprite/main.rs` (new CLI flags, `run()` branch)

**New CLI flags** (mutually exclusive with `--gif`/`--frames`, extending the
existing "exactly one source" validation to a three-way choice):

| Flag | Meaning |
|---|---|
| `--spritesheet <path>` | A single pre-packed atlas image (any format `image` decodes — PNG, WebP, etc). |
| `--cell <WxH>` | Cell size in pixels, e.g. `192x208`. Required with `--spritesheet`. |
| `--row-counts <n,n,...>` | Frame count actually used in each row, left-to-right, top-to-bottom. Trailing cells beyond a row's count are discarded, not treated as blank frames. Required with `--spritesheet`. |

Row order in `--row-counts` becomes frame order in the output sequence
(row 0's frames first, then row 1's, etc.) — this is what lets the *existing*
`--states` flag (`parse_states_arg`, already built in Task 6) slice the result
into named states without any change to that function. Example invocation for
the dog asset:

```
cargo run --manifest-path engine/Cargo.toml --bin import_sprite -- \
  --spritesheet spritesheet_dog.webp \
  --cell 192x208 \
  --row-counts 7,8,8,4,5,8,6,6,6,6,8 \
  --name dog \
  --states idle:0-6,run:7-14,run_alt:15-22,wave:23-26,run_open:27-31,sit_sleep:32-39,sit:40-45,stand:46-51,stand_alt:52-57,stand_wide:58-65,stand_final:66-71
```

**Interfaces:**
- Produces: `pub fn decode_spritesheet_grid(path: &Path, cell_width: u32, cell_height: u32, row_counts: &[usize]) -> Result<Vec<DecodedFrame>, String>`. Returns frames in row-major, row-then-column order, `delay_ms: None` for every frame (no timing data in a static atlas — `average_fps` already falls back to a default of `12.0` when every `delay_ms` is `None`, so `run()` needs no change there).
- Consumes: nothing from other tasks; operates on a plain decoded `image::DynamicImage`.

**Behavior:**
1. Decode the source file with `image::open` (already how `decode_png_folder`
   opens files) and convert to RGBA8.
2. Validate `image.width() % cell_width == 0` and
   `image.height() % cell_height == 0` — fail loudly with the actual
   dimensions and requested cell size in the error message if not (this is
   exactly the kind of "author fat-fingered `--cell`" mistake that must not
   silently produce garbled frames).
3. Validate `row_counts.len() == image.height() / cell_height` — fail loudly
   on mismatch (wrong number of rows supplied).
4. Validate every entry in `row_counts` is `<= image.width() / cell_width` —
   fail loudly (can't claim more frames in a row than there are columns).
5. For each row, for each column index `0..row_counts[row]` (not
   `0..total_columns` — this is what drops the empty trailing cells),
   crop out the `cell_width × cell_height` sub-image at
   `(column * cell_width, row * cell_height)` using `image::imageops::crop_imm`
   and push it as a `DecodedFrame { image: cropped.to_image(), delay_ms: None }`.

**Tests** (colocated, same style as `decode_gif`'s fixture-based tests):
- A hand-built fixture atlas (e.g. 2×2 grid, 4×4px cells, each cell a distinct
  solid color) with `row_counts = [2, 1]` (second row's second cell
  deliberately a decoy color) decodes to exactly 3 frames, in row-major order,
  with the correct pixel content — proves both cropping math and that the
  dropped trailing cell is actually dropped.
- Mismatched `image.width() % cell_width != 0` is rejected.
- `row_counts.len()` not matching the actual row count is rejected.
- A `row_counts` entry exceeding the available columns is rejected.

**`main.rs` wiring:**
- Extend `Args` with `spritesheet: Option<PathBuf>`, `cell: Option<(u32, u32)>`,
  `row_counts: Option<Vec<usize>>`.
- `parse_args_from` gains a `--cell` parser (split on `x`, two `u32`s) and a
  `--row-counts` parser (split on `,`, `usize` each) — same pattern as the
  existing `--bg-tolerance` numeric parse.
- The "exactly one source" check becomes exactly one of three
  (`gif`/`frames_dir`/`spritesheet` — count how many are `Some`, require
  exactly 1), and `--cell`/`--row-counts` are required exactly when
  `--spritesheet` is given (fail fast if `--spritesheet` is set without them,
  or if either is set without `--spritesheet`).
- `run()`'s existing three-way match on `(&args.gif, &args.frames_dir)` grows
  a third arm calling `decode::decode_spritesheet_grid`.

---

## Task 14: skip background removal on already-cutout sources

**Files:**
- Modify: `engine/src/bin/import_sprite/background.rs`
- Modify: `engine/src/bin/import_sprite/main.rs` (`run()`)

**Why this is a real bug, not a nice-to-have:** `remove_background`
(`background.rs`, Task 3) computes a new alpha for every pixel it flood-fills
into, from RGB-distance-to-the-corner-color — it does not look at the
pixel's *existing* alpha at all. Feed it a frame that is already alpha-cutout
(corners `RGBA(0,0,0,0)`, antialiased edges already semi-transparent) and it
will flood-fill from those transparent corners (their RGB reads as pure
black, well within any reasonable `--bg-tolerance` of itself), walk into the
antialiased edge halo, and **overwrite the original, correct edge alpha**
with a recomputed value based on how close each edge pixel's RGB happens to
be to black — silently degrading exactly the pixels where source quality
matters most, on every asset imported this way, not just the dog sheet.

**Fix — detect and skip, don't add a new flag the author has to remember:**

```rust
pub fn is_already_cutout(frame: &RgbaImage) -> bool {
    corners(frame.dimensions().0, frame.dimensions().1)
        .iter()
        .all(|&(x, y)| frame.get_pixel(x, y)[3] == 0)
}
```

- Add this alongside `remove_background` in `background.rs` (reuses the
  existing private `corners()` helper — make it `pub(crate)` or inline the
  four-corner lookup; either is fine, keep it a one-line change).
- In `run()`, replace the unconditional
  `background::remove_background(&frame.image, args.bg_tolerance, FEATHER_BAND)`
  call with: if `background::is_already_cutout(&frame.image)`, pass the frame
  through unchanged; otherwise call `remove_background` as today. This is a
  per-frame check, not a per-source-type flag — correct behavior falls out
  automatically for `--gif`/`--frames` inputs that happen to already be
  alpha-cutout too, and existing opaque-background GIF/PNG-folder assets
  (`cat_walking`, and any future ones) see zero behavior change since their
  frame corners are opaque.
- Log (`eprintln!`) when a frame is passed through unchanged, at the same
  granularity the base spec already asks for around a fully-transparent
  frame warning (§4 of the base spec) — visibility into which path a given
  asset took, not a silent branch.

**Tests:**
- `is_already_cutout` returns `true` for a frame with `RGBA(0,0,0,0)`
  corners, `false` for a frame with opaque corners (reuse the existing
  `remove_background` test fixtures' frame-construction style).
- A `run()`-level integration test (extending Task 7's
  `a_full_run_produces_a_sheet_a_sidecar_and_a_manifest` fixture, or a
  sibling test) using a fixture frame with transparent corners AND a
  semi-transparent edge pixel at a known coordinate/alpha value, asserting
  that pixel's alpha is **unchanged** after `run()` — this is the actual
  regression this task exists to prevent, so the test must check a
  semi-transparent value survives exactly, not just that fully-transparent
  stays transparent (which would pass even with the bug, since 0 distance
  rounds to 0 either way).

---

## File inventory

**Changed (no new files):**
- `engine/src/bin/import_sprite/decode.rs` — `decode_spritesheet_grid` + tests
- `engine/src/bin/import_sprite/background.rs` — `is_already_cutout` + tests
- `engine/src/bin/import_sprite/main.rs` — `--spritesheet`/`--cell`/`--row-counts`
  flags, three-way source validation, `run()`'s decode branch and the
  cutout-check branch around the background-removal call

## Verification

Same commands as the base plan's Task 7/12:

```bash
cargo test --manifest-path engine/Cargo.toml --bin import_sprite
cargo fmt --manifest-path engine/Cargo.toml --all -- --check
cargo clippy --manifest-path engine/Cargo.toml --all-targets --all-features -- -D warnings
```

Plus a real end-to-end run against the actual dog asset once the above is
green, to confirm the invocation in Task 13 produces a sane
`assets/dog/dog_sheet.png` (visually: 72 frames packed 8-wide/9-tall per
`grid_dimensions(72)`, edges clean, no re-cutout haloing) before hand-tuning
`lua/distract/manifests/dog.lua`'s physics/transitions fields.
