# Handoff — fidelity, transparency and kinematics work

Working notes for whoever picks this up next. Rewritten 2026-08-16 against
`main` at `fdcfc32`. The working tree is clean apart from one file, named in
the last section.

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

Expected: **228 Lua tests**, **137 Rust tests** (128 lib + 6 headless GPU + 2
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

Fixtures gained two optional fields in P3. `ground_row` (cells) is pushed into
the world *before* the spawn, so each engine derives the entity's own floor by
subtracting the sprite height in its own units — arithmetic written twice and
therefore worth pinning. `spawn.parallax` is applied to the entity *after* the
spawn, alongside the `path_phase` zeroing and for the same reason: a fixture
describes what the engine is given, not the `position` config and backend
capabilities that would have produced it. The half-block backend flattens every
parallax to 1, so going through the config would test nothing.

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

## What P2 and P3 built (the shape P4 plugs into)

From P2:

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

From P3:

- `lua/distract/position.lua` — anchors, both floors, and the parallax factor.
  The pure arithmetic (`placement`, `parallax_factor`) is separate from the
  parts that read `vim.o` and the backend registry, so most of it is testable
  with no editor state at all.
- `lua/distract/backends.lua` — the capability table. `register(name, caps,
  aliases)` is how the P4 kitty renderer joins: it registers, stops being a
  substitution, and `supports_parallax` starts returning true for it. Nothing
  else needs editing. `reset()` exists because the registry is process-wide and
  a spec that registers has to put it back.
- **Neither engine measures its own floor.** `distract.spawn` and the
  `VimResized` / `OptionSet` / `WinScrolled` / `TextChanged` autocommands call
  `events.sync_floor`, which measures once and pushes the same number to
  `engine.set_ground_row` and `external.set_ground_row`. The overlay gets it as
  `UpdateGrid.ground_y`, in pixels, on change only. Read that direction before
  changing anything here: an engine that measures for itself is exactly how the
  two backends drift apart.
- With **no** floor pushed, an entity's floor is its spawn point — the
  behaviour from before P3, and what `World::spawn` does with `ground_y: None`.
  The parity runners rely on it: a fixture without `ground_row` must behave
  identically on both sides.
- Moving the floor re-seats only entities standing on the *previous* world
  floor. A manifest floor and the anchor a jump takes are their own.
- `z` folds into `z_index` at spawn, so the three existing sorts were left
  alone. `parallax` multiplies the displacement — never the stored velocity,
  which would decay to zero — and the drawn size, and therefore the footprint
  the boundary modes and the floor measure.
- `:DistractSpawn cat x=10 y=5 z=2 anchor=bottom flip_x=true`.
- The cat's jump now returns through `on_land`, and a landing cancels the
  action that launched it. Handoff question 2, answered: a timeout tuned
  against `gravity` and `jump_impulse_y` is a number that has to be re-tuned
  by hand whenever either moves.

### What P3 deliberately did not do

- **`ground = "text"` uses `screenpos` on the current window**, not the
  renderer's screen map. The map is keyed on rows a line *starts* on and is
  rebuilt inside the terminal draw path, which the overlay never enters; one
  `screenpos` call on the last visible line answers the same question for both
  backends. The § 11.4 fallback is honoured: an unmappable row falls back to
  the screen floor.
- **Parallax does not scale path amplitudes**, only velocity integration and
  the sprite. The spec says "damping both `vx` and `vy`"; a path is a
  positional override, not a velocity, so leaving it alone was the reading that
  changed the least. Revisit if a parallaxed sine looks wrong.
- **`engine.lua` is now 909 lines** against a 400-line cap, and `M.spawn` and
  `M.step` are both well over the 60-line function cap. Pre-existing and
  already logged in `REVIEW.md` § 8, but P3 made it worse rather than better.
  Decomposing it is its own change, with characterisation tests first.

---

## P4 — kitty graphics backend (do this next)

Spec § 7. Preconditions are met: P0 answered, P3 green.

Start with `backends.register("kitty", { scale = true, alpha = "pixel" },
{ "ghostty", "wezterm" })`. That one call takes `kitty` out of the substitution
table, makes `:DistractBackend kitty` resolve to itself, and turns parallax on
for it — the capability plumbing is already done and has a test.

The P0 spike already settled the two things that could have sunk it, and the
answers are **not** the obvious ones:

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

`renderer.lua` dispatches through `BACKEND_DRAW`, which currently has one entry.
A kitty draw function registers there the same way the capabilities do.

---

## P5 — GIF support

Spec § 8. Pure-Lua GIF decoder, `terminal_sprites` wiring, halfblock palette
quantiser.

Owner's answer to question 1: **both**. GIF-on-overlay does not remove the need
for the in-terminal graphics-protocol backend, so P4 and P5 are both in scope
rather than alternatives.

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

Owner's answer to question 3: the redo covers **every asset, existing and
future** — not the cat alone. That makes the art-parity harness
(`validate_sprite_parity`) a precondition rather than a nicety, since three
assets times two implementations is six files that can drift.

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

## Traps P3 added

7. **Neither engine measures its own floor.** `events.sync_floor` measures and
   pushes; `engine.set_ground_row` and `external.set_ground_row` receive. A
   change that has an engine call `position.floor_row` for itself reintroduces
   the divergence class this whole harness exists to catch. The one exception
   is a spawn naming its own `ground`, which is asking about a surface the
   pushed floor does not describe.

8. **`engine.lua` holds `floor_row` as module state**, so a spec that spawns
   after another spec's push inherits its floor. `tests/physics_parity_spec.lua`
   calls `set_ground_row(nil)` before every fixture for exactly this reason.
   Any new spec that asserts on `ground_y` must do the same.

9. **`backends` and `position` warn once, process-wide.** `reset_warnings()`
   and `reset()` exist for tests. A spec that counts warnings and does not
   reset first counts zero, passes, and proves nothing.

10. **`vim.tbl_deep_extend` cannot set a field to nil**, so a placement-request
    helper built with it cannot express "no floor measured". `position_spec`
    assigns `request.floor_row = nil` after building instead.

---

## Uncommitted files

One file is left untracked on purpose:

- `engine/.github/workflows/rust-ci.yml` — **inert where it sits.** GitHub only
  reads `.github/workflows` at the repository root, so this has never run. The
  root `ci.yml` already gates `cargo fmt`, `cargo clippy` and `cargo test`
  across three platforms. Its one unique step is `cargo audit` via
  `rustsec/audit-check`, which was not folded in blind: an advisory anywhere in
  the wgpu dependency tree would turn CI red on a change that has nothing to do
  with it. Adding it is a deliberate decision, either as a non-blocking job or
  after checking the tree is clean. Delete the file or fold the step in; do not
  commit it as-is.

Everything else that was dangling — the coding standards, `rustfmt.toml`, the
reformatted `ipc.rs`, `REVIEW.md` — landed in `6370c07`.

---

## Open questions for the owner

All three previous questions are answered and folded into the sections above:
P4 **and** P5 are both required, the cat's jump now returns through `on_land`,
and the art redo covers every asset rather than the cat alone.

New ones, from P3:

1. Should `position.anchor` default to `"auto"` (what it does today: bottom for
   anything gravity binds, centre for anything that drifts) or to a literal
   `"bottom"` for everything? `auto` is what "constrained by what the entity can
   physically do" reads as, and it keeps the sun in the sky, but it does mean
   the same config places two assets differently.
2. Should a manifest be able to declare its own preferred anchor, the way it
   declares `z_index` and `locomotion`? The sun wanting the top of the screen is
   a property of a sun, not of a user's configuration. Not specified, so not
   built.
3. `cargo audit` in CI — see the uncommitted file above.
