# Handoff — fidelity, transparency and kinematics work

Working notes for whoever picks this up next. Rewritten 2026-08-16, against the
commit that landed P5.

The authoritative design is
[`docs/superpowers/specs/2026-08-16-locomotion-position-kitty-design.md`](docs/superpowers/specs/2026-08-16-locomotion-position-kitty-design.md).
This file says where the work stopped and what to watch out for; the spec says
what to build. Where they disagree, the spec wins — it has been corrected in
place as decisions were settled, and each such decision is recorded in a
"settled during implementation" subsection.

---

## The goal this work is serving

A sprite that reads like [`assets/cat_walking_1.gif`](assets/cat_walking_1.gif)
— transparent background, configurable placement and motion: top, bottom, an
explicit `(x, y)` or `(x, y, z)`, constrained by what the entity can physically
do. The sun may drift anywhere; the cat and crab are bound by gravity.

---

## Status

Two numbering schemes are in play. The original review produced **steps 1–5**;
the design doc replans the same ground as **phases P0–P5**. They are not the
same partition — the table maps both.

| Phase | Content | Step | Status |
|---|---|---|---|
| — | Correctness bugs: flip, asset fallback, physics divergence | 1 | **done** |
| — | Per-frame buffer cache, in-terminal transparency | 2 | **done** |
| P0 | Kitty protocol spike, throwaway | — | **done** (§ 7.3, § 7.4) |
| P1 | `dt` seam, parity harness, goldens | 3 | **done** |
| P2 | Locomotion, capabilities, paths, `on_land`, quiescence, spawn opts | 3 | **done** |
| P3 | Position, anchors, floor, `z`, backend capability table | 3 | **done** |
| P4 | Kitty backend | 5 | **done, unverified on screen** |
| P5 | GIF decoder, sprite wiring, halfblock quantiser | 5 | **done, unverified on screen** |
| — | Silhouette-first art redo | 4 | **not started** |

Next in line: step 4 (art), which needs the art-parity harness built first --
see "Step 4 — art" below -- and then whatever `future.md` planning selects.

---

## Verify the current state

All four gates pass. Run them before and after any change.

```bash
nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua
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

Expected: **325 Lua tests**, **145 Rust tests** (136 lib + 6 headless GPU + 2
parity + 1 screenshot; `parity_dump` is `#[ignore]`).

Note the Lua invocation: `-l` with `--noplugin -u tests/minimal_init.lua`. The
`-c "luafile tests/run_tests.lua"` form CI uses also passes -- both were checked
on 2026-08-16, so a previous note claiming the older form cannot resolve
`distract.engine` was wrong. What does fail is either form without
`-u tests/minimal_init.lua`, which is where the runtimepath is set.

`luacheck` is listed in the README as a gate but **is broken on this machine** —
it fails to load under the installed Lua 5.5. Environment problem, not a code
problem. CI may still run it; a green local run does not mean luacheck passed.

---

## The one thing P4 cannot tell you

**Nobody has watched a kitty placement render.** The backend is asserted byte
for byte — chunk boundaries, base64 payloads, diacritic encoding, `q=2` on
every command, `d=I` on every delete, one transmission per frame however many
entities show it — and it places through the same code path the half-block
renderer uses, with a test for that too. None of which is the same claim as *a
cat appears on the screen*.

The three ways it can be byte-correct and still wrong, in the order worth
checking:

1. **Neovim may not emit the placeholder unchanged.** `U+10EEEE` is plane-16
   private use; if Neovim gives it a width other than 1, or normalises the
   combining marks, the cells arrive scrambled.
2. **`vim.v.stderr` may interleave** with the TUI's own output under load. § 7.3
   measured that it *reaches* the terminal, not that a 4-chunk transmission
   survives arriving mid-frame.
3. **The float may cover the placeholders.** Rows below the last buffer line go
   to a float; its `Normal` has `bg = "NONE"`, but a terminal that paints its
   own background over a graphics placement would blank the sprite there and
   leave the buffer-overlay rows visible. A cat cut off at the waist is this.

