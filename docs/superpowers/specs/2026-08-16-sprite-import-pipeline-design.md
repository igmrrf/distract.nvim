# High-fidelity sprite import pipeline

Design for consuming real image/GIF material as sprite assets, alongside the
existing procedural generator. Written 2026-08-16 against `fix/assets` after
the sprite-quality review fixes (Zzz regression, orb fill-light parity, dead
`specular` field, doc restoration — see the corresponding commits on this
branch).

---

## Why

Procedural generation (`sprite_gen.rs` / `sprite_gen.lua`) shades ellipses —
`orb` (continuous Lambertian) and `cel_orb` (flat shadow/base/highlight bands).
That is a hard ceiling: no amount of constant-tuning turns a shaded ellipse
into a detailed, photorealistic sprite. `assets/cat_walking/` already
demonstrates the alternative — a real spritesheet, sourced from footage —
consumed today only by the overlay/GPU backend. This spec turns that one
hand-built example into a repeatable, tested pipeline, and extends real
fidelity to the kitty backend, which is capable of it but currently unwired.

## Goals

- A standalone CLI (`import_sprite`) that turns a source GIF or a folder of
  PNG frames into: a background-removed, packed spritesheet PNG (overlay
  backend), a raw-pixel sidecar (kitty backend), and a manifest scaffold.
- Real (non-GIF-palette-limited) fidelity on every backend actually capable of
  displaying it: overlay (already works, unchanged) and kitty (new).
- Zero behavior change for the halfblock backend and for every existing
  procedural or GIF-backed asset. This is additive.

## Non-goals

- Replacing procedural generation for `cat`, `crab`, `sun`. They stay as they
  are; this pipeline is for new, real-footage-sourced assets.
- Raising halfblock fidelity. It is bounded by the character-cell grid — no
  pipeline changes that.
- Verifying the kitty backend's baseline on-screen rendering, or changing
  when/whether it registers itself. HANDOFF.md already tracks its on-screen
  behavior as unverified; this spec gives kitty a fidelity upgrade path for
  when it is active, but does not touch its conditional self-registration
  (§ 3.1) or re-litigate whether it draws correctly today.
- Video (mp4/webm) input. GIF and a PNG-frame folder only.
- Compression of the `.rgba` sidecar. Uncompressed, deliberately simple; if
  repo size becomes a real problem later, that is a separate, scoped change.
- Batch import of multiple assets in one CLI invocation. One source, one
  asset, one run — matches how `cat_walking` was authored.

---

## 1. Architecture

```
source (GIF or PNG folder)
        │
        ▼
  import_sprite CLI (Rust, engine/src/bin/import_sprite)
    1. decode frames (image crate)
    2. flood-fill background removal, per frame, from the four corners
    3. pad frames to one common bounding box (anchored bottom-center)
    4. pack into a grid spritesheet
        │
        ├──► assets/<name>/<name>_sheet.png    (RGBA spritesheet, grid-packed)
        ├──► assets/<name>/<name>_frames.rgba  (same frames, raw pixel sidecar)
        └──► lua/distract/manifests/<name>.lua (scaffold: derived fields filled,
                                                  game-design fields flagged
                                                  for the author to tune)

runtime:
  overlay backend   → engine/src/asset.rs::load_spritesheet reads *_sheet.png
                      (already built and tested — unchanged by this spec)
  kitty backend     → new lua/distract/native_sprite.lua reads *_frames.rgba
  halfblock backend → unchanged: procedural draw, or the existing direct-GIF
                      decode path, whichever the asset already resolves to
```

One CLI run produces every runtime artifact from the same decoded,
background-removed frame set — no second authoring pass, no risk of the two
real-fidelity backends drifting apart from each other.

## 2. Import CLI

### 2.1 Invocation

Run from the **repository root** (there is no root-level `Cargo.toml`, so
`--manifest-path` is what points cargo at the engine crate without `cd`-ing
into it and changing what every relative path below resolves against):

```
cargo run --manifest-path engine/Cargo.toml --bin import_sprite -- \
  --gif assets/source/cat_walking.gif \
  --name cat_walking \
  --states walk:0-31 \
  --out assets/cat_walking
```

All paths — input, `--out`, and the manifest destination — are repo-root-
relative, matching every path string already written into `cat_walking.lua`
today. `--out` defaults to `assets/<name>`; the manifest defaults to
`lua/distract/manifests/<name>.lua`. Both are overridable, primarily so tests
can point them at a scratch directory instead of writing into the real repo
tree.

