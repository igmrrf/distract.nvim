# Handoff — what is still open

Working notes for whoever picks this up next. Rewritten 2026-08-19.

This file holds **only** open work and the traps that cost time. It is
deliberately not a record of what shipped:

- **What was built and why** — [`CHANGELOG.md`](CHANGELOG.md).
- **What is not built yet** — [`future.md`](future.md), and its program plan
  [`docs/superpowers/plans/2026-08-16-future-roadmap-master.md`](docs/superpowers/plans/2026-08-16-future-roadmap-master.md).
- **What the design says** —
  [`docs/superpowers/specs/2026-08-16-locomotion-position-kitty-design.md`](docs/superpowers/specs/2026-08-16-locomotion-position-kitty-design.md),
  including the unit contract and the backend/renderer split. Where this file and
  the spec disagree, the spec wins.
- **Which review findings are closed** — [`REVIEW.md`](REVIEW.md).
- **How to use the import pipeline** — [`docs/importing-assets.md`](docs/importing-assets.md),
  configuration in [`docs/configuration.md`](docs/configuration.md).

---

## Pending

| Item | State |
|---|---|
| §3 silhouette-first art redo, every asset | **the next piece of work — see below** |
| `draw_tail`'s sixth segment draws nothing, both engines | open, folds into the art redo |
| Nothing is pushed | `fix/assets` is ahead of `origin/fix/assets`, no PR; integration is the owner's call |
| No codex-pets asset is wired up as a real pet | 15 imported under `assets/codex_pets/imported/` with placeholder `physics`/`transitions` |
| `engine.lua`, `renderer.lua`, `external.lua` over the size caps | accepted debt, see below |
| `engine/tests/parity_dump.rs` superseded | `#[ignore]`d dev aid; can be deleted once nothing references it |
| An invalid engine argv value exits `0` | emits `INVALID_ARGUMENT` first and Lua surfaces it, so nothing is lost; a non-zero exit needs `jobstart`'s exit handling checked |

Everything in [`future.md`](future.md) §1–§5 is also unbuilt by definition.

---

## Verify the current state

All four gates pass. Run them before and after any change.

```bash
nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua
cargo test --manifest-path engine/Cargo.toml
stylua --check lua plugin tests
cargo clippy --manifest-path engine/Cargo.toml --all-targets -- -D warnings
```

Expected: **368 Lua tests**, **189 Rust tests** (147 lib + 31 import_sprite + 6
headless GPU + 2 physics parity + 2 sprite parity + 1 screenshot; `parity_dump`
is `#[ignore]`). The Rust count does not move with a new physics or sprite
fixture — one test function iterates the whole directory.

The Lua suite needs `-u tests/minimal_init.lua`; that is where the runtimepath is
set. Either `-l` or CI's `-c "luafile ..."` form works.

`luacheck` is listed in the README as a gate but **is broken on this machine** —
luacheck 1.2.0 under Lua 5.5 dies with `attempt to assign to const variable
'field_name'` before it reads any project file, and fails identically on files
nobody touched. Run it against an unmodified file to confirm before chasing it.
`stylua --check` is the real local Lua gate. CI may still run luacheck; a green
local run does not mean it passed.

---

## The next work: §3 silhouette-first art redo

Covers **every asset, existing and future** — not the cat alone. Three assets
times two implementations is six files that can drift, which is why the art
harness below was a precondition. It now exists, so this is unblocked.

The problem: at 24×16 a sprite is 24 columns × **8 rows**, and `sprite_gen.orb`
spends five lighting terms (Lambert, rim, fill, specular, dither) across a body
twelve pixels wide. At that size **silhouette is the only thing that reads** —
the cat currently reads as a fox. Ears are 2.4-pixel-wide triangles
(`cat.lua:72`, `draw_ears`), the four legs are identical capsules, whiskers and muzzle
are below the detail floor. Flat fills, a 1px dark contour and 2–3 tone bands
read better *and* collapse the highlight-group count.

Target reads are in [`future.md`](future.md) §3. Two things to fold in:

1. **`draw_tail`'s sixth segment draws zero pixels, on both engines.**
   `cat.lua:31` and `cat.rs:86` both loop `1..=6`. At `i = 6`, `tx` lands around
   0.0–1.2 depending on `stretch` and `curl` with a radius of 0.85, so the orb
   sits off the canvas's left edge and any sliver is already covered by segment
   5. Removing it changes not one pixel in 11,136 — the ports agree, so parity is
   intact, but `future.md` §3 names the tail as the cat's primary motion cue and
   its tip is being clipped.
