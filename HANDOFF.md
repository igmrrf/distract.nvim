# Handoff — fidelity, transparency and kinematics work

Working notes for whoever picks this up next. Rewritten 2026-08-16 against
`main` at `3739917`. The working tree is clean.

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
| P5 | GIF decoder, `terminal_sprites` wiring, halfblock quantiser | 5 | **not started** |
| — | Silhouette-first art redo, quantised palette | 4 | **not started** |

---

## Verify the current state

All four gates pass on `3739917`. Run them before and after any change.

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

Expected: **269 Lua tests**, **137 Rust tests** (128 lib + 6 headless GPU + 2
parity + 1 screenshot; `parity_dump` is `#[ignore]`).

Note the Lua invocation: `-l` with `--noplugin -u tests/minimal_init.lua`. The
older `-c "luafile ..."` form in previous notes fails to resolve
`distract.engine` because the runtimepath is set inside `minimal_init.lua`.

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

The user has Ghostty. Get a human to look at the screen: `:DistractBackend
kitty`, then `:DistractSpawn cat`. If it draws nothing at all, the first check
is `:set termguicolors?` — the backend declines without it and says so.

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
- **`engine.lua` is still 917 lines** against a 400-line cap, with `M.spawn` and
  `M.step` well over the 60-line function cap. Owner's call, 2026-08-16: leave
  it until the features are in, but no *new* file may break the standards.
  `renderer.lua` is 488, down from the 496 it started P4 at.

---

## P5 — GIF support (do this next)

Spec § 8. Pure-Lua GIF decoder, `terminal_sprites` wiring, halfblock palette
quantiser.

Owner's answer to the earlier question: **both**. GIF-on-overlay does not remove
the need for the in-terminal graphics-protocol backend, so P4 and P5 were both
in scope rather than alternatives.

**Check this first, before writing a decoder:** the overlay backend already
decodes GIFs (`engine/src/asset.rs`, `load_gif`). Pointing a manifest's
`spritesheet.path` at `assets/cat_walking_1.gif` should give reference fidelity
on the overlay *today*, with no new code. If that covers the goal for the
overlay, P5's scope is the in-terminal backends only.

P4 changes what P5 is worth on each backend. Kitty takes RGBA per pixel, so a
decoded GIF frame goes to it essentially unaltered — `frames.describe` is the
only place that would need to accept a decoded frame instead of a procedural
matrix. Half-blocks still need § 8.3's quantiser, and still land two pixel rows
per cell.

---

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

---

## Open questions for the owner

1. **Should Ghostty and kitty users get the kitty backend by default?** It
   registers itself when the environment names a confirmed terminal, but the
   default backend is still `halfblock` — the user has to select it. Making it
   the default would change what existing users see on upgrade without them
   asking. Waiting for the on-screen confirmation above before deciding.
2. **`cargo audit` in the root CI.** `engine/.github/workflows/rust-ci.yml` is
   committed (`460292b`) and inert where it sits: GitHub only reads
   `.github/workflows` at the repository root. Its one step the root workflow
   lacks is `cargo audit`. Folding it in wants a non-blocking job — an advisory
   anywhere in the wgpu tree would otherwise redden unrelated changes.