| Flag | Meaning |
|---|---|
| `--gif <path>` | GIF source. Mutually exclusive with `--frames`. |
| `--frames <dir>` | A folder of numbered PNG frames, sorted by filename. |
| `--name <name>` | Asset name; drives output paths and the manifest module name. |
| `--states name:start-end[,...]` | Slices the frame range into named states. Optional — defaults to one state, `default`, covering every frame. State boundaries are the author's call, not derivable from pixels. |
| `--out <dir>` | Output directory for the spritesheet + sidecar. Defaults to `assets/<name>`. |
| `--manifest-out <path>` | Manifest destination. Defaults to `lua/distract/manifests/<name>.lua`. |
| `--bg-tolerance <0-1>` | Flood-fill color-distance threshold. Default `0.12`. |

### 2.2 Background removal

Per frame, independently (a walk cycle can shift what touches the frame edge):

1. Sample the four corner pixels, average them into one reference background
   color.
2. Flood-fill outward from all four corners. A pixel joins the fill if its
   normalized RGB Euclidean distance to the reference is within
   `--bg-tolerance`.
3. Filled pixels get **soft alpha**, not a hard cut:
   `alpha = clamp((distance - tolerance) / feather_band, 0, 1)`, with
   `feather_band = 0.04`. This feathers the silhouette edge instead of
   producing a jagged binary cutout.
4. Pixels the flood fill never reaches stay fully opaque, regardless of color
   — a subject that happens to share a color with the background mid-body is
   never eaten, only the connected exterior region is.

### 2.3 Padding and packing

- Bounding box: GIF frames already share one logical-screen size from the
  container format. A PNG-frame folder is padded to the max width/height found
  across all input frames; each frame is bottom-aligned and horizontally
  centered within that shared canvas, so ground contact stays at a fixed pixel
  row across frames whose original art varied in height (a crouch vs. a jump,
  say). This is a packing detail, distinct from the manifest's `anchor` field
  below, which positions the whole entity relative to the floor.
- Grid: `columns = min(8, total_frames)`, `rows = ceil(total_frames / columns)`,
  packed edge-to-edge — same convention `export_sprites`' spritesheet already
  uses.

### 2.4 Manifest scaffold

Written once to `lua/distract/manifests/<name>.lua`, meant to be hand-edited
afterward. Frame layout is derived; physics/transitions are generic
placeholders the author tunes per asset:

```lua
local M = {
  name = "cat_walking",
  asset_type = "sprite",
  spritesheet = {
    path = "assets/cat_walking/cat_walking_sheet.png",
    native_path = "assets/cat_walking/cat_walking_frames.rgba",
    frame_width = 128,
    frame_height = 72,
    columns = 8,
    rows = 4,
  },
  anchor = "bottom", -- one of position.lua's accepted values: auto, bottom, top, free, or {x,y,z}
  initial_state = "walk",
  locomotion = "grounded",
  capabilities = { locomotion = { "grounded" } },
  states = {
    walk = {
      animation = {
        frames = { 0, 1, --[[ ... ]] 31 },
        fps = 12.0, -- derived: 1000 / average source frame delay, when available
        loop_anim = true,
        flip_x = false,
      },
      physics = { target_vx = 2.0, target_vy = 0.0, wrap_mode = "wrap" }, -- placeholder: tune per asset
      transitions = { on_event = { idle = "idle" } },
    },
  },
}
return M
```

`fps` is genuinely derived when the source is a GIF with per-frame delay
metadata; `physics`/`transitions` are not derivable from pixels and are
generated as a documented starting point, not a guess presented as fact.

### 2.5 The `.rgba` sidecar format

Deliberately trivial — no compression, no chunked container:

```
offset  size  field
0       4     magic "DRGB"
4       1     version (1)
5       4     frame_width  (u32, little-endian)
9       4     frame_height (u32, little-endian)
13      4     frame_count  (u32, little-endian)
17      ...   frame_count × frame_width × frame_height × 4 bytes,
              RGBA8, non-premultiplied, row-major, no row padding,
              frames concatenated in order
```

Read by ~30 lines of Lua (`string.byte` + manual little-endian assembly — this
repo targets LuaJIT/5.1, which has no `string.pack`/`unpack`), not a general
parser. Written by Rust (`u32::to_le_bytes`) with a matching round-trip test.

## 3. Runtime wiring

### 3.1 `backends.lua`

Add a third capability field alongside `scale`/`alpha`:

```lua
---@field native_resolution boolean
```

| Backend | scale | alpha | native_resolution |
|---|---|---|---|
| halfblock | false | "cell" | false |
| overlay | true | "pixel" | true |
| kitty | true | "pixel" | true |

