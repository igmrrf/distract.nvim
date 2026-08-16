# Handoff — fidelity, transparency and kinematics work

Working notes for whoever picks this up next. Rewritten 2026-08-16 against
`main` at `83988a5`, with the uncommitted files listed in the last section.

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
| P3 | Position, anchors, floor, `z`, backend capability table | 3 | **not started** |
| P4 | Kitty backend, procedural sprites | 5 | **not started** |
| P5 | GIF decoder, `terminal_sprites` wiring, halfblock quantiser | 5 | **not started** |
| — | Silhouette-first art redo, quantised palette | 4 | **not started** |

Step 4 (art) is independent of P3–P5 and can be done in either order, but see
the warning under it below.

---

## Verify the current state

All four gates pass on `83988a5`. Run them before and after any change.

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

Expected: **185 Lua tests**, **125 Rust tests** (116 lib + 6 headless GPU + 2
parity + 1 screenshot; `parity_dump` is `#[ignore]`).

Note the Lua invocation: `-l` with `--noplugin -u tests/minimal_init.lua`. The
older `-c "luafile ..."` form in previous notes fails to resolve
`distract.engine` because the runtimepath is set inside `minimal_init.lua`.

`luacheck` is listed in the README as a gate but **is broken on this machine** —
it fails to load under the installed Lua 5.5. Environment problem, not a code
problem. CI may still run it; a green local run does not mean luacheck passed.

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

---

## What P2 built (the shape P3 plugs into)

- `engine.step(dt, bounds)` — the pure simulation, with `dt` and screen size
  injected. `engine.tick()` measures `dt` and calls it. This is what makes the
  goldens possible.
- `engine.is_quiescent()` mirrors `World::is_quiescent`. It gates the **redraw
  only** — never the step. `ecs.rs` runs `World::update` unconditionally,
  because an entity can need a boundary wrap while not moving under its own
  power. Gating the step breaks vertical wrap; there is a test for it.
- Path primitives `linear` / `sine` / `orbital` / `lissajous` / `bezier` with
  `physics.path_params`. Phase advances at a base rate and per-axis frequency
  multiplies *inside* the trig term.
- `physics.locomotion` (`grounded` / `ballistic` / `omnidirectional`), derived
  from gravity when omitted, defaultable at manifest level.
- `transitions.on_land`, firing when a ballistic entity crosses its floor from
  above.
- `manifest.capabilities.locomotion`, validated once at load by
  `AssetManifest::validate_capabilities` and `lua/distract/locomotion.lua`.
- `:DistractSpawn cat x=10 y=5 flip_x=true`.

`z=` and `anchor=` are **deliberately rejected** by `:DistractSpawn` today.
Wiring them would have reached `engine.lua` alone — `external.lua` and
`IpcCommand::Spawn` have no such field — shipping a flag that worked in the
terminal and did nothing on the overlay. They arrive with P3, on both backends
together. `tests/plugin_commands_spec.lua` asserts `z=42` warns; that test
should be updated, not deleted, when P3 lands.

---

## P3 — position, anchors, floor, `z` (do this next)

Spec § 5. Preconditions are met.

1. `setup({ position = { anchor, ground, parallax } })` with per-spawn override.
2. Both floors computed **in Lua, for both backends**, because `external.lua`
   already owns the IPC unit conversion and the overlay should never need a
   buffer concept:
   - `"screen"` — `lines - cmdheight - laststatus_rows - sprite_h`, recomputed
     on `VimResized` and `OptionSet` for `cmdheight`/`laststatus`.
   - `"text"` — screen row of the last buffer line, via the screen map step 2
     built, gated on the `getwininfo()` fingerprint the overlay path already
     uses.
3. `UpdateGrid` gains `ground_y: Option<f32>` with `#[serde(default)]` for wire
   compatibility. Pushed on change, never per frame.
4. `z` = draw order (overrides `z_index`; the sorts already exist at
   `compositor.rs:138`, `gpu.rs:61`, `renderer.lua:322`) **and** parallax
   (`scale = clamp(1 + z * per_unit, min, max)`, damping both `vx` and `vy`).
   `per_unit` defaults to `0.0` — **parallax stays off unless asked for**.
5. Backend capability table replacing the `SUBSTITUTED_ALIASES` warning in
   `init.lua`. `halfblock` with `per_unit ≠ 0` warns **once** and honours order
   only — a declared degradation, not a silent divergence. Table-driven so the
   P4 kitty backend registers rather than special-cases.
6. `:DistractSpawn` gains `z=` and `anchor=`, on both backends together.

Add parity fixtures for the floor and for parallax damping. Note the § 11.4
risk: the step-2 screen map only maps the row a line *starts* on, so
`ground = "text"` must fall back to the screen floor where a row is unmappable.

---

## P4 — kitty graphics backend

