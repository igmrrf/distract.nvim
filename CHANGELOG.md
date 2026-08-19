# Changelog

All notable changes to **distract.nvim** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

Nothing yet. `distract.nvim` is feature locked, so changes here are
improvements — performance, reliability, cross-platform hardening, bug fixes
and tests. See the pending-work checklist at the bottom of `README.md`.

---

## [0.1.0] - 2026-08-19

First tagged release. The `0.1.0` and `0.2.0` headings that previously appeared
below were written before any tag existed and never pointed at one; their
contents are folded into this release as the two passes that produced it.

### Added
- **Background sliced warmup worker** (`distract.warmup`). A non-blocking background
  coroutine worker running on 16ms timer intervals with an 8ms budget per slice
  that incrementally decodes GIF frames (via `opts.on_frame`) and warms 3D voxel
  poses on `bind_manifest`. Eliminates the first-draw main-thread hitch for GIF
  and dense 3D imported models. Added unit suite `tests/warmup_spec.lua`.
- **Extracted sprite parity comparator** (`engine/src/sprite_parity.rs`). Exported
  from `engine/src/lib.rs` (§2.7 on the ecosystem roadmap), allowing downstream
  tools and external test suites to reuse exact tolerance assertions and dump
  formatting without reimplementing comparator rules.
- **A 3D render mode, on every backend.** `render.mode = "3d"` (or
  `:DistractRender 3d`) draws every entity as a voxel model instead of a flat
  sprite. There is no second set of 3D assets and no mesh format: every asset
  already resolves to RGBA frames, so a frame's opaque pixels are extruded into a
  slab of cubes — a real model of that frame, built from the art the asset already
  has. The built-ins, imported spritesheets, GIFs and anything registered through
  `register_asset` all work in 3D with nothing authored twice. A face is only
  emitted where the neighbour that would hide it is transparent, so a solid frame
  costs two quads a pixel plus its silhouette.

  This supersedes an earlier decision that "2D is the contract, 3D is not
  built". The reasoning that decision gave — that 3D must not fork a backend off
  the manifest contract, and must not mean authoring every asset twice — is what
  this design satisfies rather than ignores, and those two remain the constraints
  any future rendering mode has to meet.

  - **Nothing about the simulation changes.** Placement, floors, obstacles,
    wrapping and an asset's cell footprint are identical in both modes: a model is
    fitted into the sprite's own canvas rather than given one of its own, because
    the footprint is what the physics measures against. A model drawn face-on
    covers *exactly* the pixels its sprite does — asserted, not assumed — so
    turning 3D on never moves or reshapes a pet.
  - **`z` becomes real depth.** The perspective projection performs the shrink
    `parallax` fakes in 2D, so `parallax` keeps damping motion and stops
    multiplying size; two mechanisms scaling one sprite would compound. The camera
    sits at `(height / 2) / tan(fov / 2)`, which is what makes the `z = 0` plane
    map 1:1 to pixels.
  - **`overlay`** draws models on the GPU: a second render pass with a real
    `Depth32Float` buffer, one instanced draw per (asset, frame) group, and one
    directional light plus an ambient floor shaded per face. Its own pass, because
    a pass's depth attachment applies to every pipeline in it and depth-tested
    models cannot share one with painter-ordered sprites. It runs first and the
    sprite pass loads rather than clears, so a flat sprite draws over the models.
  - **`halfblock` and `kitty`** rasterise the same models in Lua into the sprite
    canvas, z-buffered, under an *orthographic* projection — a model spans about
    thirty sprite pixels, so a perspective divide across it moves nothing by a
    whole pixel, and an orthographic one lets a rasterised frame be cached and
    reused wherever the pet is on screen. In the steady state a 3D pet costs a
    table lookup per draw exactly as a 2D one does: 200 walking cats step and draw
    in 4.8 ms a frame against 4.1 ms in 2D. A backend that could not do 3D would
    fork the manifest contract, which is what the superseded decision was right to
    refuse.
  - **A manifest may pin its own mode** with `render = "2d"` or `"3d"`, which wins
    over the configuration in both directions. Flat overlay furniture — a speech
    bubble, a badge — stays flat in a 3D session, and the two passes composite
    together.
  - **`voxel_parity`** pins the two meshers to each other vertex for vertex,
    exactly, with no tolerance: nothing in a mesh goes through a float computation
    whose width matters. Each of the six fixtures declares its own source grid
    rather than meshing an asset, because sprite art is only equal across the
    engines within a measured drift — with a declared grid the meshing is the only
    variable. Verified to bite: reversing the order two faces are emitted in fails
    four fixtures.
  - **`gpu3d_headless`** drives the real pipeline and the real shader into an
    offscreen target and reads the pixels back, so "3D renders" is a measurement.
    It also writes four PNGs into `tests/screenshots/`, because `HANDOFF.md`
    records what judging art without looking at it costs.
  - Configuration, `:DistractRender`, `distract.set_render` and
    `distract.get_render` are documented at `:help distract-render` and in
    `docs/configuration.md`. `tools/bench_render3d.lua` is where the cost numbers
    come from and `tools/preview_sprite.lua --3d` shows a model as text.