**Kitty is not a hardcoded built-in and this does not change that.** Unlike
halfblock/overlay, kitty registers itself dynamically and conditionally —
`kitty/init.lua`'s `M.setup()` calls `backends.register(M.NAME,
M.CAPABILITIES, M.ALIASES)` only after confirming the terminal actually
supports the graphics protocol (`M.is_available()`). Until that succeeds,
`BUILT_IN_SUBSTITUTIONS`' `kitty → halfblock` entry is the correct, deliberate
fallback, not a gap this spec closes. The only change here is adding
`native_resolution` to the capability schema itself: `BUILT_IN_CAPABILITIES`
gets the field for halfblock and overlay (the two real built-ins), `M.register`
requires it from every caller, and `kitty/init.lua`'s own `M.CAPABILITIES`
table gets `native_resolution = true` so that when kitty *does* register
itself, it reports the field correctly.

Overlay's `native_resolution = true` is recorded for schema completeness and
for any future code that queries backend capabilities generically — it is
**not** consumed by `native_sprite.lua` or by § 3.2's branch. The overlay
backend never calls `sprite_sources.get_pixel_frames` at all: sprites for
that backend are drawn by the compiled Rust engine over IPC, which reads its
own spritesheet directly via `asset.rs` (§ 3.5). Only kitty's
`native_resolution = true` actually changes what `get_pixel_frames` returns —
and § 3.2's two kitty call sites pass that literal directly rather than
looking it up, since they are kitty's own internal modules and looking it up
via `backends.lua` would require a circular `require` back into
`kitty/init.lua`.

### 3.2 `sprite_sources.get_pixel_frames`

`load_sprite(asset_name)` (`sprite_sources.lua:180-203`) caches its resolved
sprite in `sprite_cache[asset_name]` — keyed by asset name **only**. Naively
branching inside that cached path on an `opts.native_resolution` flag would
mean whichever backend calls `get_pixel_frames` first for a given asset wins
the cache entry for every backend after it — halfblock could end up served
native frames, or kitty stuck with the tiny matrix, depending on call order.
That cache must not learn about native resolution at all.

Instead, native resolution is resolved as a fourth source, ahead of and
independent from `load_sprite`'s cache:

- `M.bind_manifest(asset_name, manifest)` (`sprite_sources.lua:84-93`), which
  already resolves `gif_sprite.source_of(manifest)` into `gif_sources`, also
  resolves `native_sprite.source_of(manifest)` into a parallel
  `native_sources[asset_name]` table.
- `get_pixel_frames(asset_name, opts)` gains an optional second parameter.
  When `opts.native_resolution` is true **and** `native_sources[asset_name]`
  is set, it loads via `native_sprite.load(native_sources[asset_name].native_path)`
  and returns those frames directly — `load_sprite`'s cache is never
  consulted for this asset on this call. Otherwise (`opts.native_resolution`
  false, or no native source): **byte-for-byte unchanged**, falls through to
  today's `load_sprite` precedence chain.
- `native_sprite.lua` keeps its own cache, keyed by file path (§ 3.3) — decode
  cost is still paid once, just in a cache that can't leak across backends.

There are four existing call sites, all needing the new second argument —
this is a shared function, not two separate ones per backend:

| Call site | Backend | Purpose |
|---|---|---|
| `renderer.lua:434` | halfblock | frame count (loop bounds) |
| `terminal_sprites.lua:179` | halfblock | actual per-frame pixel content |
| `kitty/renderer.lua:169` | kitty | frame count (loop bounds) |
| `kitty/frames.lua:110` | kitty | actual per-frame pixel content |

Each passes its own backend's capability (looked up once via `backends.lua`,
not re-derived per call) — halfblock's two call sites always pass
`native_resolution = false`, kitty's two always pass `true`.

### 3.3 `native_sprite.lua` (new)

```lua
---@param manifest table
---@return table|nil source  -- same shape gif_sprite.source_of returns
function M.source_of(manifest) end