Spec § 7. The P0 spike already settled the two things that could have sunk it,
and the answers are **not** the obvious ones:

- **Write mechanism.** `nvim_list_uis()[1].chan` is an RPC channel on nvim 0.12
  and rejects raw bytes. Use `vim.v.stderr` as the primary, `io.stdout` as
  fallback. § 7.3 has the verified table.
- **Detection.** ghostty answers the `a=q` graphics query but exposes no
  `$KITTY_WINDOW_ID`, so env detection is a fast path only — `a=q` is the
  authority. § 7.4.

Placement via unicode placeholders (`U+10EEEE`, `U=1`), `f=32` raw RGBA,
base64 chunked at 4096 with `m=1`/`m=0`, `c`/`r` for scaled placement, `z` for
order, `a=d,d=i` to delete.

**Unrun:** nobody has yet confirmed a kitty placement actually renders in a real
ghostty window. The user has ghostty installed. Get a human to look at the
screen once P4 draws anything.

---

## P5 — GIF support

Spec § 8. Pure-Lua GIF decoder, `terminal_sprites` wiring, halfblock palette
quantiser.

**Check this first:** the overlay backend already decodes GIFs
(`engine/src/asset.rs`, `load_gif`). Pointing a manifest's `spritesheet.path`
at `assets/cat_walking_1.gif` should give reference fidelity on the overlay
*today*, with no new code. If that covers the goal for the overlay, P5's scope
is the in-terminal backends only.

---

## Step 4 — art

Do **not** start by editing sprites. The same art exists twice —
`lua/distract/sprites/*.lua` and `engine/src/sprites/*.rs` — with **no
automated parity test** between them. `engine/tests/parity_dump.rs` is
`#[ignore]` and dumps *geometry*, not physics; it is a dev aid needing
`DUMP_TO`, and it is **not** covered by the physics parity harness above.
Build an art parity harness first or the two will drift the moment either is
touched. `future.md` § 5.8 names the tool: `validate_sprite_parity`.

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
   There is no way to *remove* a field via `setup` — which is also why
   `capabilities` (a list) merges rather than replaces, per spec § 11.5.

3. **Wall-clock `dt` in `engine.tick()`.** A tight loop of 20 ticks advances
   almost no simulated time. Use `engine.step(dt, bounds)` for anything that
   asserts on distance; `tick` is only for testing the timer path.

4. **Test probes inherit the cat's manifest.** Both parity runners and several
   spec helpers start from `AssetManifest::default_cat()`. Since P2e the cat
   declares `capabilities` and a manifest-level `locomotion = "grounded"`, so a
   probe that orbits is *correctly* refused. Clear both fields on the probe —
   `manifest.locomotion = None; manifest.capabilities = Default::default();`.
   This caught three of P2c's own tests when the gate landed.

5. **`vim.json.encode` writes an empty Lua table as `{}`, not `[]`.**
   `path_params.points` is the first array-valued manifest field, so the Rust
   deserialiser explicitly accepts both. Any future array-valued field needs
   the same treatment or it will parse in the terminal and fail on the overlay.

6. **1,909 global highlight groups** exist for the three built-in assets alone,
   created by `nvim_set_hl` and never released. Unbounded with community asset
   packs. Step 4's quantised palette should cut this by roughly 40×.

---

## Uncommitted files

These were staged before this run of work started and are **not** in any of its
commits, which touched only their own files:

- `CLAUDE.md`, `GEMINI.md`, `engine/CLAUDE.md`, `engine/GEMINI.md` — coding
  standards.
- `engine/rustfmt.toml` — active on disk; it is what reformatted
  `engine/src/ipc.rs` (whitespace only, uncommitted).
- `engine/clippy.toml` — **was invalid** and made `cargo clippy` error out
  rather than lint, so earlier clean runs were cache artifacts. Fixed here:
  dropped `cyclomatic-complexity-threshold` (a deprecated duplicate of
  `cognitive-complexity-threshold`), corrected `enum-size-threshold` to
  `enum-variant-size-threshold`, and removed `too-many-arguments-threshold`.
  That last one was set to 3 per CODING.md § 4 and flagged 19 pre-existing
  functions across atlas, compositor, gpu and ecs — `Entity::new` takes seven.
  Tightening those signatures is a repo-wide refactor and belongs in its own
  change.
- `engine/.github/workflows/rust-ci.yml`
- `REVIEW.md` (modified)

Decide whether to commit these before starting P3; `clippy.toml` in particular
is a working gate now and should not be left dangling.

---

## Open questions for the owner

1. Does GIF-on-overlay cover the fidelity goal, or is P4's in-terminal
   graphics-protocol backend required regardless?
2. Should the cat's `jump` return through `on_land` instead of its 1200 ms
   timeout? It declares `ballistic` and the transition exists; it was left
   alone because it changes how the jump feels.
3. Should step 4 redo the crab and sun to match, or is the cat the priority?