- **Three new built-in pets** — `gudong`, `iris` and `minty` — original characters
  from the [`legeling/awesome-codex-pet`](https://github.com/legeling/awesome-codex-pet)
  gallery under licences that permit redistribution (CC BY 4.0 and MIT), credited
  per artist in [`ATTRIBUTION.md`](ATTRIBUTION.md). Nine states and 74 frames
  each, imported through the existing pipeline. The rule is the licence, not the
  source: everything else in that gallery, and everything under
  `assets/codex_pets/`, is franchise fan art or states no licence, and none of it
  is bundled.
- **A second source for the pet import tooling.**
  `tools/codex_pets/scrape_pets.py --source awesome` reads the gallery over the
  GitHub contents API. Its `pet.json` carries no atlas metadata, so
  `awesome_source.py` derives the grid from the sheet's own WebP header against
  the fixed 192×208 cell and identifies the sprite version by row count; a sheet
  that is not a whole grid is skipped with the reason rather than imported against
  a guessed row mapping. Each pet's declared licence is copied into the catalogue
  so it travels with the material. `pet_layout.py`, `import_pets.py` and
  `verify_layout.py` are unchanged, which the import of a real gallery pet
  verifies.
- **`tests/builtin_assets_spec.lua`** — one suite over every shipped asset,
  discovered from the plugin rather than listed, so a new built-in is covered the
  moment it is added: its manifest resolves, its art loads at a footprint that
  fits a terminal, every state points at frames that exist, its own capability
  gate accepts it, and it spawns, animates and draws.
- **A "bring your own pet" guide** in `docs/importing-assets.md`: the exact
  `import_sprite` invocation for a codex-pets or awesome-codex-pet download, and
  what to check before redistributing anything.
- **A plugin hook pipeline** (`register_plugin`). Seven lifecycle hooks —
  `on_init`, `on_tick`, `on_state_change`, `on_collision`, `on_editor_event`,
  `on_draw`, `on_teardown` — dispatched in registration order. The entity a hook
  receives is a read-only proxy and every mutation goes through a world command
  (`request_state`, `apply_impulse`, `despawn`, `mark_dirty`) applied at the top
  of the next step, because the in-terminal backends simulate in Lua while the
  overlay simulates in its own process: a hook that assigned `entity.vx` would
  have moved the sprite on two backends out of three. The overlay reports state
  changes and collisions back over IPC from a bounded journal, and subscribes to
  world snapshots only while a plugin is actually listening. A hook that raises is
  reported once and its plugin is disabled for the session. See
  `:help distract-plugins`.
- **A spatial obstacle provider** (`register_obstacle_provider`). Rectangles in
  terminal cells, typed `solid_platform` or `hazard`, collected in Neovim on a
  debounced cadence and pushed to whichever engine is running — the same rule the
  floor follows, because only the editor can read a buffer. A platform is a
  one-way floor a falling entity lands on and a grounded entity walks along; a
  hazard turns an entity around. Bounded at 128 rectangles, with a malformed one
  refused rather than reaching the physics. Four new physics-parity fixtures pin
  the rules on both engines. See `:help distract-obstacles`.
- **Buffer-scoped positioning** (`positioning.scope`). `"editor"` (the default,
  unchanged behaviour), `"window"`, `"buffer"` — the window's text area with the
  gutter taken off — or `"absolute"`. Wrapping, bouncing and clamping all measure
  against the resolved rectangle on both backends; the overlay is told about it
  with a new `UpdateViewportScope` command and clips its render pass to it. Three
  new parity fixtures pin a scoped origin. `positioning.exclude_floating` and
  `exclude_filetypes` hide a sprite that would cover a floating window or a listed
  filetype, and `positioning.z_index_offset` (default 40) puts sprite surfaces
  below LSP hovers rather than over them.
- **Instance visibility scoping** (`restrict_to_instance`, on by default).
  Sprites are hidden while the owning Neovim instance does not have focus and
  shown again when it regains it; the simulation keeps stepping, so an entity
  halfway through a wrap is not stranded. Set it `false` to keep the old
  full-screen behaviour, which is what a standalone desktop animation wants.
- **Seamless toroidal wrap.** A wrapping entity is now drawn at both edges at
  once instead of appearing to stop at the edge and then pop: the in-terminal
  renderer places one float per slice, scrolled to that slice's own corner of the
  frame buffer, and the overlay emits complementary quads in the same instanced
  draw with the pass scissored to the bounds. A sprite leaving a corner is drawn
  four times, which is the case the tests cover first.
- **`examples/plugins/`** — two working reference plugins, one per extension
  surface, that exercise every hook and turn function headers and closed folds
  into platforms a pet walks along.
- **A tick-cost benchmark**, `engine/tests/tick_budget.rs` and
  `tools/bench_tick.lua`, which is what the ecosystem roadmap §2.5 gated ambient weather
  on. Measured: 200 entities cost 0.074 ms per tick in the overlay (0.4% of a
  60 FPS frame, debug build) and 4.0 ms per frame stepped-and-drawn in the
  terminal (12% of a 30 FPS frame), both linear in the entity count. Weather can
  be a plugin; it does not need a batched particle path in the core.
- **`tools/preview_sprite.lua`** — dumps an asset's frames as text, so a
  silhouette can be judged from a headless run.

- **A sprite import pipeline** (`import_sprite`, a new binary in the engine
  crate). Turns a GIF, a folder of PNG frames, or a pre-packed atlas
  (`--spritesheet` with `--cell` and `--row-counts`) into three artifacts from one
  decoded frame set: a background-removed spritesheet PNG for the overlay
  backend, a raw-pixel `.rgba` sidecar for the kitty backend, and a Lua manifest
  scaffold. Background removal is a four-corner flood fill with a feathered edge
  rather than a binary cutout, and a frame that already carries its own alpha
  cutout is detected and passed through untouched. See
  [`docs/importing-assets.md`](docs/importing-assets.md).
- **Native-resolution sprites on the kitty backend.** `backends.lua` gained a
  `native_resolution` capability, and a manifest may declare
  `spritesheet.native_path` pointing at a `.rgba` sidecar. Backends that can
  show real pixels get it; the half-block backend keeps its cell-grid art and is
  provably unaffected. The sidecar is resolved as a fourth art source ahead of
  `sprite_sources`' per-asset cache, so two backends asking for different
  fidelity for the same asset cannot leak into each other.
- **`lua/distract/native_sprite.lua`** — the `.rgba` reader. Byte arithmetic
  rather than a parser (LuaJIT has no `string.unpack`), cached by path, and a
  missing or malformed file is an expected `nil, err` failure that falls back to
  the asset's other art instead of stopping the render loop.
- **Full configuration reference** at
  [`docs/configuration.md`](docs/configuration.md), covering the plugin config
  table, backends, commands, the Lua API, the manifest schema and the engine.
- **codex-pets import tooling** (`tools/codex_pets/`, development only) and a
  reference for that sheet format at
  [`docs/codex-pets-sprite-layout.md`](docs/codex-pets-sprite-layout.md).

### Changed
- **Entity construction moved out of `engine.lua`** into `entity_spawn.lua`, and
  frame sourcing out of `terminal_sprites.lua` into `frame_source.lua`. Both are
  structural, and both landed behind tests written first:
  `spawn_characterisation_spec.lua` pins what a spawn produces field by field,
  because the physics fixtures cover the step and barely touch the spawn.
- **The three built-in manifests moved to `engine/src/manifests/`**, one file each,
  and the hand-written `SpritesheetConfig` deserialiser to
  `engine/src/spritesheet.rs`. `manifest.rs` comes down from 1,458 lines to 719.
  `AssetManifest::default_cat` and its siblings are still the entry points, so
  nothing that reads a built-in manifest changed; the sprite- and physics-parity
  goldens confirm it.
- **`GpuRenderer::sync_atlas` is now `sync_assets`**, because it uploads the voxel
  meshes as well as the sprite atlas. Neither half does any work when what it
  depends on has not changed, and the mesh half does none at all in a session that
  draws no models.
- **Silhouette-first art, every built-in asset.** The analytical shading model
  produced gradients that did not survive 24x16: at that size a sprite is 24
  columns by eight half-block rows, and the cat read as a fox. All three assets
  are now flat fills inside a one-pixel contour with two or three tone bands. The
  cat gained upright ears with a real gap between them, a fore/hind leg
  distinction and a tail thick enough to read as its motion cue; the crab gained
  pincers with daylight between the prongs; the sun gained rays two pixels across
  and an eclipse that is distinguishable from the shining pose. New `blob`,
  `limb` and `rect` primitives in both sprite generators are the flat vocabulary.
  Measured: **118** live highlight groups for all 79 built-in frames, against
  1,894 before — 3% of `max_highlight_groups` where it was 46%. Cross-engine
  drift more than halved, to 97 pixels in 27,136, with **zero** unexplained
  pixels on every asset, so the two previously-unexplainable sun pixels are gone.
  Two things had to be got right for any of it to read on screen, neither of them
  visible in a character grid: a contour has to be stamped as the shape's actual
  rim rather than as a filled disc with a smaller disc inset (the radii quantise,
  so a head-sized shape came out solid outline), and the rim has to be a darker
  tone *of the fill* rather than near-black, because a near-black outline
  disappears into a dark editor background and takes the silhouette's edge with
  it. `tests/screenshots/` is regenerated and is how this was judged.
- `draw_tail`'s sixth segment is removed on both engines. It drew nothing: its
  centre landed off the canvas with a radius under a pixel, and any sliver was
  already covered by the fifth.
- **A malformed engine argument now exits non-zero** after reporting
  `INVALID_ARGUMENT`. `jobstart`'s `on_exit` treats 0 as a clean shutdown, so an
  engine that refused its own arguments looked exactly like one the user had
  stopped.
- **`engine.lua`'s per-entity frame moved to `lua/distract/entity_step.lua`**,
  taking that module from 1,012 lines to 780 and turning a 200-line `M.step` into a
  64-line one that coordinates. Structural only: the physics-parity goldens did not
  move, which is what the harness is for. `renderer.lua` (635), `external.lua` (448),
  `sprite_gen.lua` (445) and `ecs.rs` (2,168) are still over the 400-line cap, each
  around one function whose locals every branch shares; `HANDOFF.md` records which
  and why.
- The engine binary's command handling moved out of `main.rs` into
  `commands.rs`, `response.rs`, `subscription.rs`, `bounds.rs`, `journal.rs`,
  `obstacles.rs` and `wrap.rs`; the Lua side gained `kinematics.lua`,
  `placement.lua`, `viewport.lua`, `visibility.lua`, `plugins.lua`,
  `obstacles.lua`, `engine_binary.lua`, `overlay_grid.lua`, `overlay_report.lua`
  and `overlay_plugins.lua`. `external.lua` came down from 537 lines to 368; the
  new modules are all inside the size caps.

- **`assets/cat_walking/`** was regenerated through the new pipeline. Its
  spritesheet keeps its 128x72 / 8x4 geometry but no longer carries an opaque
  background, and it now ships a `.rgba` sidecar alongside it.

### Fixed
- **`register_asset` now re-pushes the configuration**, so a manifest registered
  after `setup()` reaches the backend. A backend keeps the snapshot of `config` it
  was set up with, so the spawn used to fall through to
  `require("distract.manifests." .. name)`, fail, and draw the cat under the
  asked-for name — the exact failure `register_asset` exists to prevent. Only the
  art half held, because `terminal_sprites` is a live registry.
- **The `luacheck` CI gate was failing, and nothing local could see it.** luacheck
  1.2.0 cannot run under Lua 5.5 at all — it dies inside its own `standards.lua`
  before reading any project file — so the gate looked absent locally while CI's
  plain `luacheck lua plugin tests` step exited non-zero on 24 warnings. All 24 are
  fixed: locals left dead by earlier extractions, sprite palette aliases nothing
  read, three shadowed names, one over-long line, and five specs that re-`require`d
  a module their file scope already held. `HANDOFF.md` records how to run luacheck
  locally against Lua 5.1. Behaviour is unchanged; every golden and all 530 tests
  are untouched.
- **A duplicate `*distract-capabilities*` help tag** made `helptags` fail outright
  and `:help distract-capabilities` ambiguous. The asset-declaration one is now
  `*distract-asset-capabilities*`.
- The manifest scaffold emitted state names as bare Lua table keys, so any name
  that is not a plain identifier — anything hyphenated, like `running-right` —
  produced a file that would not parse. Such names are now bracketed.
- **The release build could never publish.** Three separate faults, each
  masking the next because `cargo test` stops at the first failing test binary:
  the engine built its winit event loop before parsing argv, so a display-less
  Linux runner killed it before it could report a bad argument; the physics
  goldens were compared with an absolute bound that `f32::exp` differing
  between Apple's libm, glibc and the MSVC CRT exceeded on Windows by step 12
  of `accel_floorless`; and the x86_64 macOS job asked for `macos-13`, a runner
  label GitHub has retired, so it queued indefinitely. Argument parsing now
  happens first, golden drift is bounded relative to each sample's magnitude,
  and the x86_64 binary is cross-built from the arm64 runner.
- **CI jobs could hang for hours.** `apt-get` ran with no non-interactive
  guards and no acquire bounds, so a needrestart prompt or a stalled mirror
  held a runner until the six-hour ceiling, and nothing cancelled superseded
  runs. The dependency install is now one composite action carrying those
  guards, every job has a timeout, and a concurrency group cancels superseded
  runs — except on tags, where killing a release build half way through
  publishes nothing.
- **`:DistractDownload` installed an unverified binary.** It fetched an archive
  over the network, unpacked it and marked it executable without checking what
  it had received. The release workflow already publishes a `.sha256` beside
  every artifact, so the archive is now hashed and compared before anything is
  unpacked, with no flag to skip the check. `curl` also gains `-f`: without it a
  404 writes GitHub's error page into the archive and the failure surfaces as a
  confusing `tar` error.
- **Download failures could hang silently.** `jobstart` returns a non-positive
  id when the binary is missing and then never fires `on_exit`, so a missing
  `tar` left the chain waiting with nothing reported. Every step now checks
  `executable()` and the `jobstart` return, a `chmod` refusal is reported rather
  than swallowed by `pcall`, tar's exit code is no longer taken as proof the
  binary arrived, and the in-progress lock is released on every path out.
- **A GIF warm-up cancelled by `stop()` never came back.** `warmup.reset()`
  drops the queue without running it, and the warm-up was gated on the asset's
  source having changed — so a restart re-read the same manifest, saw no change,
  and left the first draw to decode the whole GIF synchronously. The gate is now
  whether the cache is cold.
- **`:checkhealth distract` errored on Neovim 0.9**, which spells the health
  reporters `report_start` rather than `start` — before reaching its own "older
  than 0.10" warning. `warmup.lua` indexed `vim.uv`, which is nil on 0.9, for
  the same reason. Both now match the `vim.uv or vim.loop` convention the rest
  of the plugin already followed.
- **An obstacle provider's id was its list index**, so unregistering one shifted
  every later provider's id onto a different provider. Ids are now stable.
- **The `luacheck` gate had gone red again** on four locals left dead by earlier
  extractions. CI runs it with no ratchet, so one unused local fails the build.
- A kitty free-list claimed to recycle image ids while nothing ever inserted
  into it. Resetting the id range on `reset()` is the actual fix and is kept.

### Added
- **Every GitHub Action raised to the node24 runtime.** Two were still on
  node16 and five on node20, running only because the runner force-upgrades
  them. Each pinned ref was checked for `runs.using: node24` and each major
  bump checked against how this workflow uses it.
- **`:checkhealth distract`** reports the terminal environment, whether an
  engine binary is installed and whether one is published for this platform,
  the active backend and render mode, live highlight groups, and registered
  assets and plugins.
- **GIF assets on every backend.** A manifest whose `spritesheet.path` ends in
  `.gif` is drawn per-pixel on `kitty` and `overlay` and in half-blocks in the
  terminal, with no per-backend branching. Decoding in the terminal is pure Lua
  (`lua/distract/gif/`) — GIF87a and GIF89a, LZW, interlacing, global and local
  palettes, the transparency index and disposal methods 0-3 — so no engine
  binary and no external process is involved. `spritesheet.frame_width` and
  `frame_height` are the size the sprite is drawn at, in sprite pixels, on every
  backend; a screen-sized animation is resampled to them.
- **Source frame timing.** A state's `animation.fps` wins as before; a state
  that declares none is timed by the per-frame delays stored in the GIF. Both
  engines apply that precedence (`frame_duration_seconds` in
  `lua/distract/engine.lua` and `engine/src/ecs.rs`).
- **Palette quantisation for imported art** (`lua/distract/quantise.lua`,
  frequency-weighted median cut) and a **ceiling on live highlight groups**
  (`lua/distract/highlights.lua`). Groups belong to the asset that asked for
  them; at `max_highlight_groups` the least recently drawn asset's groups are
  cleared and its cached frames dropped, and the asset being drawn is never the
  victim. New config: `max_sprite_colours` (128), `max_highlight_groups` (4096).
- **The kitty graphics-protocol backend** (`lua/distract/kitty/`): real RGBA
  sprites with per-pixel alpha, drawn by the terminal, in exactly the cells the
  half-block renderer would use. Offered only when the terminal answers the
  `a=q` query and `termguicolors` is set; `ghostty` and `wezterm` are aliases.
- **Placement vocabulary**: `position.anchor` (`auto`, `bottom`, `top`, `free`,
  or an explicit `{x, y, z}`), `position.ground` (`screen` or `text`), a `z`
  axis that is both draw order and parallax depth, and a backend capability
  table (`lua/distract/backends.lua`) that degrades parallax explicitly on a
  backend that cannot scale a sprite. An asset may declare its own `anchor`.
- **Locomotion classes and capability gating** (`grounded`, `ballistic`,
  `omnidirectional`), parametric paths (`sine`, `orbital`, `lissajous`,
  `bezier`), `transitions.on_land`, and spawn options for position and facing.
  A manifest that asks for motion it declared it cannot do is refused once, at
  spawn, with the same words on either backend.
- **A cross-engine physics parity harness.** `engine/tests/physics_parity.rs`
  generates goldens in `tests/fixtures/physics/` and `tests/physics_parity_spec.lua`
  asserts the Lua engine reproduces them, in terminal cells, so the two
  implementations cannot drift apart unnoticed. `engine.step(dt, bounds)` is the
  injected-`dt` seam that makes it possible.
- **Shared procedural sprite art on both backends.** `sprite_gen` and the cat,
  crab and sun assets are ported to Rust (`engine/src/sprite_gen.rs`,
  `engine/src/sprites/`), so the overlay draws the same pose curves and the same
  hemisphere shading as the terminal. The overlay went from 4 frames per asset to
  29/25/25, and the Rust default manifests derive their frame lists from the
  ported layout instead of hardcoding indices.
- **Instanced GPU renderer.** Sprites are drawn as instanced textured quads from
  an atlas uploaded once. Per-frame upload went from a full-screen framebuffer
  (33 MB at 4K, ~2 GB/s at 60 fps) to 32 bytes per visible entity, and redraws
  are skipped entirely when nothing is moving.
- **Cursor attention.** Editor events now carry the cursor's screen position, and
  an entity picking up a moving state turns to face it.
- **Per-entity desynchronisation at spawn** on both backends, so entities of the
  same type spawned together are no longer perfectly in step.
- **`:DistractBuild`** builds the overlay engine in the background.
- **`doc/distract.txt`** — `:help distract` now works.
- **Configurable overlay cell size** (`cell_width`, `cell_height`), with a
  `CSI 16 t` terminal query and a documented 10x20 default.
- **Headless GPU tests** (`engine/tests/gpu_headless.rs`) that run the real
  shader on a real device, plus asset-loading, atlas-packing and
  instance-building tests.
- **Lint and format gates in CI**: `cargo fmt --check`, `cargo clippy -D
  warnings`, `stylua --check` and `luacheck`, with `.stylua.toml` and
  `.luacheckrc`. The same gates run in `.pre-commit-config.yaml`.

### Changed
- **The backend is chosen for you when you name none.** `config.backend`
  defaults to unset, and unset resolves to `kitty` where the terminal speaks the
  graphics protocol and `halfblock` everywhere else. Naming one in `setup()` or
  with `:DistractBackend` still wins, and is remembered across a later `setup()`
  that names none.
- **The overlay honours a declared GIF frame size.** `load_gif` resamples to
  `spritesheet.frame_width`/`frame_height` instead of drawing the source canvas,
  so one manifest describes one footprint on every backend. A GIF canvas may be
  up to 4096 px per side before resampling; the drawn frame is still bounded at
  1024.
- **`distract.terminal_sprites` is decomposed.** Art sourcing moved to
  `distract.sprite_sources`, the scratch-buffer cache to
  `distract.frame_buffers`, and manifest path resolution — shared with the
  overlay's IPC payload — to `distract.asset_path`. The public surface is
  unchanged.
- **Manifest units are defined and shared.** Positions and velocities are in
  sprite pixels, per frame at 60 FPS, where one sprite pixel is one terminal cell
  wide and half a cell tall. Both backends convert from that unit; they
  previously applied unrelated ad-hoc factors (`dt * 60` against `dt * 15` and
  `dt * 30`), so one manifest moved at different speeds on each backend. The
  cat's jump was retuned once for the shared unit (`jump_impulse_y` -4.0 -> -2.2,
  `gravity` 0.15 -> 0.32).
