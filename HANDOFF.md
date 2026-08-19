# Handoff — what is still open

Working notes for whoever picks this up next. Rewritten 2026-08-19, after the
full-feature pass.

This file holds **only** open work and the traps that cost time. It is
deliberately not a record of what shipped:

- **What was built and why** — [`CHANGELOG.md`](CHANGELOG.md).
- **What is not built yet** — [`future.md`](future.md), which is now entirely
  separate repositories: every core surface they need exists.
- **The decisions this pass took** —
  [`docs/superpowers/plans/2026-08-19-full-feature-completion.md`](docs/superpowers/plans/2026-08-19-full-feature-completion.md).
- **What the design says** —
  [`docs/superpowers/specs/2026-08-16-locomotion-position-kitty-design.md`](docs/superpowers/specs/2026-08-16-locomotion-position-kitty-design.md),
  including the unit contract and the backend/renderer split. Where this file and
  the spec disagree, the spec wins.
- **How to use the import pipeline** — [`docs/importing-assets.md`](docs/importing-assets.md),
  configuration in [`docs/configuration.md`](docs/configuration.md).

---

## Pending

| Item | State |
|---|---|
| Nothing is pushed | `fix/assets` is ahead of `origin/fix/assets`, no PR; integration is the owner's call |
| Four Lua modules and `ecs.rs` are over the 400-line cap | partly closed; see below for what is left and why |
| Three gallery pets ship as built-ins; nothing else from either gallery may | the rule is the licence, not the source — see below |
| The first draw of a GIF asset hitches | 130–375 ms, once per asset; not fixed, see below |
| Nothing else | every roadmap section is now out-of-repo work |

Everything in [`future.md`](future.md) is unbuilt by definition, and all of it is
now out-of-repo work.

---

## Verify the current state

All four gates pass. Run them before and after any change.

```bash
nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua
cargo test --manifest-path engine/Cargo.toml
stylua --check lua plugin tests
cargo clippy --manifest-path engine/Cargo.toml --all-targets -- -D warnings
```

Expected: **463 Lua tests**, **232 Rust tests** (179 lib + 31 import_sprite + 6
headless GPU + 6 IPC contract + 2 physics parity + 2 sprite parity + 2 tick
budget + 1 argv exit + 1 screenshot). The Rust count does not move with a new
physics or sprite fixture — one test function iterates the whole directory.

`cargo fmt --manifest-path engine/Cargo.toml -- --check` is a fifth gate worth
running; CI enforces it.

The Lua suite needs `-u tests/minimal_init.lua`; that is where the runtimepath is
set. Either `-l` or CI's `-c "luafile ..."` form works.

`luacheck` is listed in the README as a gate but **is broken on this machine** —
luacheck 1.2.0 under Lua 5.5 dies with `attempt to assign to const variable
'field_name'` before it reads any project file, and fails identically on files
nobody touched. Run it against an unmodified file to confirm before chasing it.
`stylua --check` is the real local Lua gate. CI may still run luacheck; a green
local run does not mean it passed.

**The Lua suite starts the overlay engine.** Some specs drive a real
`distract-engine` process, and `engine_binary.find()` prefers
`engine/bin` → `target/release` → `target/debug`. A stale release binary makes
those specs exercise an old IPC contract and log `unknown variant` parse errors
while still passing. Rebuild it (`cargo build --release --manifest-path
engine/Cargo.toml`) after changing `ipc.rs`.

---

## Looking at the art

**Look at the rendered pixels, not only at a text grid.** The screenshot suite is
the fastest way, and it writes 17 real PNGs through the actual wgpu pipeline:

```bash
cargo test --manifest-path engine/Cargo.toml --test render_flow_screenshots
# -> tests/screenshots/*.png, including a composite of all three assets
```

Judging the silhouette-first redo from `preview_sprite.lua` alone passed art that
was plainly wrong when rendered: the cat read as an orange sausage with a dark
head. Two defects were invisible in a character grid and obvious in a picture, and
both are worth knowing about before drawing anything else:

1. **A contour drawn as "a filled disc, then a smaller filled disc inset" is not a
   one-pixel outline.** The radii quantise to whole pixels, so at a head-sized
   `rx = 2.4` the inner disc collapses to a single plus and the shape renders as
   solid outline with one pixel of fill. `blob` now stamps the rim proper — a pixel
   inside the ellipse whose four-neighbourhood leaves it.
