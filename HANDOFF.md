# Handoff — what is still open

Working notes for whoever picks this up next. Rewritten 2026-08-19, after the
full-feature pass, and again after the 3D render pass.

This file holds **only** open work and the traps that cost time. It is
deliberately not a record of what shipped:

- **What was built and why** — [`CHANGELOG.md`](CHANGELOG.md).
- **What downstream repositories could be built on this** —
  [`docs/ecosystem-roadmap.md`](docs/ecosystem-roadmap.md). None of it is a
  missing feature of this plugin; every core surface it needs exists.
- **The decisions the feature pass took** —
  [`docs/superpowers/plans/2026-08-19-full-feature-completion.md`](docs/superpowers/plans/2026-08-19-full-feature-completion.md).
  Its decision 1 ("2D is the contract, 3D is not built") is **superseded** by
  [`docs/superpowers/plans/2026-08-19-voxel-3d-rendering.md`](docs/superpowers/plans/2026-08-19-voxel-3d-rendering.md),
  which explains how 3D was built without doing the two things that decision was
  right to refuse: forking a backend off the manifest contract, and making anyone
  author an asset twice.
- **What the design says** —
  [`docs/superpowers/specs/2026-08-16-locomotion-position-kitty-design.md`](docs/superpowers/specs/2026-08-16-locomotion-position-kitty-design.md),
  including the unit contract and the backend/renderer split. Where this file and
  the spec disagree, the spec wins.
- **How to use the import pipeline** — [`docs/importing-assets.md`](docs/importing-assets.md),
  configuration in [`docs/configuration.md`](docs/configuration.md).

## Feature Lock Policy

**`distract.nvim` is officially FEATURE LOCKED.**

No new features outside the documented scope and specifications may be added. All future contributions and changes are strictly limited to **improvements**:
- Performance, frame budget, and memory leak optimizations.
- Reliability, resource cleanup, and lifecycle safety fixes.
- Cross-platform compatibility hardening and driver fallbacks.
- Bug fixes, parity regressions, and test suite additions.

Any speculative extensions, domain-specific companion behaviors, or game mechanics belong in **external downstream plugins** via the Plugin API (`require("distract").register_plugin`), not in core.

---

## Status

| Item | State | Notes |
|---|---|---|
| Core Engine & Plugin | **Feature Locked** | All documented capabilities implemented, verified, and locked. |
| Memory & Lifecycle | Clean | Kitty ID allocation, obstacle provider indexing, and backend teardown resolved. |
| Test Suite | 100% Green | 557 Lua tests, 282 Rust tests passing. |
| Downstream ecosystem plugins | External | External companion plugins and integrations described in [`docs/ecosystem-roadmap.md`](docs/ecosystem-roadmap.md). |

---

## Verify the current state

All four gates pass. Run them before and after any change.

```bash
nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua
cargo test --manifest-path engine/Cargo.toml
stylua --check lua plugin tests
cargo clippy --manifest-path engine/Cargo.toml --all-targets -- -D warnings
```

Expected: **557 Lua tests**, **298 Rust tests** (124 lib + 31 import_sprite +
17 asset_loading + 16 gpu_setup + 15 ecs_world + 14 manifest_schema + 12
ecs_motion + 12 ecs_placement + 11 voxel mesh + 10 IPC contract + 7 headless
voxel GPU + 7 sprite_canvas + 6 headless GPU + 5 ecs_locomotion + 3 voxel
parity + 2 physics parity + 2 sprite parity + 2 tick budget + 1 argv exit + 1
screenshot). The Rust count does not move with a new physics, sprite or voxel
fixture — one test function iterates the whole directory.

The previous note here said 282 and folded eight suites into its "lib" figure.
Count them rather than trusting a remembered total:

```bash
cargo test --manifest-path engine/Cargo.toml 2>&1 \
  | grep -oE "^test result: ok\. [0-9]+ passed" | grep -oE "[0-9]+" | paste -sd+ - | bc
```

`cargo fmt --manifest-path engine/Cargo.toml -- --check` is a fifth gate worth
running; CI enforces it.

The Lua suite needs `-u tests/minimal_init.lua`; that is where the runtimepath is
set. Either `-l` or CI's `-c "luafile ..."` form works.

**`luacheck` is now green, and it was not before.** It fails to *run* under Lua
5.5 — luacheck 1.2.0 dies inside its own `standards.lua` with `attempt to assign
to const variable 'field_name'` before it reads any project file — so the local
gate looked absent and nobody could see that CI's `luacheck lua plugin tests` step
was failing on 24 warnings. It is a plain invocation with no ratchet and no
`--no-warnings`, so any warning at all exits non-zero.

The breakage is the interpreter, not the project. Install it against 5.1 and run
it through luajit:

```bash
luarocks --lua-version=5.1 --tree=/tmp/lr install luacheck
sh /tmp/lr/bin/luacheck lua plugin tests   # 0 warnings / 0 errors in 91 files
```