The user has Ghostty. Get a human to look at the screen: `:DistractSpawn cat`
— since P5 the kitty backend is what a Ghostty session gets by default, so no
`:DistractBackend` call is needed; `:DistractBackend` still reports which one is
running. If it draws nothing at all, the first check is `:set termguicolors?` —
the backend declines without it and says so.

The same visit answers P5's open question: point a manifest at
`assets/cat_walking_1.gif` with `frame_width = 32, frame_height = 24` and see
whether an imported animation reads better than the procedural cat.

---

## The cross-engine parity harness — read this first

The recurring defect class in this project is `lua/distract/engine.lua` and
`engine/src/ecs.rs` drifting apart while both file headers claim "one manifest
describes one behaviour on both backends". Three such divergences had to be
found by reading before the harness existed; it has since caught two more on
its own (a stray `- 1` in the `clamp` ceiling, and the `ground_y` units bug).

- `engine/tests/physics_parity.rs` generates the goldens and asserts Rust still
  reproduces them.
- `tests/physics_parity_spec.lua` asserts the Lua engine reproduces the same
  numbers. Neither suite runs the other's toolchain; they meet at the JSON in
  `tests/fixtures/physics/`.
- Trajectories are stored in **terminal cells**. Lua integrates in cells, Rust
  in pixels, so dividing Rust x by `cell_w` and y by `cell_h` puts both in one
  frame with no fudge factor.

**Any change to physics on either side means adding a fixture.** Regenerate
after an intentional behaviour change:

```bash
UPDATE_GOLDEN=1 cargo test --manifest-path engine/Cargo.toml --test physics_parity
```

Then run the Lua suite. If it disagrees, that is the point — read the reported
step index before assuming the fixture is wrong.

Fixtures gained two optional fields in P3. `ground_row` (cells) is pushed into
the world *before* the spawn, so each engine derives the entity's own floor by
subtracting the sprite height in its own units — arithmetic written twice and
therefore worth pinning. `spawn.parallax` is applied to the entity *after* the
spawn, alongside the `path_phase` zeroing and for the same reason: a fixture
describes what the engine is given, not the `position` config and backend
capabilities that would have produced it.

Two fixtures deliberately avoid knife edges, and say so in their own
`description` field so nobody "fixes" them back: `constant_velocity_wrap` uses
`target_vx = 1.3` rather than a value that divides the width exactly, and
`path_bezier` uses `freq = 0.47` so the loop wrap never lands on a sample. In
both, f32 and f64 land either side of a discontinuity — a precision artefact of
two runtimes, not a behavioural divergence.

---

## Unit contract — load-bearing

- Positions (`x`, `y`, `ground_y`, path anchors) are in **terminal cells**.
- Velocities, accelerations and path amplitudes are in **sprite pixels per
  frame at 60 FPS**. One sprite pixel is one cell wide and half a cell tall.
- `z` is dimensionless.

Lua converts on integration (`CELLS_PER_SPRITE_PX_X = 1.0`, `_Y = 0.5`); Rust
multiplies by `sprite_scale_x = cell_w`, `sprite_scale_y = cell_h / 2`. A
*position* arriving from a manifest converts with `cell_w`/`cell_h`, not with
the sprite scale — getting that wrong is exactly the `ground_y` bug fixed in
`e70a53b`. `external.lua` owns the cells→pixels conversion at the IPC boundary.

**A kitty sprite occupies the same cells a half-block one does** — W columns by
H/2 rows. That is deliberate and load-bearing: fidelity comes from pixel density
inside the rectangle, not a bigger rectangle, so nothing above changes when the
backend does.

---

## The shape the next backend plugs into

From P2:

- `engine.step(dt, bounds)` — the pure simulation, with `dt` and screen size
  injected. `engine.tick()` measures `dt` and calls it. This is what makes the
  goldens possible.
- `engine.is_quiescent()` mirrors `World::is_quiescent`. It gates the **redraw
  only** — never the step. `ecs.rs` runs `World::update` unconditionally,
  because an entity can need a boundary wrap while not moving under its own
  power. Gating the step breaks vertical wrap; there is a test for it.
- Path primitives, `physics.locomotion`, `transitions.on_land`, and
  `manifest.capabilities.locomotion` validated once at load.

From P3:

- `lua/distract/position.lua` — anchors, both floors, and the parallax factor.
- `lua/distract/backends.lua` — the capability table.
- **Neither engine measures its own floor.** `events.sync_floor` measures once
  and pushes the same number to `engine.set_ground_row` and
  `external.set_ground_row`.
- `z` folds into `z_index` at spawn; `parallax` multiplies the displacement,
  the drawn size, and therefore the footprint the boundary modes and the floor
  measure.

From P4:

- **`distract.renderer` owns placement; a backend owns content.**
  `register_backend(name, build_surface, on_reset)` takes a provider returning a
  `DistractFrameSurface`: a buffer for the float, a `runs()` thunk for the
  overlay extmarks, a cell size, and a `key` that changes exactly when the
  picture does. Clamping, the overlay/float split and the
  zero-API-calls-while-stationary guard are inherited, not reimplemented. A
  fourth in-terminal backend is one more `register_backend` call.
- `lua/distract/screen_map.lua` — where a buffer line sits on the screen, with
  its own cache, invalidation and version counter. Both in-terminal backends
  consult it.
- `lua/distract/kitty/` — `protocol` (pure escapes), `writer` (the tty, with an
  injectable sink), `detect` (env fast path, `a=q` authority, fails closed),
  `frames` (RGBA plus a per-cell opacity mask), `renderer` (transmit-once and
  the surface), `diacritics` (generated data), `init` (registration).
- An asset may declare its own `anchor`. Precedence: the spawn or config, then
  the asset, then locomotion. The sun declares `"top"`.

### What P4 deliberately did not do

- **No GIF, no new art.** P4 is the transport. Whether the cat *reads* better
  in RGBA than in half-blocks is step 4's problem, and at 24×16 the answer is
  probably "not much" — see the art section below.
- **No placement ids.** Each distinct scaled rectangle is transmitted as its own
  image rather than as a second placement of one image, which would need the
  placement id encoded in the cell's underline colour. Distinct rectangles are
  bounded by the sprite's own size, so this does not grow without limit.
- **`engine.lua` is still over 900 lines** against a 400-line cap, with
  `M.spawn` and `M.step` well over the 60-line function cap. Owner's call,
  2026-08-16: leave it until the features are in, but no *new* file may break
  the standards. `renderer.lua` is 501.

---

## What P5 did

Spec § 8, in full, plus the two things it turned out to depend on.

