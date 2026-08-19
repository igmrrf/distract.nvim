# Handoff — what is still open

Working notes for whoever picks this up next. Rewritten 2026-08-16, against the
commit that landed P5.

This file is **only** the pending work and the traps that cost time. It is not a
record of what shipped:

- **What was built and why** — [`CHANGELOG.md`](CHANGELOG.md).
- **What the design says** —
  [`docs/superpowers/specs/2026-08-16-locomotion-position-kitty-design.md`](docs/superpowers/specs/2026-08-16-locomotion-position-kitty-design.md),
  including the unit contract, the backend/renderer split, and every decision
  settled during implementation. Where this file and the spec disagree, the spec
  wins.
- **Which review findings are closed** — [`REVIEW.md`](REVIEW.md).
- **What might come next** — [`future.md`](future.md), which holds unbuilt work
  only, and its program plan
  [`docs/superpowers/plans/2026-08-16-future-roadmap-master.md`](docs/superpowers/plans/2026-08-16-future-roadmap-master.md).

---
mkdir -p ~/Desktop/tmp/distract_sprites && DUMP_TO=~/Desktop/tmp/distract_sprites cargo test --manifest-path engine/Cargo.toml --test parity_dump -- --ignored


## Pending

| Item | State |
|---|---|
| Kitty backend (P4) and GIF assets (P5) seen on a real screen | **unverified — needs a human** |
| Native-resolution sprites on kitty, seen on a real screen | **unverified — needs a human; same reason** |
| Step 4: silhouette-first art redo, every asset | **not started; blocked on an art-parity harness** |
| Art parity between the Lua and Rust ports | **measured, unenforced — see below** |
| `engine.lua` over the size caps | **accepted debt, see below** |
| codex-pets sheets under `assets/codex_pets/` | **staged, deliberately uncommitted — see below** |

---

## Sprite import pipeline — status 2026-08-19

The pipeline from
[`docs/superpowers/specs/2026-08-16-sprite-import-pipeline-design.md`](docs/superpowers/specs/2026-08-16-sprite-import-pipeline-design.md)
and its [atlas addendum](docs/superpowers/specs/2026-08-19-spritesheet-atlas-import-addendum.md)
is **fully implemented and green**. All 12 base-plan tasks plus the addendum's
Tasks 13 and 14 are landed on `fix/assets`.

How to use it: [`docs/importing-assets.md`](docs/importing-assets.md).
Configuration: [`docs/configuration.md`](docs/configuration.md).

**Shipped:**

- `engine/src/bin/import_sprite/` — the CLI. Three input modes (`--gif`,
  `--frames`, `--spritesheet` + `--cell`/`--row-counts`), corner flood-fill
  background removal with soft alpha, already-cutout detection, padding, grid
  packing, `.rgba` sidecar writer, Lua manifest scaffold. 31 tests.
- `lua/distract/native_sprite.lua` — the sidecar reader, `nil, err` contract,
  cached by path.
- `backends.lua` gained `native_resolution`; `sprite_sources.get_pixel_frames`
  gained an `opts` second parameter resolving the sidecar as a fourth art source
  *ahead of* its per-asset-name cache; all four call sites pass their backend's
  capability.
- `assets/cat_walking/` regenerated through the pipeline, now with a
  `native_path` sidecar.
- `tools/codex_pets/` — the test harness described below.

**Gates at the last sweep:** 177 Rust (137 lib + 31 import_sprite + 6 GPU + 2
parity + 1 screenshot), 341 Lua, `cargo fmt --check` clean, `clippy -D warnings`
clean, `stylua --check` clean. `luacheck` could not run — see the traps section.

### What is NOT done

1. **Nothing is pushed.** `fix/assets` is ahead of `origin/fix/assets`. No PR
   exists. Deciding how this integrates is the owner's call.
2. **Kitty's native-resolution output has never been looked at.** A
   characterization test proves `kitty/frames.lua` and `protocol.lua` are
   resolution-agnostic — a 24×16 sidecar goes through kitty's real call site and
   comes out with the right `pixel_w`/`cols`/`rgba` length — but no human has
   seen a native-resolution sprite on a real terminal. Same standing caveat as
   every other kitty claim in this file.