The 24 warnings were all trivial and all are fixed: locals left dead by earlier
extractions, four aliases in `sprites/crab.lua` and one in `sprites/cat.lua` that
nothing read, a `position`/`highlights`/`spans` shadow apiece, one over-long line,
and five specs that re-`require`d a module their own file scope already held. None
of it changed behaviour; all 530 tests and every golden are unchanged.

**It went red again after that, and the same class of thing did it.** Four more
leftovers accumulated from later extractions — `FLOOR_MATCH_EPSILON_CELLS` in
`engine.lua` after the live copy moved to `engine_world.lua`, `wraps_at_the_edge`
in `renderer.lua` after the live copy moved to `renderer_surface.lua`, and
`overlay_spawn` required twice in `external.lua`. All four are deleted. Because
CI's step is a plain `luacheck lua plugin tests` with no ratchet, *one* unused
local is a red build, and the local gate is the only place anyone will see it
before CI does. Run it on every change:

```bash
sh /tmp/lr/bin/luacheck lua plugin tests   # 0 warnings / 0 errors in 106 files
```

`stylua --check` is still the formatting gate and is separate.

`cargo test --test gpu3d_headless` and `--test gpu_headless` **skip rather than
fail** when no GPU adapter is available, so a green run on a headless runner does
not prove the shaders compiled. Run them on a machine with a GPU before trusting
a renderer change.

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
nvim --headless --noplugin -u tests/minimal_init.lua -l tools/preview_sprite.lua cat --3d=70
```

**The same rule applies twice over to the 3D mode**, and it has its own screenshot
writer for exactly that reason:

```bash
cargo test --manifest-path engine/Cargo.toml --test gpu3d_headless
# -> tests/screenshots/18..21_voxel_*.png, through the real pipeline and shader
```

A model that is silently wrong still produces plenty of opaque pixels, so a pixel
count proves nothing about whether it reads as a pet. `--3d=0` is the useful
comparison: a model turned no degrees covers *exactly* the pixels its sprite does,
so any difference there is a bug in the projection or the canvas mapping rather
than a matter of taste.

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

## The voxel-parity harness — read before touching meshing or the 3D renderer

`engine/tests/voxel_parity.rs` writes `tests/fixtures/voxels/*.golden.json` and
asserts `engine/src/voxel.rs` still reproduces them;
`tests/voxel_parity_spec.lua` asserts `lua/distract/voxel.lua` does too. A pet
that meshes differently on the two backends is a pet that changes shape when the
overlay opens.

```bash
UPDATE_GOLDEN=1 cargo test --manifest-path engine/Cargo.toml --test voxel_parity
```

**This harness has no tolerance, unlike the other two, and that is deliberate.**
Nothing in a mesh goes through a float computation whose width matters: a voxel
coordinate is a whole number or an exact half, a normal is one unit on one axis,
and a colour is a source byte copied through. Any difference at all is a real
divergence, and a tolerance would only hide one. If you find yourself wanting to
add one, the mesher has changed in a way that needs explaining rather than
absorbing.

**Every fixture declares its own source grid rather than meshing an asset.**
Sprite art is only equal across the engines within a measured drift, so meshing
each engine's own cat would compare two things that were already allowed to
differ, and a one-pixel sprite drift would read as a meshing bug. With a declared
grid the meshing is the only variable.

**The golden is the mesh, not a picture.** The two engines rasterise deliberately
differently — the overlay on a GPU under a perspective camera, the terminal in Lua
under an orthographic one — so comparing pixels would fold two unrelated
divergences into one number. Nothing compares the two rasterisers to each other,
and nothing should.

**Emission order is part of the contract.** The golden records vertices in the
order they are emitted and the index list addresses that order, so the faces in
`exposed_faces` and the corners in `Face::corners` are pinned face for face and
corner for corner. Verified to bite: reversing the order two faces are emitted in
fails four fixtures rather than passing quietly. Both sides must change together.

**A change to the meshing means adding a fixture**, the same rule physics
follows. `wide_resampled` exists because the nearest-neighbour fit has its own
arithmetic: its stripes are 8 source pixels wide against a cap of 12, which is
deliberately not a whole ratio, so an off-by-one moves a stripe edge.

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

30. **At a yaw of 0 or 180 degrees, a voxel turn is indistinguishable from a
    mirror.** The side faces project to no width at all there, so a test claiming
    "a turned model is not a mirrored one" passes only at an intermediate angle.
    The first draft of that test asserted it at yaw 0 and failed for the right
    reason. `render_3d_spec` now pins both halves: the equivalence at 0, and the
    difference at 35.

31. **The 3D screenshot writer proves pixels exist, not that they read as a pet.**
    A model with its corner order scrambled still fills plenty of pixels. Look at
    `tests/screenshots/18..21_voxel_*.png`, and use `--3d=0` against the 2D
    silhouette as the exact check.

32. **`frame_source`, `raster3d` and the kitty describer all hold process-wide
    caches keyed by asset**, on top of the seven modules trap 15 lists.
    `frame_source.configure` drops the first two and announces to the third; a
    spec that changes the render mode and does not put it back leaves every later
    spec drawing models. `render_3d_spec` restores `render.DEFAULTS` in
    `after_each` for that reason, and re-binds the cat's manifest, because
    `bind_manifest` is what records an asset's pinned mode.

33. **A voxel model's fidelity is bounded by the voxel grid, not by the source
    image.** On kitty that means a 192-wide imported pet transmits a 32-wide model
    in 3D where 2D transmits the full sheet. That is the deliberate tradeoff and
    `render.voxel_max_width` is the lever; the *footprint* is unchanged in both
    modes, which is the part that matters, because it is what the engine wraps and
    anchors against.

34. **`parallax` must not scale a model.** The perspective projection already
    performs the depth shrink, so multiplying the footprint by `parallax` as the
    2D path does would compound two mechanisms for one cue and a distant pet would
    shrink twice. `mesh_draw.rs` takes the unscaled footprint on purpose and says
    so.

35. **`:DistractDownload` verifies a checksum and there is no way off that
    path.** `engine_download.lua` fetches the `.sha256` the release publishes
    beside each archive, hashes the archive with `vim.fn.sha256` over a libuv
    read, and refuses to unpack — let alone `chmod +x` — anything that does not
    match. If you add a platform, add it to the release matrix in
    `.github/workflows/ci.yml` *and* to `PUBLISHED_ARTIFACTS` in
    `tests/engine_download_spec.lua`, which is the only thing comparing the two
    lists. A name that drifts from the workflow is a 404 the user reads as "no
    release for my platform".

36. **No `v*` tag has ever been pushed, so there is no release to download
    yet.** The release job is gated on `refs/tags/v`, and every artifact URL is
    currently a 404. `curl` is invoked with `-f` for exactly this reason:
    without it curl writes GitHub's 404 HTML page into the archive and the
    failure surfaces as a confusing `tar` error instead of a download error.

37. **`vim.uv` is 0.10, and `doc/distract.txt` promises 0.9.** Use
    `local uv = vim.uv or vim.loop`, as `engine.lua`, `events.lua`, `warmup.lua`
    and `engine_download.lua` all do. The same applies to `vim.health.start`,
    which is 0.10 and is spelled `report_start` on 0.9; `health.lua` resolves
    all four reporters once at require time and is the only module allowed to
    touch `vim.health`.

38. **A warm-up is queued when the cache is cold, not when the source
    changed.** `distract.stop()` calls `warmup.reset()`, which drops the queue
    without running it, so a GIF decode can be cancelled with
    `gif_sources[asset]` already recorded. Gating `warm_gif_asset` on the source
    alone left that asset with no queued decode and no way to get one, and the
    first draw after a restart paid for the whole GIF synchronously — a frame
    hitch with no error anywhere. `warmup.request` deduplicates by key, so
    asking again while one is queued costs nothing. The voxel path was never
    affected: `warm_voxel_asset` re-requests unconditionally.

39. **The kitty id range restarts at `reset()`, and nothing recycles ids
    individually.** `M.reset()` deletes every transmitted image and then sets
    `next_offset = 0`, which is the whole of the fix; placements are only ever
    cleared in bulk, so a free-list would never have anything to hold. A
    free-list was added here once and was dead on arrival — nothing inserted
    into it — while reading as though exhaustion had been solved.

40. **The headless GPU suites skip when there is no adapter.** `gpu3d_headless`
    and `gpu_headless` both return early rather than failing, so green on a runner
    without a GPU says nothing about whether `shader3d.wgsl` even compiles. Run
    them locally before trusting a shader change.

---

## Architectural insights & trade-offs

- **The three bundled pets cost ~47 MB of git history, and both artifacts per pet are load-bearing.** The
  packed sheet (4 MB) is what the overlay backend draws from; the `.rgba` sidecar
  (11.8 MB) is the only form the in-terminal backends can decode, because pure Lua
  can read a GIF but not a PNG. Dropping a sidecar causes the pet to fall back
  silently to the procedural cat. Pre-fitting sidecars at build time would reduce
  kitty backend resolution fidelity, which is why dual-resolution assets exist.

- **`assets/codex_pets/` is gitignored on purpose.** 236 MB of third-party artwork
  with no stated licence, kept on disk as local test material only. `imported/`
  regenerates from `sheets/` via `tools/codex_pets/`.

- **Licensing dictates bundled pets.** `gudong` (CC BY 4.0), `iris` (MIT), and `minty` (MIT)
  are original characters from [`legeling/awesome-codex-pet`](https://github.com/legeling/awesome-codex-pet)
  credited in [`ATTRIBUTION.md`](ATTRIBUTION.md). Everything else in that gallery is
  franchise fan art or unlicenced, and cannot be bundled.

- **Steady-state performance benchmark.** 200 walking cats step and draw in 4.8 ms
  per frame in 3D vs 4.1 ms in 2D (14% vs 12% of a 30 FPS frame). An idle 3D world
  costs 1.0 ms vs 0.9 ms. Re-run `tools/bench_render3d.lua` before altering render
  hot paths.