2. **Highlight headroom is thinner than it looks.** All 79 built-in frames create
   **1,894** live highlight groups against a `max_highlight_groups` cap of
   4,096 — three assets consuming 46%. A quantised palette is what actually
   shrinks that.

Regenerate the art goldens as part of the change and read the drift numbers
rather than raising the budgets.

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
drawing step into the adjacent pixel: a *tiny* precision difference produces a
*large* colour difference, which a per-channel tolerance alone cannot describe. A
differing pixel is accepted when the other engine's value appears anywhere in its
3×3 neighbourhood, or when both are opaque and no channel differs by more than
24. Across all 79 frames the drift is 220 cells in 27,136 (0.81%), 93% of it
explained by the adjacent-cell rule. **No transcription error exists** — the
ports agree as closely as f32 and f64 allow.

**The budgets are measurements, not allowances.** `drifted` caps every pixel that
differs at all; `unexplained` caps the pixels no rule accounts for:

| Asset | Pixels | Drifted | Budget | Unexplained | Budget |
|---|---|---|---|---|---|
| cat | 11,136 | 31 (0.28%) | 39 | 0 | 0 |
| crab | 9,600 | 158 (1.65%) | 166 | 0 | 0 |
| sun | 6,400 | 102 (1.59%) | 110 | 2 | 2 |

Re-measure and update them alongside any regenerated golden. Do not raise one to
make a failure go away — read the reported pixel first.

**The neighbourhood rule is structurally blind to a difference an opaque layer is
drawn over.** Sun's two unexplained pixels are both at (7, 13), in `rising` frame
16 and `setting` frame 21, where `draw_horizon` cuts a gap at every
`(x + row) % 7 == 0` (`sun.lua:97`, `sun.rs:150`). Inside that one-pixel window
f32 places the disc's lower edge and f64 does not, and every adjacent pixel is
covered by the opaque band, so the gold shade has nowhere neighbouring to appear.
A third such pixel fails the budget, which is the intended behaviour.

---

## The physics-parity harness — read before touching physics

The recurring defect class in this project is `lua/distract/engine.lua` and
`engine/src/ecs.rs` drifting apart while both claim "one manifest describes one
behaviour on both backends". Three such divergences had to be found by reading
before the harness existed; it has since caught two more on its own.

`engine/tests/physics_parity.rs` generates the goldens;
`tests/physics_parity_spec.lua` asserts the Lua engine reproduces them. They meet
at the JSON in `tests/fixtures/physics/`, in **terminal cells**.

**Any change to physics on either side means adding a fixture.**

```bash
UPDATE_GOLDEN=1 cargo test --manifest-path engine/Cargo.toml --test physics_parity
```

Then run the Lua suite. If it disagrees, that is the point — read the reported
step index before assuming the fixture is wrong.

**Avoid knife edges, and say so in the fixture's own `description`** so nobody
"fixes" them back. `constant_velocity_wrap` uses `target_vx = 1.3` and
`path_bezier` uses `freq = 0.47` for this reason; the two frame-timing fixtures
with a bound spritesheet use `dt = 0.013` so no frame boundary lands within 1 ms
of a step. In each case f32 and f64 fall either side of an exact boundary — a
precision artefact, not a divergence.

**The two engines index `animation.frames` differently, on purpose.** Lua's
`frame_idx` is 1-based (`spawn` picks `math.random(1, count)`, the loop resets to
`1`); Rust's is 0-based (`% frame_count`, resets to `0`). Each indexes its own
convention correctly and both cycle the same number of frames. This is why a
fixture records the *resolved sheet index* rather than `frame_idx` — comparing
the raw index would fail on the convention and tell you nothing — and why the Lua
runner sets `e.frame_idx = 1` where the Rust runner sets `0`.

