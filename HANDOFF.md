# Handoff — fidelity, transparency and kinematics work

Working notes for whoever picks this up next. Written 2026-08-16, against a clean
tree on `main` at `58394c4` plus the uncommitted changes described below.

---

## The goal this work is serving

A sprite that reads like [`assets/cat_walking_1.gif`](assets/cat_walking_1.gif)
— but with a transparent background, and with configurable placement and motion:
top, bottom, an explicit `(x, y)` or `(x, y, z)`, constrained by what the entity
can physically do. The sun may drift anywhere; the cat and crab are bound by
gravity.

A five-step plan came out of the review. **Steps 1 and 2 are done and verified.
Steps 3–5 are not started.**

| Step | What | Status |
|---|---|---|
| 1 | Correctness bugs: flip, asset fallback, Lua/Rust physics divergence | **done** |
| 2 | Per-frame buffer cache + genuine in-terminal transparency | **done** |
| 3 | `locomotion` + `position` schema, per-asset capability gating | not started |
| 4 | Silhouette-first art redo, quantised palette | not started |
| 5 | Kitty graphics-protocol backend | not started |

---

## Verify the current state

All four gates pass. Run them before and after any change.

```bash
nvim --headless -u tests/minimal_init.lua -c "luafile tests/run_tests.lua"
```

```bash
cargo test --manifest-path engine/Cargo.toml
```

```bash
stylua --check lua plugin tests
```

```bash
cargo clippy --manifest-path engine/Cargo.toml --all-targets -- -D warnings
```

Expected: **145 Lua tests** (was 113 before this work), **102 Rust tests**
(95 lib + 6 headless GPU + 1 screenshot; `parity_dump` is `#[ignore]`).

`luacheck` is listed in the README as a gate but **is broken on this machine** —
it fails to load under the installed Lua 5.5 (`luacheck/builtin_standards/love.lua`
blows up inside `luarocks/loader.lua`). That is an environment problem, not a
code problem. CI may still run it; do not assume a green local run means
luacheck passed.

---

## Step 1 — what changed and why

### Sprites never mirrored in the terminal

`engine.lua` set `entity.flip_x`; `renderer.lua` never read it, so a cat walking
left was drawn facing right. Half of all locomotion looked wrong on the default
backend. The overlay had always done it correctly (`gpu.rs`, `entity.flip_x ^
anim.flip_x`).

- `terminal_sprites.mirror_matrix(rows)` mirrors a pixel matrix, padding ragged
  rows first so mirrored art keeps its position inside its own bounding box.
- `get_rendered_frame(asset, frame, flip_x)` takes facing as part of the cache
  key. Mirroring the *rendered* output would mean reversing byte offsets in every
  highlight span on every draw; mirroring the matrix once is free after the first
  call.
- `renderer.resolve_flip(entity)` XORs entity heading with `animation.flip_x`,
  matching `build_instances`, so art authored facing left is not mirrored twice.

### Custom assets silently rendered as a cat

`terminal_sprites.load_sprite` fell back to the cat module for any unknown name,
with no warning. The `my_pet` example in the README is written against
`backend = "halfblock"` and produced a cat.

Resolution order is now: registered sprite set → built-in module →
`require("distract.sprites.<name>")` on the runtimepath → cat, **with a warning
emitted once per asset** (once per asset, not per draw — an unknown asset is
asked for at 30 FPS).

New public API, matching `future.md` §2.1:

```lua
require("distract").register_asset("my_pet", {
  manifest = { ... },
  sprites  = { frames = ..., layout = ..., width = n, height = n },
})
```

`engine.spawn` also warns when it falls back to the cat *manifest*.

### The two engines ran different physics

Despite both file headers claiming "one manifest describes one behaviour on both
backends":

| | Lua before | Rust |
|---|---|---|
| `wrap` | x only | x and y |
| `bounce` | x only, no edge transitions | x and y, fires `on_edge_left` / `on_edge_right` |
| `animation.flip_x` | ignored | XOR'd with heading |

Lua now matches Rust on all three.

### `accel_x` / `accel_y` were schema-only

