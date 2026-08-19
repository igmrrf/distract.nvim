# Importing sprite assets

How to turn real image material — a GIF, a folder of PNG frames, or a pre-packed
atlas — into an asset this plugin can draw, using the `import_sprite` CLI.

For the design behind it, see
[`superpowers/specs/2026-08-16-sprite-import-pipeline-design.md`](superpowers/specs/2026-08-16-sprite-import-pipeline-design.md)
and its [atlas addendum](superpowers/specs/2026-08-19-spritesheet-atlas-import-addendum.md).
For configuring the plugin once an asset exists, see
[`configuration.md`](configuration.md).

---

## What the importer produces

One run reads one source and writes three artifacts from the same decoded,
background-removed frame set, so the two pixel-accurate backends can never drift
apart:

| Artifact | Consumed by |
|---|---|
| `<name>_sheet.png` — RGBA spritesheet, grid-packed | overlay backend, via the compiled engine |
| `<name>_frames.rgba` — raw pixel sidecar | kitty backend, via `lua/distract/native_sprite.lua` |
| `<name>.lua` — manifest scaffold | you, after hand-tuning |

The halfblock backend is untouched by this: it draws the cell-grid art and never
reads the sidecar. That separation is enforced by a regression test.

Procedural assets (`cat`, `crab`, `sun`) are unaffected — this pipeline is for
new, real-footage- or artwork-sourced assets.

## Running it

Always run from the **repository root**. There is no root-level `Cargo.toml`, so
`--manifest-path` is what points cargo at the engine crate without changing what
relative paths mean.

```bash
cargo run --manifest-path engine/Cargo.toml --bin import_sprite -- --help-less-see-below
```

### Source modes — exactly one

| Flag | Meaning |
|---|---|
| `--gif <path>` | An animated GIF. Per-frame delays are read and become the manifest's `fps`. |
| `--frames <dir>` | A folder of PNG frames, ordered by filename. No timing data. |
| `--spritesheet <path>` | A single pre-packed atlas image, any format the `image` crate decodes (PNG, WebP, …). Requires `--cell` and `--row-counts`. |

### Every flag

| Flag | Default | Meaning |
|---|---|---|
| `--name <name>` | *required* | Asset name. Drives output filenames and the manifest module name. |
| `--out <dir>` | `assets/<name>` | Where the sheet and sidecar go. |
| `--manifest-out <path>` | `lua/distract/manifests/<name>.lua` | Where the scaffold goes. |
| `--states <name:start-end,…>` | one state, `default`, covering everything | Slices the frame sequence into named states. |
| `--bg-tolerance <0-1>` | `0.12` | Flood-fill colour-distance threshold. |
| `--cell <WxH>` | — | Atlas cell size, e.g. `192x208`. Only with `--spritesheet`. |
| `--row-counts <n,n,…>` | — | Frames actually used per atlas row, top to bottom. Only with `--spritesheet`. |

Supplying `--cell` or `--row-counts` without `--spritesheet` is an error, as is
naming more than one source. Failures are loud and specific — an unreadable
path, a GIF that will not decode, an empty frame set, an atlas whose dimensions
are not a whole number of cells, a row claiming more frames than there are
columns, or a padded frame over the 4096px budget.

### Example: an animated GIF

```bash
cargo run --manifest-path engine/Cargo.toml --bin import_sprite -- \
  --gif assets/source/cat_walking.gif \
  --name cat_walking \
  --states walk:0-31
```

### Example: a pre-packed atlas

```bash
cargo run --manifest-path engine/Cargo.toml --bin import_sprite -- \
  --spritesheet assets/codex_pets/sheets/super-saiyan-goku-combat.webp \
  --cell 192x208 \
  --row-counts 7,8,8,4,5,8,6,6,6,8,8 \
  --states idle:0-6,running-right:7-14,running-left:15-22,waving:23-26,jumping:27-31,failed:32-39,waiting:40-45,running:46-51,review:52-57 \
  --name goku \
  --out assets/goku
```

Row order becomes frame order (row 0's frames first, then row 1's), which is what
lets `--states` slice the result by index. Trailing cells beyond a row's count
are discarded rather than imported as blank frames.

For codex-pets sheets specifically, the grid and per-row counts are already known
and scripted — see [`codex-pets-sprite-layout.md`](codex-pets-sprite-layout.md)
and `tools/codex_pets/`.

## What happens to the pixels

**Background removal** runs per frame, independently, because a walk cycle can
change what touches the frame edge:

1. The four corner pixels are averaged into one reference background colour.
2. A flood fill spreads from all four corners; a pixel joins if its normalised
   RGB distance to the reference is within `--bg-tolerance`.
3. Filled pixels get *soft* alpha — `clamp((distance - tolerance) / 0.04, 0, 1)`
   — so the silhouette edge feathers instead of turning into a jagged cutout.
4. Pixels the fill never reaches stay fully opaque. A subject that happens to
   share a colour with the background mid-body is never eaten; only the
   connected exterior region is.

**Already-cutout frames are detected and skipped.** If all four corners have
alpha 0, the frame passes through untouched and the importer says so on stderr.
This matters: `remove_background` computes alpha from RGB distance and ignores
the alpha a pixel already has, so running it over art that is already cut out
would walk into the antialiased edge halo and overwrite correct edge alpha.
The check is per frame, so mixed sources behave correctly, and opaque-background
sources see no change.

A frame left fully transparent by background removal is a named warning pointing
at `--bg-tolerance`, never a silently shipped blank asset.

**Padding and packing.** Frames are padded to one shared canvas — the max width
and height across the set — each frame bottom-aligned and horizontally centred,
so ground contact stays on a fixed pixel row. Nothing is ever resampled: if your
source is 1920×1080 stills, you get 1920×1080 frames and a very large sheet.
Downscale before importing if you want smaller cells. The sheet is then packed
`columns = min(8, frame_count)`, `rows = ceil(frame_count / columns)`.

## The manifest scaffold

The scaffold fills in what is derivable and flags what is not:

- **Derived:** `frame_width`, `frame_height`, `columns`, `rows`, `path`,
  `native_path`, each state's frame indices, and `fps` when the source is a GIF
  carrying per-frame delays (otherwise `12.0`).
- **Placeholders you must tune:** `physics` (`target_vx`, `target_vy`,
  `wrap_mode`), `transitions`, `anchor`, `locomotion`, `capabilities`. These are
  game-design decisions and are not derivable from pixels.

State names that are not plain Lua identifiers — anything with a hyphen, a
leading digit, a space, or a Lua keyword — are emitted as bracketed keys
(`["running-right"] = { … }`) so the file always parses.

**The scaffold overwrites its destination.** When re-importing an asset that
already has a hand-tuned manifest, write the scaffold somewhere else and merge by
hand:

```bash
cargo run --manifest-path engine/Cargo.toml --bin import_sprite -- \
  --spritesheet … --name cat_walking \
  --out assets/cat_walking \
  --manifest-out /tmp/cat_walking_generated.lua

diff /tmp/cat_walking_generated.lua lua/distract/manifests/cat_walking.lua
```

Then copy across only the fields that actually changed — typically
`native_path` and the frame geometry.

Manifest field reference: [`configuration.md`](configuration.md#manifest-schema).

## The `.rgba` sidecar format

Deliberately trivial: no compression, no chunked container, so the Lua reader is
byte arithmetic rather than a parser. This repo targets LuaJIT/5.1, which has no
`string.pack`/`unpack`.

```
offset  size  field
0       4     magic "DRGB"
4       1     version (1)
5       4     frame_width   (u32, little-endian)
9       4     frame_height  (u32, little-endian)
13      4     frame_count   (u32, little-endian)
17      ...   frame_count x frame_width x frame_height x 4 bytes,
              RGBA8, non-premultiplied, row-major, no row padding,
              frames concatenated in order
```

Uncompressed is a deliberate trade: a 74-frame 192×208 sidecar is ~11.5 MB.
Three readers must stay byte-compatible —
`engine/src/bin/import_sprite/rgba_sidecar.rs` (writer, plus a test-only reader),
`lua/distract/native_sprite.lua` (runtime), and `tools/codex_pets/sidecar.py`
(tooling). A round-trip test pins the format.

At runtime a missing or malformed sidecar is an expected failure: `native_sprite.load`
returns `nil, err`, `sprite_sources` warns once per asset, and the asset falls
back to whatever art it would have had without a `native_path`. A bad asset file
never takes down the render loop.

## Verifying an import

```bash
# the CLI's own tests
cargo test --manifest-path engine/Cargo.toml --bin import_sprite

# does the generated manifest actually parse, and what states did it get?
nvim --headless -c "lua local m = dofile('/tmp/generated.lua'); local n = {} for k in pairs(m.states) do n[#n+1] = k end table.sort(n) print(table.concat(n, ', '))" -c "qa!"

# full gates
cargo test --manifest-path engine/Cargo.toml --all-targets --all-features
nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua
```