- **The overlay engine refuses to start on X11** rather than opening a
  fullscreen always-on-top window that captures every mouse click.
- **The overlay engine is no longer built synchronously** on first start.
  `:DistractStart` reports what to run instead of freezing the editor for the
  length of a cold Rust build.
- **`:DistractClear` no longer stops the in-terminal engine**, matching the
  overlay backend's `ClearAll`.
- **The cat is procedural on both backends.** It no longer references
  `assets/cat_sprite.png`, which is a 4-frame sheet that made all 29 manifest
  indices collapse modulo 4.
- Sprite rasterisation is deferred to first use. Loading the plugin went from
  10.5 ms to 0.73 ms.
- Asset frames are stored once; mirroring happens at draw time instead of
  keeping a flipped copy of every frame for the process lifetime.

### Fixed
- **`distract.kitty.reset()` left the renderer answering for a backend the
  registry no longer offered**, which is the on-paper-only backend the two
  registries are kept in step to prevent.
- **Overlay window captured all mouse input on X11.** `set_cursor_hittest`
  returns `Err(NotSupported)` there and the result was discarded, leaving a
  fullscreen input trap dismissible only by killing the process.
- **Overlay coordinates matched nothing on screen.** The terminal cell size was
  hardcoded to 10x20 on both sides.