Declared in `manifest.rs`, read by nothing, absent from Lua entirely. A manifest
could set them and watch them do nothing. Both engines now integrate them as
constant acceleration applied after the friction lerp toward `target_vx`.

Semantics, worth stating because they are a choice and not the only possible one:
**`gravity` is `accel_y` under a name that also brings a floor with it.**
`accel_y` is the floorless version. This matters for step 3 — ballistic and
omnidirectional locomotion both want one or the other.

### Spawn coordinates meant different things per backend

`spawn { x = 40 }` meant column 40 in the terminal and pixel 40 — roughly column
4 — on the overlay. **Spawn coordinates are now terminal cells on both
backends**; `external.lua` multiplies by `cell_size()` on the way out. Manifest
*velocities* remain sprite pixels per 60 FPS frame. Step 3 must not reintroduce a
third unit.

### `sprite_scale` was uniform

A sprite pixel is one cell wide and **half** a cell tall, so a single scale factor
is only correct on an exactly 2:1 cell. On a 16×36 HiDPI cell the overlay drew a
16px sprite 7.1 cells tall where the terminal drew 8.

`World.sprite_scale: u32` is gone, replaced by `sprite_scale_x: f32` /
`sprite_scale_y: f32` (`cell_w` and `cell_h / 2`). `Compositor::blend_sprite_ex`
takes `scale_x, scale_y`. Position integration uses `px` and `py` separately.

### Colourscheme change wiped every sprite colour

Found while working, not in the original review. `:colorscheme` runs `:hi clear`,
which deletes the ~1,900 generated `Distract_*` groups. `hl_cache` still believed
they existed, so every sprite rendered in the default foreground until restart.
A `ColorScheme` autocmd now calls `reset_highlights()` + `reset_cache()` and
re-declares the sprite background.

---

## Step 2 — what changed and why

### Per-frame buffers

A frame's content is immutable, but writing one cost `nvim_buf_set_lines` +
`nvim_buf_clear_namespace` + one `nvim_buf_set_extmark` per coloured cell —
**~92 API calls per entity per frame change**. Measured maxima: cat 93, crab 89,
sun 99 extmarks per frame. A cat sprinting at 12 FPS spent ~1,100 calls a second
redrawing pictures it had already drawn.

Each `(asset, frame, facing)` now gets one scratch buffer, populated once, with
extmarks baked in. Entities showing the same frame share the buffer. Advancing
the animation is a single `nvim_win_set_buf`.

Measured over a warm walk cycle: **0 extmarks, 0 line writes, 1 buffer swap.**

`renderer.close_window` no longer deletes the buffer — frame buffers outlive any
one window. `terminal_sprites.reset_cache` owns their lifetime. A cached handle
is checked with `nvim_buf_is_valid` before use, since a user may `:bwipeout`
anything.

### Genuine transparency

Two surfaces, because neither alone can do it:

- A float paints **every** cell it covers, transparent ones included, so a
  sprite-sized float blanks a sprite-sized rectangle of your code. Measured: a
  buffer cell reading `E` reads ` ` once a float is over it.
- Overlay virtual text touches only the cells it is given, but **cannot be placed
  where there is no buffer line** — which is exactly where a pet usually walks.

They are complementary, so the renderer uses both:

| Sprite rows | Surface |
|---|---|
| over buffer text | overlay extmarks, `virt_text_win_col` |
| past the last line | float, `Normal` = `bg=NONE` |

Result over a screen of code:

```
overlay_rows=8 float_rows=0 marks=11 | sprite cells drawn=83 | buffer cells destroyed=0

 6 |ABCDEFGHIJABCDEFGHIJA▄▀▄E▄▀▄IJABCDEFGHIJ|
 7 |ABCDEFGHIJABCDEFGHIJA▀▀▀▀▀▀▀IJABCDEFGHIJ|
```

The `E` between the cat's ears survives. Previously the whole 24×8 rectangle went
blank.

Supporting pieces:

- `terminal_sprites.get_frame_runs(asset, frame, flip)` — a frame as per-row runs
  of *adjacent drawn cells*, merging neighbours that share a highlight. Padded
  lines cannot be used: a run of spaces would occlude exactly what it is meant to
  leave alone. A cat frame is ~11 extmarks, not ~90.