3. **No codex-pets asset is wired up as a real plugin asset.** Fifteen are
   imported under `assets/codex_pets/imported/`, each with a manifest scaffold
   whose `physics` and `transitions` are still placeholders. Turning one into a
   usable pet means hand-tuning that scaffold and registering it.
4. **`assets/codex_pets/` is staged but uncommitted, on purpose.** 236 MB — 34 MB
   of source sheets and 202 MB of derived output. Committing it puts a
   quarter-gigabyte in git history permanently. `imported/` is regenerable from
   `sheets/` with one command, so if any of it should land, it should probably be
   `sheets/` only. It is also third-party artwork with no stated licence: fine as
   local test material, not fine as shipped plugin assets.

### If you want to trim before committing

A v2 pet's 74 frames include 16 directional look poses that are not an
animation. Importing only the 9 animated rows drops it to 57 frames
(~8.9 MB instead of ~11.5 MB per sidecar):

```bash
# animated rows only: drop the trailing 8,8
--row-counts 7,8,8,4,5,8,6,6,6
```

---

## Verification sweep — 2026-08-16

Everything below was run, not assumed. Re-run before trusting it again.

**Green:**

- Four gates pass: **325 Lua**, **145 Rust** (136 lib + 6 GPU + 2 parity + 1
  screenshot, `parity_dump` ignored), `stylua --check` clean, `clippy -D warnings`
  clean. The only clippy output is a future-incompat notice for the transitive
  `block v0.1.6` crate, not this codebase.
- `cargo build --release` produces the overlay engine.
- **Gravity, live.** A cat given `jump` leaves at `vy = -2.2`, peaks at `y = 1.76`
  at t = 0.10s under `gravity = 0.32`, falls, lands on its `ground_y`, and returns
  to `idle`. Gravity is per-state and only `jump` declares it — a cat in `idle`
  does not fall, and that is the manifest's contract, not a bug.
- **14 physics fixtures** cover gravity, ballistic landing, settling to rest,
  floorless acceleration, a pushed floor, bounce, clamp, wrap, parallax damping
  and all four path types.
- **Manifest integrity:** every animation frame index is in range. cat 29 frames /
  6 states, crab 25 / 6, sun 25 / 5.
- **Real GIF assets decode.** `cat_walking_1.gif` → 15 frames in **126 ms**,
  `cat_walking_2.gif` → 32 frames in **372 ms**, both resampled to 32×24 with
  per-frame delays read from the file (100 ms / 80 ms). This confirms the
  first-draw hitch figures quoted in the open question below.
- **Backend registry** offers `halfblock` (no parallax) and `overlay` (parallax);
  an unset backend resolves to `halfblock` headless, as designed.
- Failures are explicit: an over-budget GIF names `spritesheet.frame_width` /
  `frame_height` as the fix rather than degrading silently.

**Finding — art parity drifts and nothing enforces it.** Dumping all 79 built-in
frames from both engines gives **220 mismatched cells out of 27,136 (0.81%)** —
112 in the alpha mask, 108 in colour, 44 of those differing by more than 128 on a
channel.

The mechanism is not what `parity_dump.rs`'s docstring implies. `Canvas::set`
floors its coordinates on both sides; Lua computes in f64, Rust in f32. A
coordinate landing either side of an integer boundary throws a whole drawing step
into the adjacent cell, so a *tiny* precision difference produces a *large* colour
difference. 204 of the 220 (93%) are explained by the other engine's value sitting
in an adjacent cell; the remaining 16 sit inside a smooth shading gradient.
**No transcription error was found** — the ports agree as closely as f32 and f64
allow.

Consequences for the art-parity harness: an exact-mask assertion cannot pass, and
a per-channel colour tolerance alone cannot either. Assert that the other engine's
value appears in the same cell or one of its eight neighbours, or within a channel
tolerance — and cap the total mismatch count per asset just above today's
measurement, so a transcription error's large jump fails while precision drift
does not.

**Finding — highlight headroom is thinner than it looks.** Rendering all 79
built-in frames creates **1,894** live highlight groups against a
`max_highlight_groups` cap of 4,096. Three assets consume 46% of the cap.

**Still unverifiable without a human:** the kitty backend on screen, a GIF on
screen, and the overlay window on screen. See the next section.

---

## Verify the current state