- **Every sprite rendered washed out.** The GPU sampled a non-sRGB texture and
  wrote to an sRGB surface.
- **Semi-transparent pixels composited at `rgb * a^2`.** The pipeline emitted
  premultiplied colour into a surface declared straight-alpha.
- **The overlay never recovered from `SurfaceError::Outdated`** or from
  `ScaleFactorChanged`, so a monitor or DPI change left it permanently stale.
- **The GPU surface was configured from the requested window size**, not the
  size the window manager granted.
- **Despawned entities were not reported to Neovim**, so `:DistractStatus`
  disagreed with reality.
- **No bound on spritesheet or GIF size.** A large GIF was decoded in full at
  source resolution; frame dimensions were taken from whichever frame decoded
  last. Now capped at 1024 px per side, 512 frames and 256 MiB, with frames
  validated against each other.
- **Assets were re-decoded on every spawn**, and load errors were discarded so a
  broken spritesheet path silently degraded to procedural art.
- **`idle` never reached the in-terminal backend**, making `idle_timeout_ms` dead
  config for the default backend.
- **Event debouncing was defeated in insert mode**, where `typing` and `moving`
  alternate on every keystroke and short-circuited the single shared throttle
  flag. Throttling is now per event name.
- **Event timers leaked a libuv handle per setup/teardown cycle.**
- **`WrapMode::Wrap` was gated on velocity** on both backends, so an entity whose
  velocity decayed while off-screen stayed there invisible forever.