**Frame timing is fully covered; keep it that way.** A fixture may declare a
`spritesheet` block naming a GIF **relative to the repository root**, which is
the one path form both runners resolve (Lua joins it onto
`asset_path.plugin_root()`, Rust onto `CARGO_MANIFEST_DIR`'s parent).
`tests/fixtures/physics/frame_delays.gif` is that art — 209 bytes, four solid
24×16 frames, delays 40/120/80/200 ms. Three properties are deliberate and must
survive any regeneration:

- **No two delays are equal**, so a lookup that ignored the atlas index fails.
- **No delay is 100 ms**, so a run that fell through to `FALLBACK_FRAME_SECONDS`
  cannot land on the same trajectory. `cat_walking_1.gif` is uniformly 100 ms and
  would have made a vacuous fixture.
- **24×16 matches the size an unbound probe already reports**, so binding art
  changes the timing and nothing else about the trajectory.

The regeneration command is in the harness header. `fps` is 6.25 on the
precedence fixture — 0.16 s, matching none of the file's delays and not the
fallback either.

---

## Traps that cost time — read before debugging

1. **One asset has one cell footprint, and fidelity is independent of it.**
   `get_dimensions` takes no backend argument by design: sprite size feeds
   physics through `sprite_cell_size` (`engine.lua:214`), which is what wrapping
   and floor-anchoring measure against, so a per-backend answer makes one
   manifest describe two behaviours. An imported
   asset reports its *fitted* size; the native size is a fidelity detail.
   Kitty's `c`/`r` fields resample a transmitted image into a given cell box, so
   kitty loses nothing by honouring the fitted footprint.

2. **A kitty opacity mask must be built on the footprint grid, not the image
   grid.** `frames.spans` resamples the mask *from* `frame.cols` × `frame.rows`.
   Building it on the image's grid still produces the right *number* of rows
   while reading the wrong region — the top 17 pixel rows of 72 — and the sprite
   silently vanishes. `describe` therefore takes `rgba` from the native matrix
   and `mask` from the fitted one.

3. **Any test of a spatial mask needs art that varies in space.** The mutation in
   trap 2 initially slipped past its own test because the fixture was fully
   opaque and every candidate mask was identical.

4. **`terminal_sprites.lua:193` is the quantiser's only gate.** 32 unquantised
   imported frames go straight through the 4,096 highlight-group cap, so
   `needs_quantising` must stay true for sidecar-backed assets.

5. **macOS display detection matches by ID, never by coordinates or size.**
   `NSScreen.mainScreen`'s `NSScreenNumber` is a `CGDirectDisplayID` and so is
   winit's `native_id()`, so **no Cocoa-to-winit conversion is involved**. Do not
   reintroduce one: Cocoa's origin is the primary screen's bottom-left and
   winit's is its top-left. Matching by size breaks on two identical monitors.

6. **Neither engine measures its own floor.** `events.sync_floor` measures once
   and pushes the same number to `engine.set_ground_row` and
   `external.set_ground_row`. A change that has an engine call `position.floor_row`
   for itself reintroduces the divergence class the harness exists to catch. The
   one exception is a spawn naming its own `ground`.

7. **`engine.lua` holds `floor_row` as module state**, so a spec that spawns
   after another spec's push inherits its floor. `tests/physics_parity_spec.lua`
   calls `set_ground_row(nil)` before every fixture; any new spec asserting on
   `ground_y` must do the same.

8. **`engine.setup` merges with `vim.tbl_deep_extend("force", ...)`.**
   Registering two test manifests under the same asset name lets the first one's
   `physics` fields survive into the second. Every spec that builds probe
   manifests gives each test its own `probe_N` name for this reason.

9. **Test probes inherit the cat's manifest.** Both parity runners and several
   spec helpers start from `AssetManifest::default_cat()`, which declares
   `capabilities` and `locomotion = "grounded"` — so a probe that orbits is
   *correctly* refused. Clear both:
   `manifest.locomotion = None; manifest.capabilities = Default::default();`.

10. **Wall-clock `dt` in `engine.tick()`.** A tight loop of 20 ticks advances
    almost no simulated time. Use `engine.step(dt, bounds)` for anything that
    asserts on distance; `tick` is only for testing the timer path.

11. **A spawn randomises `frame_idx`, `frame_timer` and `path_phase`** so two
    entities spawned together do not move in lockstep. Any test asserting on
    elapsed animation time has to zero the first two afterwards;
    `tests/gif_assets_spec.lua` has `spawn_at_first_frame()` for it.

12. **`vim.fn.screenstring` lies inside `nvim -l` scripts.** It reads the current
    window's grid, not the composited screen, so floating windows appear at the
    wrong place or not at all. A **vanilla** float at `row=12, col=10` reproduces
    the artifact while `nvim_win_get_position` correctly reports `{12, 10}`;
    attaching a real UI via a pty does not fix it. Assert on
    `nvim_win_get_position` / `nvim_win_get_config` for float rows.
    `screenstring` **is** trustworthy for the extmark overlay path, because those
    are written into the current window's own buffer.

13. **`backends`, `position` and `distract.kitty` warn once, process-wide, and
    the registries are process-wide too.** `reset_warnings()`, `backends.reset()`
    and `kitty.reset()` exist for tests. A spec that registers kitty and does not
    put it back breaks `backends_spec`, which asserts the exact backend list; a
    spec that counts warnings without resetting counts zero, passes, and proves
    nothing. `kitty.reset()` also unregisters the renderer surface — leaving it
    registered is the on-paper-only backend the two registries are kept in step
    to prevent.

14. **The kitty test seam is `writer.set_writer`, and every spec that uses it
    must put it back.** A leaked capture silently swallows every subsequent
    escape and nothing fails — the assertions are all on what was captured. Use
    `captured()` / `with_kitty()` in `tests/kitty_spec.lua` rather than calling
    `set_writer` directly.

15. **`detect.is_available()` answers once and caches.** Headless it is always
    false, because there is no UI to answer the query. `detect.override(true)`
    gets a test past that; `detect.reset()` puts it back.

16. **`config.backend` is `nil` by default**, and nil means "pick the best this
    terminal can draw". A spec that wants the default path back has to assign
    `distract.config.backend = nil` before calling `setup()`, because a previous
    `setup()` resolved it to a concrete name and that is what "the user chose it"
    looks like from the inside.

17. **`tests/run_tests.lua` has an explicit `SPECS` list.** A new spec file that
    is not added to it silently never runs, and the suite still reports green.

18. **The harness is not Plenary.** `tests/test_harness.lua` provides
    `assert.are.same`, `assert.are_equal`, `assert.are_not_equal`,
    `assert.is_true/is_false/is_nil/is_not_nil/is_function`. There is no
    `assert.are.equal`, no `assert.is_not.same`.

19. **The harness compares with `vim.inspect`, which prints table identity.**
    `assert.are.same({ RED, RED }, row)` fails against a row of two equal but
    distinct tables, because the expected side prints
    `{ <1>{...}, <table 1> }`. Assert per pixel rather than per row.

20. **`native_sprite.load` caches by path**, so a spec reusing one fixture path
    across tests reads the first test's frames. Call `native_sprite.reset()` in
    `after_each`.

21. **`vim.json.encode` writes an empty Lua table as `{}`, not `[]`.**
    `path_params.points` is the first array-valued manifest field, so the Rust
    deserialiser explicitly accepts both. Any future array-valued field needs the
    same treatment or it parses in the terminal and fails on the overlay.

22. **`vim.tbl_deep_extend` cannot set a field to nil**, so a placement-request
    helper built with it cannot express "no floor measured". `position_spec`
    assigns `request.floor_row = nil` after building instead.

23. **`x and false or y` never yields `false` in Lua.** `false` is falsy, so the
    `or` branch always wins. This shipped in a design document's own
    sidecar-decoding snippet and would have made every transparent pixel render
    opaque. Use an explicit `if`.

24. **A hyphen in a table key is a Lua syntax error.** Generated manifests need
    `["running-right"] = { … }`. Real action names have hyphens; `walk`/`idle`
    test fixtures never caught it.

25. **The importer never resamples.** Feed it 1920×1080 stills and you get
    1920×1080 frames, a 15360×4320 sheet and a 265 MB sidecar. Downscale first.
    This is why `cat_walking` was regenerated from its existing packed atlas
    rather than from `assets/cat_walking/source/` or the source GIFs.

26. **Don't point `--manifest-out` at a hand-tuned manifest.** The scaffold
    overwrites, and its `physics`/`transitions` are placeholders. Write it
    elsewhere and diff.

---

## Accepted debt

**`engine.lua` is over 900 lines** against a 400-line cap, with `M.spawn` and
`M.step` well over the 60-line function cap. `renderer.lua` is 508 and
`external.lua` is 537. `sprite_sources.lua` is at 394 with **no room left** — the
next thing added to it must be extracted instead. `run()` in
`engine/tests/physics_parity.rs` is 74 lines after `probe_manifest` was
extracted; the step loop has no further natural seam. Owner's call, 2026-08-16:
leave them until the features are in, but **no new file may break the
standards**.

`assets/codex_pets/` is gitignored on purpose — 236 MB of third-party artwork
with no stated licence, kept on disk as local test material only. `imported/`
regenerates from `sheets/` via `tools/codex_pets/`.

---

## Open questions for the owner

1. **Should a pet that only ever walks one way be the design?** `cat` and
   `cat_walking` both declare `wrap_mode = "wrap"` for `walk`, and wrap never
   flips `heading_x` — only `bounce` does (`engine.lua:731`). The code does what
   the manifest says; whether the manifest says the right thing is a design
   question.

2. **How large may a first-draw hitch be?** A GIF is decoded once, on the first
   frame that needs it, on the main loop: ~130 ms for the 15-frame reference
   asset, ~375 ms for the 32-frame one. If that is too much, the fix is a
   coroutine seam in `sprite_sources.load_sprite` that yields between frames —
   worth building only if someone actually notices it.

3. **Should the half-block quantiser run on procedural art too?** It is gated on
   imported art today, because the built-ins are drawn from a small palette by
   construction. The §3 redo changes that arithmetic; if the quantised palette
   lands there, the gate can go and `max_sprite_colours` becomes the single
   answer for every asset.