---@param path string
---@return table[] frames  -- parsed once, cached by path
function M.load(path) end
```

`load` parses the header, validates magic/version, and produces the same
per-frame pixel-matrix shape the rest of the render pipeline already consumes
— nothing downstream needs to know a frame came from a `.rgba` sidecar rather
than a GIF or procedural draw. Parsed results are cached in a module-level
table keyed by path, decoded once per asset.

### 3.4 `kitty/frames.lua` / `protocol.lua`

`kitty/frames.lua` needs the mechanical change from § 3.2 (pass
`native_resolution = true` at its `get_pixel_frames` call site) but its
placement/transmission *logic* is expected to need no further change —
`protocol.lua` already transmits arbitrary-resolution RGBA (`f=32`, raw
pixels) and relies on the terminal's own `c=`/`r=` placement to resample to
the on-screen cell footprint. Only the source data's resolution changes; the
transmission and placement mechanics are already resolution-agnostic. This
gets a characterization test in the implementation plan (§ 5) to confirm
before relying on it — if it turns out `frames.lua` silently assumed
tiny-matrix-sized input somewhere, `protocol.lua` is the fallback place that
would need a real change.

### 3.5 Overlay backend

No changes. `engine/src/asset.rs::load_spritesheet` already decodes and slices
a PNG spritesheet, cached by manifest hash, tested
(`a_real_spritesheet_slices_into_the_declared_grid`).

### 3.6 Halfblock backend

No changes, by construction of § 3.2's `opts.native_resolution = false`.

---

## 4. Error handling

**Import CLI** — fails fast and loud on: an unreadable source path, a GIF
decode error, an empty decoded frame set, a padded frame size exceeding a
sane max-dimension budget. A frame left fully transparent by background
removal (misconfigured tolerance) is a named warning pointing at
`--bg-tolerance`, not a silently shipped blank asset.

**Runtime** — `native_sprite.load` follows this codebase's `nil, err`
contract for expected failures (never `error()`/throw) for a missing or
malformed `.rgba` (bad magic, truncated, wrong declared length). The caller
in `sprite_sources.get_pixel_frames` treats that exactly like
`load_gif_sprite`'s existing failure path (`sprite_sources.lua:150-162`):
warn once per asset (a new `warn_native_failure`, mirroring the existing
`warn_decode_failure` dedup-by-asset pattern at `sprite_sources.lua:133-147`)
and fall through to whatever `get_pixel_frames` would have returned without a
`native_path`. Never a crash: this is a Neovim plugin, and a bad asset file
must not take down the render loop.

## 5. Testing

**Rust** (`engine/src/bin/import_sprite/`, colocated `#[cfg(test)]`):

- Flood-fill against hand-built fixture pixels — known background color and
  known subject color, assert the resulting alpha mask and the soft-alpha
  feather band.
- Packing/grid math — frame count → columns/rows, edge-to-edge placement.
- `.rgba` writer/reader round-trip — write, read back, assert pixel-for-pixel
  equality with the input.
- One CLI integration test over a tiny fixture GIF (2-3 frames, a few pixels
  each), asserting output PNG dimensions, frame count, and `.rgba` header
  fields.

**Lua**:

- `native_sprite_spec.lua` — decode correctness against a hand-built minimal
  buffer (a few bytes, one pixel); a corrupted-header fallback test.
- `backends_spec.lua` — capability-shape test covering the new
  `native_resolution` field and kitty's registration.
- `sprite_assets_spec.lua` — a regression test asserting halfblock **never**
  receives native frames even when `native_path` is present on the manifest.
- A characterization test exercising `kitty/frames.lua` with a native-
  resolution-sized frame, confirming § 3.4's "no changes needed" holds before
  the implementation plan relies on it.

---

## File inventory

**New:**
- `engine/src/bin/import_sprite/main.rs`
- `engine/src/bin/import_sprite/background.rs` — flood-fill + soft alpha
- `engine/src/bin/import_sprite/pack.rs` — padding + grid packing
- `engine/src/bin/import_sprite/rgba_sidecar.rs` — `.rgba` writer (and reader, shared with tests)
- `engine/src/bin/import_sprite/manifest_scaffold.rs` — Lua manifest text generation
- `lua/distract/native_sprite.lua`
- `tests/native_sprite_spec.lua`

**Changed:**
- `engine/Cargo.toml` — new `[[bin]]` entry
- `lua/distract/backends.lua` — `native_resolution` capability, kitty registration
- `lua/distract/sprite_sources.lua` — `opts.native_resolution` threading
- `lua/distract/terminal_sprites.lua` — pass-through of `opts` to `get_pixel_frames`
- `lua/distract/renderer.lua`, `lua/distract/kitty/renderer.lua`, `lua/distract/kitty/frames.lua` — pass backend capability at all four existing `get_pixel_frames` call sites (§ 3.2)
- `tests/backends_spec.lua`, `tests/sprite_assets_spec.lua`, `tests/kitty_spec.lua` — capability + fallback + characterization tests

**Possibly changed** (confirm during implementation, § 3.4):
- `lua/distract/kitty/protocol.lua` — only if the characterization test in § 5 shows the placement/transmission mechanics assume the tiny procedural-sized frame after all