- **The in-terminal backend hardcoded a 16-cell sprite width** for boundary
  checks against real widths of 24, and silently ignored the `despawn` and `none`
  wrap modes.
- **`nvim_win_set_config` was called every tick per entity**, forcing a redraw
  even for a stationary sprite. Half-block frame rendering is also cached at
  `(asset, frame)`.
- **`set_backend` left the plugin stopped** with no indication a restart was
  needed.
- **`VimLeavePre` was registered without a group**, accumulating a duplicate on
  every `setup()`.
- **A custom action without `target_state` set `current_state = nil`** and broke
  the next tick.
- **`send_command` restarted a stopped overlay engine**, so `:DistractClear`
  after `:DistractStop` respawned the process.
- **The binary and the library each compiled their own copy of every module**,
  running the whole test suite twice.
- **Published release binaries were unreachable**: nothing looked anywhere they
  could be installed. `engine/bin/distract-engine` is now searched first.
- Stale `float` backend references in `:DistractBackend` help and config
  comments; `is_overlay()` tested a value `normalize_backend` can never return.
- CI ran the Lua suite twice over the same directory.

---

### Removed
- `engine/tests/parity_dump.rs`, superseded by `engine/tests/sprite_parity.rs`.
  It was an `#[ignore]`d development aid that dumped geometry rather than pixels.