All four gates pass. Run them before and after any change.

```bash
nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua
cargo test --manifest-path engine/Cargo.toml
stylua --check lua plugin tests
cargo clippy --manifest-path engine/Cargo.toml --all-targets -- -D warnings
```

Expected: **325 Lua tests**, **145 Rust tests** (136 lib + 6 headless GPU + 2
parity + 1 screenshot; `parity_dump` is `#[ignore]`).

The Lua suite needs `-u tests/minimal_init.lua` — that is where the runtimepath
is set. Either `-l` or CI's `-c "luafile ..."` form works.

`luacheck` is listed in the README as a gate but **is broken on this machine** —
it fails to load under the installed Lua 5.5. Environment problem, not a code
problem. CI may still run it; a green local run does not mean luacheck passed.

---

## The one thing the test suite cannot tell you

**Nobody has watched a kitty placement render.** The backend is asserted byte for
byte — chunk boundaries, base64 payloads, diacritic encoding, `q=2` on every
command, `d=I` on every delete, one transmission per frame however many entities
show it. None of which is the same claim as *a cat appears on the screen*.

The three ways it can be byte-correct and still wrong, in the order worth
checking:

1. **Neovim may not emit the placeholder unchanged.** `U+10EEEE` is plane-16
   private use; if Neovim gives it a width other than 1, or normalises the
   combining marks, the cells arrive scrambled.
2. **`vim.v.stderr` may interleave** with the TUI's own output under load. The P0
   spike measured that it *reaches* the terminal, not that a 4-chunk transmission
   survives arriving mid-frame.
3. **The float may cover the placeholders.** Rows below the last buffer line go
   to a float; its `Normal` has `bg = "NONE"`, but a terminal that paints its own
   background over a graphics placement would blank the sprite there and leave
   the buffer-overlay rows visible. A cat cut off at the waist is this.

The user has Ghostty. Get a human to look at the screen: `:DistractSpawn cat` —
a Ghostty session gets the kitty backend by default, so no `:DistractBackend`
call is needed; it still reports which one is running. If nothing draws at all,
check `:set termguicolors?` — the backend declines without it and says so.

The same visit answers the GIF question: point a manifest at
`assets/cat_walking_1.gif` with `frame_width = 32, frame_height = 24` and see
whether an imported animation reads better than the procedural cat.

---

## Step 4 — art

Do **not** start by editing sprites. The same art exists twice —
`lua/distract/sprites/*.lua` and `engine/src/sprites/*.rs` — with **no automated
parity test** between them. `engine/tests/parity_dump.rs` is `#[ignore]` and
dumps *geometry*, not physics; it is a dev aid needing `DUMP_TO`, and it is
**not** covered by the physics parity harness. Build an art-parity harness first
or the two drift the moment either is touched. `future.md` §5.7 names the tool:
`validate_sprite_parity`. The tolerance it must use is measured in the
verification sweep above — read that before designing it.

Owner's answer: the redo covers **every asset, existing and future** — not the
cat alone. Three assets times two implementations is six files that can drift,
which makes the harness a precondition rather than a nicety.

The art problem itself: at 24×16 the sprite is 24 columns × **8 rows**, and
`sprite_gen.orb` spends five lighting terms (Lambert, rim, fill, specular,
dither) across a body twelve pixels wide. At that size **silhouette is the only
thing that reads** — the cat currently reads as a fox. Ears are 3-pixel stubs
(`cat.lua`, `EAR_HALF = {0,1,1}`), the four legs are identical capsules,
whiskers and muzzle are below the detail floor. Flat fills, a 1px dark contour
and 2–3 tone bands read better *and* collapse the highlight-group count.

---

## The parity harness — read before touching physics

The recurring defect class in this project is `lua/distract/engine.lua` and
`engine/src/ecs.rs` drifting apart while both claim "one manifest describes one
behaviour on both backends". Three such divergences had to be found by reading
before the harness existed; it has since caught two more on its own.

`engine/tests/physics_parity.rs` generates the goldens; `tests/physics_parity_spec.lua`
asserts the Lua engine reproduces them. Neither suite runs the other's
toolchain — they meet at the JSON in `tests/fixtures/physics/`, in **terminal
cells**.

**Any change to physics on either side means adding a fixture.** Regenerate
after an intentional behaviour change:

```bash
UPDATE_GOLDEN=1 cargo test --manifest-path engine/Cargo.toml --test physics_parity
```

Then run the Lua suite. If it disagrees, that is the point — read the reported
step index before assuming the fixture is wrong.

Two fixtures deliberately avoid knife edges and say so in their own `description`
field, so nobody "fixes" them back: `constant_velocity_wrap` uses
`target_vx = 1.3`, and `path_bezier` uses `freq = 0.47`. In both, f32 and f64
land either side of a discontinuity — a precision artefact, not a divergence.

**The harness does not cover frame timing.** `frame_duration_seconds` exists in
both `lua/distract/engine.lua` and `engine/src/ecs.rs` and must keep saying the
same thing (`animation.fps` wins, else the source file's per-frame delay, else
0.1s). It is the newest member of the divergence class with no fixture guarding
it.

---

## Traps that cost time — read before debugging

0. **Traps from the sprite import work, 2026-08-19.**
   - **`luacheck` is broken in this environment, not by your change.** luacheck
     1.2.0 under Lua 5.5 dies with `attempt to assign to const variable
     'field_name'` before it reads any project file, and fails identically on
     files nobody touched. Run it against an unmodified file to confirm before
     chasing it. `stylua --check` still works and is the real Lua gate.
   - **`x and false or y` never yields `false` in Lua.** `false` is falsy, so the
     `or` branch always wins. This shipped in the plan's own sidecar-decoding
     snippet and would have made every transparent pixel render opaque. Use an
     explicit `if`.
   - **A hyphen in a table key is a Lua syntax error.** Generated manifests need
     `["running-right"] = { … }`, not `running-right = { … }`. Real action names
     have hyphens; `walk`/`idle` test fixtures never caught it. Fixed in
     `manifest_scaffold.rs::lua_table_key`.
   - **`tests/run_tests.lua` has an explicit `SPECS` list.** A new spec file that
     is not added to it silently never runs, and the suite still reports green.
   - **The harness is not Plenary.** `tests/test_harness.lua` provides
     `assert.are.same`, `assert.are_equal`, `assert.are_not_equal`,
     `assert.is_true/is_false/is_nil/is_not_nil/is_function`. There is no
     `assert.are.equal`, no `assert.is_not.same`.
   - **`native_sprite.load` caches by path**, so a spec reusing one fixture path
     across tests reads the first test's frames. Call `native_sprite.reset()` in
     `after_each`.
   - **The importer never resamples.** Feed it 1920×1080 stills and you get
     1920×1080 frames, a 15360×4320 sheet and a 265 MB sidecar. Downscale first.
     This is why `cat_walking` was regenerated from its existing packed atlas
     rather than from `assets/cat_walking/source/` or the source GIFs.
   - **Don't point `--manifest-out` at a hand-tuned manifest.** The scaffold
     overwrites, and its `physics`/`transitions` are placeholders. Write it
     elsewhere and diff.

1. **`vim.fn.screenstring` lies inside `nvim -l` scripts.** It reads the current
   window's grid, not the composited screen, so floating windows appear at the
   wrong place or not at all. A **vanilla** float at `row=12, col=10` reproduces
   the artifact while `nvim_win_get_position` correctly reports `{12, 10}`.
   Attaching a real UI via a pty does not fix it.
   - Assert on `nvim_win_get_position` / `nvim_win_get_config` for float rows.
   - `screenstring` **is** trustworthy for the extmark overlay path, because
     those are written into the current window's own buffer.

2. **`engine.setup` merges with `vim.tbl_deep_extend("force", ...)`.**
   Registering two test manifests under the same asset name lets the first one's
   `physics` fields survive into the second. Every spec that builds probe
   manifests gives each test its own `probe_N` name for this reason.

3. **Wall-clock `dt` in `engine.tick()`.** A tight loop of 20 ticks advances
   almost no simulated time. Use `engine.step(dt, bounds)` for anything that
   asserts on distance; `tick` is only for testing the timer path.

4. **Test probes inherit the cat's manifest.** Both parity runners and several
   spec helpers start from `AssetManifest::default_cat()`, which declares
   `capabilities` and `locomotion = "grounded"` — so a probe that orbits is
   *correctly* refused. Clear both on the probe:
   `manifest.locomotion = None; manifest.capabilities = Default::default();`.