- `renderer` keeps a screen-row → buffer-line map, rebuilt only when a
  `getwininfo()` fingerprint changes. Building it costs a `screenpos` per visible
  line, which is not something to pay 30 times a second for a screen that has not
  scrolled.
- Everything is guarded by a signature over
  `(frame_buf, row, col, width, height, overlay_limit, screen_map_version)`. A
  stationary sprite costs **zero** API calls.

`virt_text_win_col` is used rather than `virt_text_pos = "overlay"` because the
latter needs the underlying line to be long enough to reach that column. It is
measured from the window's first *text* column, so `wi.textoff` (the gutter) has
to come out of the screen column.

**Known limits.** Two cases fall back to the float because a buffer line cannot
address them: continuation rows of a wrapped line, and folded lines. Only the row
a line *starts* on is mapped. `first_unmappable_row` treats the first failure as
the start of a tail handed to the float; an isolated failure mid-sprite therefore
costs a few rows of occluded text below it. That is never worse than the old
behaviour, where every row was occluded.

---

## Traps that cost time — read before debugging

1. **`vim.fn.screenstring` lies inside `nvim -l` scripts.** It reads the current
   window's grid, not the composited screen, so floating windows appear at the
   wrong place or not at all. I read this as a float-positioning bug and chased
   it for a while. A **vanilla** float at `row=12, col=10` reproduces the exact
   same artifact, while `nvim_win_get_position` correctly reports `{12, 10}`.
   Attaching a real UI via a pty does **not** fix it.
   - Assert on `nvim_win_get_position` / `nvim_win_get_config` for float rows.
   - `screenstring` **is** trustworthy for the extmark overlay path, because
     those are written into the current window's own buffer.

2. **`engine.setup` merges with `vim.tbl_deep_extend("force", ...)`.** Registering
   two test manifests under the same asset name lets the first one's `physics`
   fields survive into the second. `tests/engine_spec.lua` gives each test its own
   `probe_N` name for this reason. There is no way to *remove* a field via
   `setup`.

3. **Wall-clock `dt` in Lua engine tests.** `engine.tick()` derives `dt` from real
   elapsed time, so a tight loop of 20 ticks advances almost no simulated time.
   Assert on direction (`vx > 0`) against a zero-accel control, not on magnitude.

4. **1,909 global highlight groups** exist for the three built-in assets alone,
   created by `nvim_set_hl` and never released. Unbounded with community asset
   packs. Step 4's quantised palette should cut this by roughly 40×.

---

## New/changed API surface

**`lua/distract/terminal_sprites.lua`**
`has_sprite(name)` · `register(name, sprite)` · `reset_highlights()` ·
`mirror_matrix(rows)` · `get_rendered_frame(name, idx, flip_x)` (new arg) ·
`get_frame_runs(name, idx, flip_x)` · `get_frame_buffer(name, idx, flip_x)` ·
`frame_namespace()` · `reset_cache(name?)` (new optional arg)

**`lua/distract/renderer.lua`**
`resolve_flip(entity)` · `background_group()` · `refresh_highlights()` ·
`overlay_namespace()` · `invalidate_screen_map()` ·
`window_state(id)` now returns `{ row, col, width, height, buf, win, float_row,
float_height, overlay_limit, overlay_marks }` — `win` is `nil` when no float was
needed.

**`lua/distract/init.lua`**
`register_asset(name, { manifest, sprites })`

**`engine/src/ecs.rs`**
`World.sprite_scale` → `sprite_scale_x` / `sprite_scale_y`, both `f32`.

**`engine/src/compositor.rs`**
`blend_sprite_ex(..., scale_x, scale_y)`.

---

## Files touched