---

### The multi-backend rendering pass

#### Added
- **Multi-Backend Rendering Architecture**:
  - `halfblock` (Default): High-fidelity in-terminal 24-bit Truecolor pixel-art renderer using Unicode half-blocks (`▀` / `▄`) and native Neovim floating windows with zero external OS window overlays.
  - `overlay`: Hardware-accelerated WGPU transparent desktop overlay window.
- **Hardware-Accelerated WGPU Rendering Engine**:
  - Native WebGPU/Metal rendering pipeline with explicit `CompositeAlphaMode::PostMultiplied` and `PreMultiplied` support, eliminating solid black background artifacts on macOS Retina/Metal surfaces.
  - Custom WGSL textured quad shader pipeline.
  - Configured macOS AppKit `NSWindow` and `CAMetalLayer` with `[layer setOpaque:NO]`, `[ns_window setIgnoresMouseEvents:YES]`, and clear background colors.
- **In-Terminal Manifest ECS & Physics**:
  - Full Lua-driven ECS state machine, timer transitions, custom action dispatching (`jump`, `yawn`, `clip`, `burrow`, `eclipse`, `flare`), gravity physics, and boundary handling inside the Neovim editor grid.
- **User Commands & Tooling**:
  - Added `:DistractBackend [name]` with dynamic shell autocompletion for live backend switching.
  - Enhanced `:DistractToggle` to support both in-terminal and external overlay engines.