5. **`vim.json.encode` writes an empty Lua table as `{}`, not `[]`.**
   `path_params.points` is the first array-valued manifest field, so the Rust
   deserialiser explicitly accepts both. Any future array-valued field needs the
   same treatment or it parses in the terminal and fails on the overlay.

6. **Highlight groups are the unbounded shape to watch.** 1,894 live groups exist
   for the three built-in assets alone (46% of the 4,096 cap); kitty adds one per transmitted
   image. `highlights.lua` caps live groups, but step 4's quantised palette is
   what actually shrinks the number.

7. **Neither engine measures its own floor.** `events.sync_floor` measures once
   and pushes the same number to `engine.set_ground_row` and
   `external.set_ground_row`. A change that has an engine call `position.floor_row`
   for itself reintroduces the divergence class the harness exists to catch. The
   one exception is a spawn naming its own `ground`.

8. **`engine.lua` holds `floor_row` as module state**, so a spec that spawns
   after another spec's push inherits its floor. `tests/physics_parity_spec.lua`
   calls `set_ground_row(nil)` before every fixture. Any new spec asserting on
   `ground_y` must do the same.

9. **`backends`, `position` and `distract.kitty` warn once, process-wide, and
   the registries are process-wide too.** `reset_warnings()`, `backends.reset()`
   and `kitty.reset()` exist for tests. A spec that registers kitty and does not
   put it back breaks `backends_spec`, which asserts the exact backend list; a
   spec that counts warnings without resetting counts zero, passes, and proves
   nothing. `kitty.reset()` also unregisters the renderer surface — leaving it
   registered is the on-paper-only backend the two registries are kept in step to
   prevent.

10. **`vim.tbl_deep_extend` cannot set a field to nil**, so a placement-request
    helper built with it cannot express "no floor measured". `position_spec`
    assigns `request.floor_row = nil` after building instead.

11. **The kitty test seam is `writer.set_writer`, and every spec that uses it
    must put it back.** A leaked capture silently swallows every subsequent
    escape and nothing fails — the assertions are all on what was captured. Use
    `captured()` / `with_kitty()` in `tests/kitty_spec.lua` rather than calling
    `set_writer` directly.

12. **`detect.is_available()` answers once and caches.** Headless it is always
    false, because there is no UI to answer the query. `detect.override(true)`
    gets a test past that; `detect.reset()` puts it back.

13. **A spawn randomises `frame_idx`, `frame_timer` and `path_phase`** so two
    entities spawned together do not move in lockstep. Any test asserting on
    elapsed animation time has to zero the first two afterwards;
    `tests/gif_assets_spec.lua` has `spawn_at_first_frame()` for it.

14. **The harness compares with `vim.inspect`, which prints table identity.**
    `assert.are.same({ RED, RED }, row)` fails against a row of two equal but
    distinct tables, because the expected side prints `{ <1>{...}, <table 1> }`.
    Assert per pixel rather than per row.

15. **`config.backend` is `nil` by default**, and nil means "pick the best this
    terminal can draw". A spec that wants the default path back has to assign
    `distract.config.backend = nil` before calling `setup()`, because a previous
    `setup()` resolved it to a concrete name and that is what "the user chose it"
    looks like from the inside.

---

## Accepted debt

**`engine.lua` is over 900 lines** against a 400-line cap, with `M.spawn` and
`M.step` well over the 60-line function cap. `renderer.lua` is 501. Owner's call,
2026-08-16: leave them until the features are in, but **no new file may break the
standards**.

---

## Open questions for the owner

1. **How large may a first-draw hitch be?** A GIF is decoded once, on the first
   frame that needs it, on the main loop: ~130ms for the 15-frame reference
   asset, ~375ms for the 32-frame one. If that is too much, the fix is a
   coroutine seam in `sprite_sources.load_sprite` that yields between frames —
   worth building only if someone actually notices it.

2. **Should the half-block quantiser run on procedural art too?** It is gated on
   imported art today, because the built-ins are drawn from a small palette by
   construction. Step 4's redo changes that arithmetic; if the quantised palette
   lands there, the gate can go and `max_sprite_colours` becomes the single
   answer for every asset.