2. **A near-black rim disappears into a dark editor background** and takes the
   silhouette's edge with it. Each asset's rim is now a darker tone *of its own
   fill* (`FUR_DARK`, `SHELL_DARK`, the sun's `LIMB`), and the near-black `CONTOUR`
   is kept only for accents that must read as holes: eyes, an open mouth.

Legs also have to be drawn *below* the barrel, not inside it. They were inside it,
which is why the first rendered cat had none.

`tools/preview_sprite.lua` dumps an asset's frames as text: `#` for opaque, and a
letter per distinct colour so the tone bands can be counted. It is how the
silhouette-first redo was judged, and it is the only way to see this art from a
headless run.

```bash
nvim --headless --noplugin -u tests/minimal_init.lua -l tools/preview_sprite.lua cat
nvim --headless --noplugin -u tests/minimal_init.lua -l tools/preview_sprite.lua crab 0 4
```

**The canvas is 1-based on both engines.** `Canvas.set` drops anything at `x < 1`
or `y < 1`, so a layout laid out from row 0 sits one row high and the bottom row
of the sprite is empty — which floats the pet above the floor it is anchored to,
because an asset's cell footprint is its whole canvas. Cost an hour; the giveaway
is a sprite whose last row is blank in every frame.

---

## The art-parity harness — read before touching sprites

`engine/tests/sprite_parity.rs` dumps every frame of `cat`, `crab` and `sun` to
`tests/fixtures/sprites/<name>.golden.json` and asserts Rust still reproduces
them exactly. `tests/sprite_parity_spec.lua` asserts the Lua generators reproduce
the same pixels within the drift f32 against f64 makes unavoidable. Neither suite
runs the other's toolchain; they meet at the JSON.

```bash
UPDATE_GOLDEN=1 cargo test --manifest-path engine/Cargo.toml --test sprite_parity
```

The golden also pins each asset's canvas size, frame count and state-to-frame
`layout`. A state pointing at the wrong frames is the same defect class as a
frame drawn wrongly and far cheaper to catch here than on a screen.

**Why the tolerance has two rules.** `Canvas.set` floors its coordinates on both
sides, so a coordinate landing either side of an integer boundary throws a whole
drawing step into the adjacent pixel. A differing pixel is accepted when the
other engine's value appears anywhere in its 3×3 neighbourhood, or when both are
opaque and no channel differs by more than 24.

**The budgets are measurements, not allowances.** Re-measured after the redo:

| Asset | Pixels | Drifted | Budget | Unexplained | Budget |
|---|---|---|---|---|---|
| cat | 11,136 | 14 (0.13%) | 22 | 0 | 0 |
| crab | 9,600 | 4 (0.04%) | 12 | 0 | 0 |
| sun | 6,400 | 79 (1.23%) | 87 | 0 | 0 |

Flat fills are why `unexplained` is now zero everywhere: a gradient put a
different colour on every radius, so a boundary difference changed the colour as
well as the position and no neighbourhood rule could explain it. With one fill
and one band per part, a boundary difference moves a pixel between two colours
that are already neighbours. **Any unexplained pixel is now a real divergence.**

All 79 built-in frames create **118** live highlight groups against the 4,096
cap — 3%, where the shaded art used 46%.

---

## The physics-parity harness — read before touching physics

`engine/tests/physics_parity.rs` generates the goldens;
`tests/physics_parity_spec.lua` asserts the Lua engine reproduces them. They meet
at the JSON in `tests/fixtures/physics/`, in **terminal cells**.

**Any change to physics on either side means adding a fixture.**

```bash
UPDATE_GOLDEN=1 cargo test --manifest-path engine/Cargo.toml --test physics_parity
```

Then run the Lua suite. If it disagrees, that is the point — read the reported
step index before assuming the fixture is wrong.

A fixture may declare `bounds.col` / `bounds.row` for a **scoped viewport**, and
an `obstacles` list of `solid_platform` / `hazard` rectangles in cells. Both are
absent on the older fixtures, which means "the whole editor grid, no obstacles"
and keeps every existing trajectory unchanged.

**Avoid knife edges, and say so in the fixture's own `description`** so nobody
"fixes" them back. `constant_velocity_wrap` uses `target_vx = 1.3` and
`path_bezier` uses `freq = 0.47` for this reason; the frame-timing fixtures use
`dt = 0.013`. Two obstacle fixtures put a platform edge at `45.3` and a hazard at
`61.3` rather than on even cells: an entity advancing exactly 2 cells a step and
24 wide would otherwise land with its right edge exactly on the obstacle's left
edge, where f32 and f64 fall either side of the span test. That cost a debugging
round; the symptom was one engine picking the higher platform a step earlier.

**The two engines index `animation.frames` differently, on purpose.** Lua's
`frame_idx` is 1-based; Rust's is 0-based. Each indexes its own convention
correctly and both cycle the same number of frames, which is why a fixture
records the *resolved sheet index* rather than `frame_idx`.

`tests/fixtures/physics/frame_delays.gif` is the art the frame-timing fixtures
bind: 209 bytes, four solid 24×16 frames, delays 40/120/80/200 ms. No two delays
are equal, none is 100 ms (the fallback), and 24×16 matches the size an unbound
probe already reports. The regeneration command is in the harness header.

---

## Traps that cost time — read before debugging

1. **A `local function` used by an earlier closure is a nil global.** Lua
   resolves a local by lexical position, so a helper declared below the function
   that calls it is looked up as a global and is nil at call time. In
   `renderer.lua` that error was swallowed by `tick`'s `pcall`, so the symptom
   was a sprite that silently stopped drawing rather than an error. Declare
   shared helpers above every consumer.

2. **One asset has one cell footprint, and fidelity is independent of it.**
   `get_dimensions` takes no backend argument by design: sprite size feeds
   physics through `sprite_cell_size`, which is what wrapping and floor-anchoring
   measure against, so a per-backend answer makes one manifest describe two
   behaviours. Kitty's `c`/`r` fields resample a transmitted image into a given
   cell box, so kitty loses nothing by honouring the fitted footprint.

3. **A kitty opacity mask must be built on the footprint grid, not the image
   grid.** `frames.spans` resamples the mask *from* `frame.cols` × `frame.rows`.
   Building it on the image's grid still produces the right *number* of rows
   while reading the wrong region — the top 17 pixel rows of 72 — and the sprite
   silently vanishes. `describe` therefore takes `rgba` from the native matrix
   and `mask` from the fitted one.

4. **Any test of a spatial mask needs art that varies in space.** The mutation in
   trap 3 initially slipped past its own test because the fixture was fully
   opaque and every candidate mask was identical.

5. **`terminal_sprites.lua` is the quantiser's only gate.** 32 unquantised
   imported frames go straight through the 4,096 highlight-group cap, so
   `needs_quantising` must stay true for sidecar-backed assets. It stays false
   for procedural art: the silhouette-first redo took the built-ins to 123 groups
   on its own, so quantising them again would only spend CPU.

6. **macOS display detection matches by ID, never by coordinates or size.**
   `NSScreen.mainScreen`'s `NSScreenNumber` is a `CGDirectDisplayID` and so is
   winit's `native_id()`, so **no Cocoa-to-winit conversion is involved**. Do not
   reintroduce one: Cocoa's origin is the primary screen's bottom-left and
   winit's is its top-left. Matching by size breaks on two identical monitors.

7. **Neither engine measures its own floor, viewport scope or obstacles.**
   `events.sync_floor`, `external.sync_viewport_scope` and
   `events.sync_obstacles` each measure once in Neovim and push the same answer
   to both engines. A change that has an engine look for itself reintroduces the
   divergence class the harness exists to catch. The one exception is a spawn
   naming its own `ground`.

8. **`engine.lua` holds `floor_row` and the obstacle list as module state**, so a
   spec that spawns after another spec's push inherits it.
   `tests/physics_parity_spec.lua` calls `set_ground_row(nil)` and
   `set_obstacles({})` before every fixture; any new spec asserting on `ground_y`
   or on platform physics must do the same.

9. **`engine.setup` merges with `vim.tbl_deep_extend("force", ...)`.**
   Registering two test manifests under the same asset name lets the first one's
   `physics` fields survive into the second. Every spec that builds probe
   manifests gives each test its own `probe_N` name for this reason.

10. **Test probes inherit the cat's manifest.** Both parity runners and several
    spec helpers start from `AssetManifest::default_cat()`, which declares
    `capabilities` and `locomotion = "grounded"` — so a probe that orbits is
    *correctly* refused. Clear both:
    `manifest.locomotion = None; manifest.capabilities = Default::default();`.

11. **The cat's states do not share a `wrap_mode`.** `idle` clamps, `walk` wraps,
    `pounce` bounces. A test that spawns a cat and expects wrapping gets a
    clamped one, because the initial state is `idle`; put the entity in `walk`
    first. This is also the answer to "should a pet only walk one way" — the
    manifest decides, per state.

12. **Wall-clock `dt` in `engine.tick()`.** A tight loop of 20 ticks advances
    almost no simulated time. Use `engine.step(dt, bounds)` for anything that
    asserts on distance; `tick` is only for testing the timer path.

13. **A spawn randomises `frame_idx`, `frame_timer` and `path_phase`** so two
    entities spawned together do not move in lockstep. Any test asserting on
    elapsed animation time has to zero the first two afterwards;
    `tests/gif_assets_spec.lua` has `spawn_at_first_frame()` for it.

14. **`vim.fn.screenstring` lies inside `nvim -l` scripts.** It reads the current
    window's grid, not the composited screen, so floating windows appear at the
    wrong place or not at all. Assert on `nvim_win_get_position` /
    `nvim_win_get_config` for float rows. `screenstring` **is** trustworthy for
    the extmark overlay path, because those are written into the current window's
    own buffer.

15. **`backends`, `position`, `distract.kitty`, `viewport`, `visibility`,
    `plugins` and `obstacles` all hold process-wide state.** Each has a `reset()`
    for tests. A spec that registers kitty and does not put it back breaks
    `backends_spec`; a spec that leaves a plugin registered changes what the
    overlay subscribes to for every later spec; a spec that leaves an obstacle
    provider registered changes physics for every later spec.

16. **The kitty test seam is `writer.set_writer`, and every spec that uses it
    must put it back.** A leaked capture silently swallows every subsequent
    escape and nothing fails — the assertions are all on what was captured. Use
    `captured()` / `with_kitty()` in `tests/kitty_spec.lua`.

17. **`detect.is_available()` answers once and caches.** Headless it is always
    false, because there is no UI to answer the query. `detect.override(true)`
    gets a test past that; `detect.reset()` puts it back.

18. **`config.backend` is `nil` by default**, and nil means "pick the best this
    terminal can draw". A spec that wants the default path back has to assign
    `distract.config.backend = nil` before calling `setup()`.

19. **`tests/run_tests.lua` has an explicit `SPECS` list.** A new spec file that
    is not added to it silently never runs, and the suite still reports green.

20. **The harness is not Plenary.** `tests/test_harness.lua` provides
    `assert.are.same`, `assert.are_equal`, `assert.are_not_equal`,
    `assert.is_true/is_false/is_nil/is_not_nil/is_function`. There is no
    `assert.are.equal`, no `assert.is_not.same`.

21. **The harness compares with `vim.inspect`, which prints table identity.**
    `assert.are.same({ RED, RED }, row)` fails against a row of two equal but
    distinct tables. Assert per pixel rather than per row.

22. **An imported asset's art binds on spawn, not on `setup`.**
    `sprite_sources` resolves a manifest's spritesheet when
    `bind_manifest` is called, which `engine.spawn` does. Asking for such an
    asset's frames before anything has spawned it reports the *procedural
    fallback* — 29 cat frames at 24×16 — with a notification saying so, which
    reads exactly like a manifest whose frame indices are out of range. It fooled
    the first draft of `tests/builtin_assets_spec.lua` into "finding" a bug in
    `cat_walking`, whose 32 frames are correct. `bind_manifest(name, manifest)`
    first, as that spec now does.

23. **`native_sprite.load` caches by path**, so a spec reusing one fixture path
    across tests reads the first test's frames. Call `native_sprite.reset()` in
    `after_each`.

24. **`vim.json.encode` writes an empty Lua table as `{}`, not `[]`.** Both
    `path_params.points` and `UpdateObstacles.obstacles` accept either encoding
    for that reason. Any future array-valued field needs the same treatment or it
    parses in the terminal and fails on the overlay.

25. **`vim.tbl_deep_extend` cannot set a field to nil**, so a placement-request
    helper built with it cannot express "no floor measured". `position_spec`
    assigns `request.floor_row = nil` after building instead. It also merges
    lists by index, which is why `viewport.configure` replaces
    `exclude_filetypes` outright rather than extending it.

26. **`x and false or y` never yields `false` in Lua.** `false` is falsy, so the
    `or` branch always wins. Use an explicit `if`.

27. **A hyphen in a table key is a Lua syntax error.** Generated manifests need
    `["running-right"] = { … }`. Real action names have hyphens; `walk`/`idle`
    test fixtures never caught it.

28. **The importer never resamples.** Feed it 1920×1080 stills and you get
    1920×1080 frames, a 15360×4320 sheet and a 265 MB sidecar. Downscale first.

29. **Don't point `--manifest-out` at a hand-tuned manifest.** The scaffold
    overwrites, and its `physics`/`transitions` are placeholders. Write it
    elsewhere and diff.

---

## Accepted debt

**The size-cap debt is partly closed.** Everything added during the feature pass
went into new modules rather than into the three files that were already over the
cap — `kinematics.lua`, `placement.lua`, `viewport.lua`, `visibility.lua`,
`plugins.lua`, `obstacles.lua`, `entity_step.lua`, `engine_binary.lua`,
`overlay_grid.lua`, `overlay_report.lua`, `overlay_plugins.lua`, and `commands.rs`,
`response.rs`, `subscription.rs`, `bounds.rs`, `journal.rs`, `obstacles.rs`,
`wrap.rs`. `main.rs` stayed under the cap that way, and `ipc.rs` came down to 186
by moving its wire-format tests to `engine/tests/ipc_contract.rs`.

`engine.lua`'s per-entity frame then moved out to `entity_step.lua`, which took the
module from 1,012 lines to 780 and turned a 200-line `M.step` into a 64-line one
that coordinates. The physics-parity fixtures are what made that safe: they are the
characterization tests a parity-first refactor requires, and none of the goldens
moved.

| File | Lines | Cap |
|---|---|---|
| `engine.lua` | 780 | 400 |
| `renderer.lua` | 635 | 400 |
| `external.lua` | 448 | 400 |
| `sprite_gen.lua` | 445 | 400 |
| `engine/src/ecs.rs` | 2,168 | 400 |

What is left in each is one function whose locals every branch shares:
`M.spawn` (135 lines), `M.place_surface`, `World::update`, and
`entity_step.advance` — which is over the *function* cap by design and says so in
its own header, because its five numbered steps each read what the previous one
wrote. §5 of the standards covers that case: a cap is a signal to decompose, not a
reason to fragment a unit that has to be read as one. `M.spawn` is the next
worthwhile extraction and the one with the clearest seam (build the entity, then
insert and report it); the parity harnesses do **not** cover spawn placement as
tightly as they cover the step, so write the characterization test first.

**One trap, learned the expensive way.** `git checkout <file>` during a session with
uncommitted work throws that work away with no warning and no recovery — it cost a
full reconstruction of `engine.lua` from the session record. Use `git stash` or a
copy before reverting anything, and prefer reverting the specific edit.

`assets/codex_pets/` is gitignored on purpose — 236 MB of third-party artwork
with no stated licence, kept on disk as local test material only. `imported/`
regenerates from `sheets/` via `tools/codex_pets/`.

**Three pets ship as built-ins, and what let them is the licence rather than the
source.** `gudong` (CC BY 4.0), `iris` (MIT, with the artist's explicit
redistribution permission) and `minty` (MIT) are original characters from
[`legeling/awesome-codex-pet`](https://github.com/legeling/awesome-codex-pet),
credited per artist in [`ATTRIBUTION.md`](ATTRIBUTION.md), which is what CC BY's
attribution term requires. Everything else in that gallery — and everything under
`assets/codex_pets/` — is franchise fan art or carries no stated licence: **none
of it may be bundled.** `scrape_pets.py --source awesome` copies each pet's
declared licence into the catalogue so that question can be answered before
publishing rather than after.

Their cells are 192×208. `sprite_sources.TERMINAL_SPRITE_MAX_WIDTH` fits that to
32 sprite pixels wide — 32 columns by 18 terminal rows — for the half-block
renderer, while `kitty` and `overlay` use the full-resolution sidecar and packed
sheet. **`frame_width`/`frame_height` must keep describing the source cell**: the
asset loader slices the packed sheet by them, so shrinking them to shrink the
drawn sprite silently slices the wrong pixels. The drawn footprint is
`TERMINAL_SPRITE_MAX_WIDTH`'s job, not the manifest's. Verified: all three spawn,
animate across nine states and 74 frames, and draw.

**The first draw of a GIF asset hitches**: it is decoded once, on the first frame
that needs it, on the main loop — ~130 ms for the 15-frame reference asset,
~375 ms for the 32-frame one. The fix is a coroutine seam in
`sprite_sources.load_sprite` that yields between frames. Not built: it is real
work for a hitch nobody has reported.