#### Fixed
- **Half-block renderer was non-functional.** Transparent pixel-matrix cells used `nil`, which truncated every sprite row at the first hole, so `#lines[1]` was `0` and `nvim_open_win` raised `Invalid 'width'` on every tick at 30 FPS with no error handling.
- Extmark highlight columns were character indices rather than byte offsets; each half-block glyph is 3 bytes, so all colour landed in the first few cells and `end_col` split codepoints.
- `animation.frames` was ignored by the in-terminal renderer, which indexed art by animation position instead of sheet index — a sleeping cat drew its idle art and the sun's eclipse drew its shining art.
- The Lua test runner exited 0 on failure (a trailing `-c "q"` in CI masked the error) and hung headless Neovim when the report raised. It now exits non-zero via `:cquit`.
- Repeated render failures now stop the engine and report once instead of looping at the frame rate.
- The overlay compositor silently skipped drawing when a manifest frame index exceeded the loaded sheet, rendering the entity invisible; it now wraps.

#### Changed
- **Sprites are generated procedurally** by `distract.sprite_gen` and `distract.sprites.*` instead of being stored as hand-authored pixel matrices. States are pose curves, so animation is smooth by construction; volume comes from a lit-hemisphere shading model with rim and specular terms. Frame counts went from 4 per asset to 29 (cat), 25 (crab), 25 (sun), giving every state its own art.
- Manifests reference the generated `layout` table rather than hand-written frame indices, so the two cannot drift apart.
- Sun `rising`, `setting` and `eclipse` are one-shot transitions rather than looping animations, which previously snapped back to the start pose.