- **`lua/distract/gif/`** — a pure-Lua decoder. `lzw` (variable-width codes,
  dictionary as `prefix`/`suffix`/`first` arrays), `parser` (block structure,
  interlace, palettes, disposal and delay extraction), `resample` (area-average
  to the sprite size), `sprite` (a manifest's declaration into a sprite set),
  `init` (composition, budgets, the public `decode`/`decode_bytes`).
- **`lua/distract/sprite_sources.lua`** — which art an asset has: a registered
  set, a bound GIF, or a procedural module. Split out of `terminal_sprites`,
  which was 506 lines before this work started.
- **`lua/distract/frame_buffers.lua`** — the scratch-buffer cache, likewise
  split out. `terminal_sprites` is 347 lines and back under the cap.
- **`lua/distract/quantise.lua`** — frequency-weighted median cut. Applied to
  imported art only, on the half-block path only.
- **`lua/distract/highlights.lua`** — groups are owned by the asset that asked
  for them, counted, and evicted least-recently-drawn-first at
  `max_highlight_groups`. Eviction clears the groups *and* drops the frames
  cached against them; the asset being drawn is never the victim.
- **`lua/distract/asset_path.lua`** — one answer to "relative to what?", now
  used by both the overlay's IPC payload and the in-terminal decoder.
- **Rust**: `load_gif` returns per-frame delays and resamples to the manifest's
  declared frame size; `LoadedAsset.frame_delays_ms` carries the timing;
  `frame_duration_seconds` in `ecs.rs` mirrors the Lua rule exactly.

### The precedence rules, both engines

- **Frame timing.** `animation.fps > 0` wins. Otherwise the source file's own
  per-frame delay. Otherwise 0.1s. `lua/distract/engine.lua`
  (`frame_duration_seconds`) and `engine/src/ecs.rs` (same name) must keep
  saying this identically — it is the newest member of the divergence class the
  parity harness exists for, and it is *not* covered by the physics fixtures.
- **Sprite size.** `spritesheet.frame_width`/`frame_height` are the size the art
  is *drawn* at, in sprite pixels, on every backend. Lua area-averages to it,
  Rust resizes with a triangle filter, so the two agree on footprint and differ
  slightly in colour — deliberate, and the reason the cap on the source canvas
  (`MAX_SOURCE_DIM` / `gif.MAX_CANVAS_DIM`, both 4096) is looser than the cap on
  the drawn frame.

### What P5 deliberately did not do

- **No decoding off the main loop.** `assets/cat_walking_1.gif` (1600x1200, 15
  frames) decodes in ~130ms and `cat_walking_2.gif` (32 frames) in ~375ms, once,
  on first draw. That is a visible hitch on the first frame of a large GIF and
  nothing more; chunking it across ticks would need a coroutine seam that
  nothing else wants yet.
- **No `image()`-style asset for the overlay's own GIF path.** Rust still
  decodes with the `image` crate; the Lua decoder is for the in-terminal
  backends. Two decoders, one contract, pinned by the precedence rules above.

## Step 4 — art

Do **not** start by editing sprites. The same art exists twice —
`lua/distract/sprites/*.lua` and `engine/src/sprites/*.rs` — with **no
automated parity test** between them. `engine/tests/parity_dump.rs` is
`#[ignore]` and dumps *geometry*, not physics; it is a dev aid needing
`DUMP_TO`, and it is **not** covered by the physics parity harness above.
Build an art parity harness first or the two will drift the moment either is
touched. `future.md` § 5.8 names the tool: `validate_sprite_parity`.

Owner's answer: the redo covers **every asset, existing and future** — not the
cat alone. That makes the art-parity harness a precondition rather than a
nicety, since three assets times two implementations is six files that can
drift.

The art problem itself: at 24×16 the sprite is 24 columns × **8 rows**, and
`sprite_gen.orb` spends five lighting terms (Lambert, rim, fill, specular,
dither) across a body twelve pixels wide. At that size **silhouette is the only
thing that reads** — the cat currently reads as a fox. Ears are 3-pixel stubs
(`cat.lua`, `EAR_HALF = {0,1,1}`), the four legs are identical capsules,
whiskers and muzzle are below the detail floor. Flat fills, a 1px dark contour
and 2–3 tone bands will read better *and* collapse the highlight-group count.

---

## Traps that cost time — read before debugging

1. **`vim.fn.screenstring` lies inside `nvim -l` scripts.** It reads the current
   window's grid, not the composited screen, so floating windows appear at the
   wrong place or not at all. A **vanilla** float at `row=12, col=10` reproduces
   the same artifact while `nvim_win_get_position` correctly reports
   `{12, 10}`. Attaching a real UI via a pty does not fix it.
   - Assert on `nvim_win_get_position` / `nvim_win_get_config` for float rows.
   - `screenstring` **is** trustworthy for the extmark overlay path, because
     those are written into the current window's own buffer.

2. **`engine.setup` merges with `vim.tbl_deep_extend("force", ...)`.**
   Registering two test manifests under the same asset name lets the first
   one's `physics` fields survive into the second. Every spec that builds
   probe manifests gives each test its own `probe_N` name for this reason.

3. **Wall-clock `dt` in `engine.tick()`.** A tight loop of 20 ticks advances
   almost no simulated time. Use `engine.step(dt, bounds)` for anything that
   asserts on distance; `tick` is only for testing the timer path.

4. **Test probes inherit the cat's manifest.** Both parity runners and several
   spec helpers start from `AssetManifest::default_cat()`. Since P2e the cat
   declares `capabilities` and a manifest-level `locomotion = "grounded"`, so a
   probe that orbits is *correctly* refused. Clear both fields on the probe —
   `manifest.locomotion = None; manifest.capabilities = Default::default();`.

5. **`vim.json.encode` writes an empty Lua table as `{}`, not `[]`.**
   `path_params.points` is the first array-valued manifest field, so the Rust
   deserialiser explicitly accepts both. Any future array-valued field needs
   the same treatment or it will parse in the terminal and fail on the overlay.

6. **1,909 global highlight groups** exist for the three built-in assets alone,
   created by `nvim_set_hl` and never released. Unbounded with community asset
   packs. Step 4's quantised palette should cut this by roughly 40×. Kitty adds
   one group per transmitted image, which is 174 for the built-ins at one depth
   — small beside that, but it is the same unbounded shape.

7. **Neither engine measures its own floor.** A change that has an engine call
   `position.floor_row` for itself reintroduces the divergence class this whole
   harness exists to catch. The one exception is a spawn naming its own
   `ground`, which is asking about a surface the pushed floor does not describe.

8. **`engine.lua` holds `floor_row` as module state**, so a spec that spawns
   after another spec's push inherits its floor. `tests/physics_parity_spec.lua`
   calls `set_ground_row(nil)` before every fixture. Any new spec that asserts
   on `ground_y` must do the same.

9. **`backends`, `position` and `distract.kitty` warn once, process-wide, and
   the registries are process-wide too.** `reset_warnings()`, `backends.reset()`
   and `kitty.reset()` exist for tests. A spec that registers kitty and does not
   put it back breaks `backends_spec`, which asserts the exact backend list; a
   spec that counts warnings without resetting counts zero, passes, and proves
   nothing.

10. **`vim.tbl_deep_extend` cannot set a field to nil**, so a placement-request
    helper built with it cannot express "no floor measured". `position_spec`
    assigns `request.floor_row = nil` after building instead.

11. **The kitty test seam is `writer.set_writer`, and every spec that uses it
    must put it back.** A leaked capture silently swallows every subsequent
    escape, and nothing fails — the assertions are all on what was captured.
    `tests/kitty_spec.lua` wraps it in `captured()` / `with_kitty()` for this
    reason; use those rather than calling `set_writer` directly.

12. **`detect.is_available()` answers once and caches.** Headless it is always
    false, because there is no UI to answer the query. `detect.override(true)`
    is how a test gets past that; `detect.reset()` puts it back.

13. **`kitty.reset()` now also unregisters the renderer surface.** It did not
    before P5, so a spec that registered kitty left `renderer.supports("kitty")`
    answering true for every spec that ran after it — with `distract.backends`
    already put back, which is precisely the on-paper-only backend the two
    registries are kept in step to prevent.

14. **A spawn randomises `frame_idx`, `frame_timer` and `path_phase`** so two
    entities spawned together do not move in lockstep. Any test that asserts on
    elapsed animation time has to zero the first two afterwards;
    `tests/gif_assets_spec.lua` has `spawn_at_first_frame()` for it.

15. **The harness compares with `vim.inspect`, which prints table identity.**
    `assert.are.same({ RED, RED }, row)` fails against a row of two equal but
    distinct tables, because the expected side prints `{ <1>{...}, <table 1> }`.
    Assert per pixel rather than per row.

16. **`config.backend` is `nil` by default now**, and nil means "pick the best
    this terminal can draw". A spec that wants the default path back has to
    assign `distract.config.backend = nil` before calling `setup()`, because a
    previous `setup()` resolved it to a concrete name and that is what "the user
    chose it" looks like from the inside.

---

## Open questions for the owner

1. ~~**Should Ghostty and kitty users get the kitty backend by default?**~~
   Owner: yes. Implemented in P5: `config.backend` now defaults to *unset*, and
   an unset backend resolves to `kitty` where the terminal answers the protocol
   query and `halfblock` everywhere else. Naming one in `setup` or with
   `:DistractBackend` still wins, and is remembered across a later `setup()`
   that names none.

2. **How large may a first-draw hitch be?** A GIF is decoded once, on the first
   frame that needs it, on the main loop: ~130ms for the 15-frame reference
   asset, ~375ms for the 32-frame one. If that is too much, the fix is a
   coroutine seam in `sprite_sources.load_sprite` that yields between frames —
   worth building only if someone actually notices it.

3. **Should the half-block quantiser run on procedural art too?** It is gated on
   imported art today, because the built-ins are drawn from a small palette by
   construction. Step 4's redo changes that arithmetic; if the quantised palette
   lands there, this gate can go and `max_sprite_colours` becomes the single
   answer for every asset.