```
README.md                      transparency section; units; register_asset
engine/src/ecs.rs              per-axis scale, accel integration, 3 new tests
engine/src/gpu.rs              per-axis instance sizing
engine/src/compositor.rs       per-axis blit
lua/distract/engine.lua        vertical wrap/bounce, edge transitions, accel, warn on fallback
lua/distract/external.lua      spawn coords cells -> overlay pixels
lua/distract/init.lua          register_asset, ColorScheme autocmd
lua/distract/renderer.lua      screen map, overlay path, float tail, buffer swap
lua/distract/terminal_sprites.lua  mirror, registry, runs, frame buffers, hl reset
tests/transparency_spec.lua    NEW — 9 tests
tests/{engine,external,renderer,review_fixes,sprite_assets}_spec.lua  new coverage
tests/run_tests.lua            registers transparency_spec
```

**Nothing is committed.** 15 modified files + 1 new, ~1,400 insertions.

---

## Step 3 — locomotion and position (do this next)

Specified in `future.md` §4.2. Concrete shape:

```lua
physics = {
  locomotion = "grounded",     -- "grounded" | "ballistic" | "omnidirectional"
  path_type  = "lissajous",    -- "linear" | "sine" | "lissajous" | "bezier" | "orbital"
  path_params = { freq_x, freq_y, amp_x, amp_y, phase_delta },
}
```

```lua
require("distract").setup({
  position = { anchor = "bottom" },        -- "bottom" | "top" | { x = , y = }
})
```

Requirements the review turned up:

- `:DistractSpawn` currently **drops its opts** (`plugin/distract.lua` calls
  `distract().spawn(pet_type)` and never passes x/y). Surface them.
- Capability gating: the cat and crab must not accept `omnidirectional`; the sun
  must not be forced into `grounded`. Gate in the manifest loader on both sides so
  a bad manifest is an error, not silent nonsense.
- `ground_y` is currently just "wherever the entity spawned". An anchor system
  needs a real floor concept, recomputed on `VimResized`.
- There is **no z axis**. `z_index` is draw order only. Decide whether `z` means
  draw order, parallax scale, or both, before adding it to the schema.
- `path_type` today accepts only the literal string `"sine"`, hardcoded in both
  engines. Generalise once, in a way both can share.
- Keep the unit contract: **spawn/position in terminal cells, velocity in sprite
  pixels per 60 FPS frame.**

Also worth doing here: the Lua engine has **no quiescence check**. `ecs.rs` has
`is_quiescent`; `engine.lua:tick` returns early only when zero entities exist, so
a screen of sleeping cats still wakes the editor loop 30×/sec forever.

## Step 4 — art

Do **not** start by editing sprites. The same art exists twice — `lua/distract/sprites/*.lua`
and `engine/src/sprites/*.rs` — with **no automated parity test** between them
(`engine/tests/parity_dump.rs` is `#[ignore]`, a dev aid needing `DUMP_TO`).
Build the parity harness first or the two will drift the moment either is touched.
`future.md` §5.8 already names the tool: `validate_sprite_parity`.

The art problem itself: at 24×16 the sprite is 24 columns × **8 rows**, and
`sprite_gen.orb` spends five lighting terms (Lambert, rim, fill, specular, dither)
across a body twelve pixels wide. At that size **silhouette is the only thing that
reads** — the cat currently reads as a fox. Ears are 3-pixel stubs
(`cat.lua`, `EAR_HALF = {0,1,1}`), the four legs are identical capsules, whiskers
and muzzle are below the detail floor. Flat fills, a 1px dark contour and 2–3 tone
bands will read better *and* collapse the highlight-group count.

## Step 5 — kitty graphics backend

The only route to reference-GIF fidelity in-terminal: half-blocks give exactly two
vertical subpixels per cell. `kitty` / `ghostty` / `wezterm` are currently
`SUBSTITUTED_ALIASES` in `init.lua` that warn and resolve to `halfblock`.

**Check this before building it:** the overlay backend already decodes GIFs
(`engine/src/asset.rs`, `load_gif`). Pointing a manifest's `spritesheet.path` at
`assets/cat_walking_1.gif` should give reference fidelity on the overlay *today*,
with no new backend. If that covers the goal, step 5 may be unnecessary.

---

## Open questions for the owner

1. Does GIF-on-overlay cover the fidelity goal, or is an in-terminal
   graphics-protocol backend required?
2. Does `z` mean draw order, parallax, or both?
3. Should step 4 redo the crab and sun to match, or is the cat the priority?