#### Removed
- The ASCII `float` backend and all text-art sprite data. `float`/`ascii` resolve to `halfblock` with a warning.
- The `kitty` backend is no longer advertised — the graphics protocol was never implemented, and selecting it silently rendered plain ASCII. The alias resolves to `halfblock` with a warning.
- Dead `lua/distract/pets/cat.lua`, which nothing referenced.
- Modernized Rust crate dependencies (`wgpu 0.16`, `bytemuck 1.25`, `pollster 0.3`, `serde 1.0.229`, `image 0.24.9`).
- Switched default rendering backend to `halfblock` for seamless, out-of-the-box compatibility with Ghostty, tmux, SSH, and all terminal environments.
- Optimized process teardown during `VimLeavePre` with synchronous `jobwait` to prevent orphaned background processes.

---

### The initial engine and plugin

#### Added
- **Core Engine & Architecture**:
  - Rust background rendering engine with fixed 60 FPS delta-time tick loop via `winit`.
  - Porter-Duff alpha compositing for transparent sprite overlays.
  - Multi-threaded asynchronous JSON-RPC IPC layer over stdin/stdout channels.
  - Viewport dimension synchronization and grid coordinate transformation.
- **Entity State Machine & ECS**:
  - Multi-entity ECS architecture managing component systems, timers, velocity physics, and animations.
  - Autonomous behavior transitions (timeout transitions, editor event triggers).
  - Wrap modes (`wrap`, `clamp`, `bounce`, `none`) and parabolic gravity jump physics.
  - Mathematical sine-wave pathing with customizable oscillation phases.
- **Data-Driven Asset Manifests**:
  - Declarative Lua & JSON asset manifest schemas.
  - Built-in `cat` asset: idle, walk, sprint, parabolic jump, yawn, sleep, sit, and wake capabilities.
  - Built-in `crab` asset: horizontal scuttle, pincer snapping (`clip`), sand burrowing, and sleep.
  - Built-in `sun` asset: radiant solar corona, celestial arc movement, solar flares, sunrise/sunset, and solar eclipses.
  - Procedural geometric sprite generation fallback when no image asset is provided.
- **Neovim Plugin Integration**:
  - Throttled editor event listeners (`TextChanged`, `CursorMoved`, `WinScrolled`, `VimResized`).
  - User commands: `:DistractStart`, `:DistractStop`, `:DistractToggle`, `:DistractSpawn`, `:DistractAction`, `:DistractClear`, `:DistractStatus`.
  - Dynamic shell completions for assets and action capabilities.
- **Testing & Quality Assurance**:
  - 29 Rust engine unit tests covering IPC, ECS, compositor, manifests, and assets.
  - Visual headless screenshot capture test verifying state rendering.
  - 31 modular Neovim Lua integration tests for plugin lifecycle, events, IPC, manifests, and backends.
- **CI / CD Automation**:
  - GitHub Actions workflow for cross-platform Rust builds (Linux, macOS, Windows) and Neovim Lua tests.
  - Automated tag-based GitHub Releases packaging cross-platform binaries with SHA-256 checksums.
