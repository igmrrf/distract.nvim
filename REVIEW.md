# Review Status

Every S1, S2 and S3 finding from the correctness and production-readiness
review has been fixed. This file records what was wrong, what was done, and the
few things deliberately left open.

Verified after the work, and re-verified at `v0.1.0`: 298 Rust tests and 557 Lua
tests passing; `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt
--check` clean; `stylua --check` clean; `luacheck` clean over 106 files.

Counts move with every fixture. Read them from the suites rather than from here:

```bash
cargo test --manifest-path engine/Cargo.toml 2>&1 \
  | grep -oE "^test result: ok\. [0-9]+ passed" | grep -oE "[0-9]+" | paste -sd+ - | bc
nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua
```

Severity, as used below:

- **S1** — broken or unsafe for users today
- **S2** — wrong behaviour, degraded output, or a real trap
- **S3** — efficiency, structure, maintainability

---

## 1. Rust engine (`engine/`)

### S1 — Overlay window captured all mouse input on Linux/X11 — **fixed**

`platform.rs` discarded the result of `set_cursor_hittest(false)`. On X11 winit
returns `Err(NotSupported)` unconditionally, so the fullscreen always-on-top
borderless window swallowed every click on the desktop with no way out except
killing the process.

`make_click_through` now checks the result and returns a typed
`ClickThroughUnsupported` error naming the platform and pointing at the working
backend. `create_overlay_window` propagates it, so the engine refuses to start
rather than trapping the user. macOS keeps its direct
`setIgnoresMouseEvents:YES` path.

### S1 — Overlay coordinates did not match the editor grid — **fixed**

The cell size was hardcoded to 10×20 on both sides, so on any font that is not
exactly that — and never on a HiDPI display, where a real cell is nearer 16×36 —
entities were positioned against a coordinate space matching nothing on screen.

Resolution is now, in order: `cell_width`/`cell_height` from user config; the
terminal's own answer to `CSI 16 t` (kitty, WezTerm, Ghostty, foot, iTerm2);
then a documented 10×20 default. `World::set_grid` derives the sprite scale from
the measured cell width, so an overlay sprite comes out the same apparent size
as the same sprite drawn in the terminal. Documented at `:help distract-overlay`.

### S2 — Colour was gamma-wrong in the GPU path — **fixed**

The surface was sRGB while the framebuffer texture was `Rgba8Unorm`, so raw
values were written to a target that applied a linear→sRGB encode they had never
earned, and every sprite rendered washed out.

The atlas and the offscreen compositing target are both `Rgba8UnormSrgb` now, so
sampling decodes to linear, blending happens in linear, and the hardware
re-encodes on write. Covered by
`an_opaque_sprite_survives_the_srgb_round_trip_unchanged` in
`engine/tests/gpu_headless.rs`, which runs the real shader on a real GPU.

### S2 — Surface never recovered from `Outdated` — **fixed**

`Outdated` and `Timeout` fell into `_ => {}`. `Outdated` is what wgpu returns
when the surface no longer matches the window, so a monitor change or compositor
restart left the overlay permanently stale.

`Outdated` is now handled exactly like `Lost` (reconfigure), and `Timeout`
requests another frame.

### S2 — Window was sized before it existed — **fixed**

`GpuRenderer::new` took the *requested* monitor dimensions. `main.rs` now reads
`window.inner_size()` after `build()` and configures everything from what the
window manager actually granted.

### S2 — Entities could vanish without telling Neovim — **fixed**

`WrapMode::Despawn` set `is_active = false` and `update()` silently retained the
rest, so Neovim's idea of what was alive diverged from the engine's.

`World::update` now returns the ids it removed and `main.rs` emits
`IpcResponse::Despawned` for each. Regression test:
`update_reports_entities_it_despawns`.

### S2 — No bound on spritesheet or GIF size — **fixed**

A GIF was decoded in full, every frame, at source resolution; the two samples in
`assets/` are 1600×1200 and 1920×1080. `frame_w`/`frame_h` were also taken from
whichever frame decoded last.

`asset.rs` now caps frames at 1024 px per side, 512 frames, and 256 MiB decoded
in total. GIF frames are pulled lazily and checked as they arrive, so an
oversized animation is refused before it is all in memory, and every frame is
validated against frame 0's dimensions. Errors name the limit.

### S3 — Every module was compiled twice — **fixed**

`main.rs` re-declared the whole module tree instead of using the
`distract_engine` library crate, so the binary and the library each compiled
their own copy and every test ran twice (the 25/29 split in the old output).

`main.rs` is now a thin driver over `distract_engine`. The binary target reports
0 tests.

### S3 — Assets were reloaded on every spawn — **fixed**

Neovim sends the manifest on every `:DistractSpawn`, and `register_manifest` ran
the full decode-and-slice path each time, so spawning ten cats decoded the same
spritesheet ten times. The `let _ =` also discarded load errors, so a broken
path degraded to procedural art with no diagnostic.

Manifests are hashed (through `serde_json::Value`, whose object type is ordered —
hashing the direct JSON encoding would differ between two identical manifests
because `HashMap` iteration order is per-instance) and an unchanged manifest is a
no-op. `World::spawn` propagates the load error instead of swallowing it.

### S3 — Flipped frames doubled asset memory permanently — **fixed**

`flipped_frames` is gone. The GPU path mirrors by swapping the u bounds of the
atlas rectangle; the CPU compositor reads the source column in reverse.

---

## 2. Rendering efficiency

### S3 — The GPU path was a software renderer with a GPU-shaped blit — **fixed**

Per frame, unconditionally: a full-screen memset, a scalar per-pixel blend loop,
and a full-screen `write_texture` — about 2 GB/s each way at 4K to draw three
32×32 sprites.

Replaced with a sprite atlas uploaded once and one instanced draw call:

| | before (3840×2160) | after |
|---|---|---|
| per-frame upload | 33.2 MB | 32 bytes per visible entity |
| per second at 60 fps | ~2 GB/s | ~6 KB/s for three sprites |
| draw calls | 1 fullscreen blit | 1 instanced quad batch |
| idle cost | full rate | nothing |

`request_redraw` is now gated on `World::is_quiescent`, so an overlay of
sleeping cats submits no frames at all. `Compositor` remains for the screenshot
test and headless rendering, where a deterministic software reference is the
point; it gained mirroring and integer upscaling so it still matches the GPU
output.

Also fixed here, found while rewriting: the pipeline emitted premultiplied
colour into a surface declared straight-alpha, so any pixel with `a < 255`
composited at `rgb · a²`. Sprites now composite premultiplied into an offscreen
target and a resolve pass converts to whatever convention the surface asked for.
Both directions are tested on a real GPU.

### S3 — Terminal backend recomputed and rewrote more than it needed — **fixed**

`render_halfblock_frame` is cached at `(asset, frame)` — the result depends on
nothing else — and `nvim_win_set_config` is called only when position or size
actually changed. That call forces a redraw, so making it unconditionally cost a
redraw per entity per tick even for a sleeping pet. A stationary entity now
costs zero Neovim API calls; regression test asserts exactly that.

> **Correction to the second-pass review.** §6 previously recommended replacing
> the per-entity floats with a single full-screen float and compositing in Lua.
> That does not work: a Neovim float always paints the screen cells it covers and
> is not transparent per cell, so a full-screen float would blank the editor
> behind it. The per-entity windows are load-bearing. The cost that mattered was
> the unconditional API calls, and that is what was fixed.

---

## 3. Lua plugin

### S2 — `idle` never reached the in-terminal engine — **fixed**

`reset_idle_timer` called `external.send_event` directly instead of the
`dispatch_event` that routes to both backends, so the default backend never saw
an `idle` event and `idle_timeout_ms` was dead config for it. It now dispatches.

### S2 — Debounce was defeated exactly when it mattered — **fixed**

`is_throttled` was one global flag that a differing event name short-circuited.
In insert mode `TextChangedI` ("typing") and `CursorMovedI` ("moving") both fire
on every keystroke, so the name alternated every time and the throttle never
applied — every keystroke dispatched and the entity flip-flopped between
`walk_fast` and `walk`.

Throttling is now a deadline per event name.

### S3 — Timers were never closed — **fixed**

Created at module load and only ever stopped, leaking a libuv handle per
setup/teardown cycle. The idle timer is created on demand and closed on
teardown; the debounce timer is gone entirely, since a per-name deadline needs no
handle. Regression test walks the libuv handle list across ten cycles.

### S3 — First overlay start froze Neovim — **fixed**

`vim.fn.system("cargo build --release ...")` made the editor unresponsive for the
length of a cold Rust build. `M.start` now refuses and prints exactly what to
run; `:DistractBuild` builds via `jobstart` with the last lines of cargo's output
reported on failure.

### S3 — Inconsistent `clear` semantics between backends — **fixed**

`engine.clear()` called `M.stop()`, so `:DistractClear` meant "clear and stop" on
one backend and "clear" on the other. It now clears entities and leaves the
engine running, matching `ClearAll`; `tick` returns immediately while nothing is
alive.

### S3 — `set_backend` left the plugin stopped — **fixed**

It now restarts if it was running, and says plainly that entities do not carry
over between backends.

### S3 — `VimLeavePre` autocmd registered without a group — **fixed**

Registered in a cleared `Distract` augroup, so repeated `setup()` calls no longer
accumulate duplicates. Regression test asserts exactly one after four calls.

### S3 — Lua `trigger_action` did not validate `target_state` — **fixed**

A custom action missing the field set `current_state = nil` and broke the next
tick's lookup. Both a missing `target_state` and one naming a state the manifest
does not define are now reported and ignored.

---

## 4. Testing and tooling

### S2 — The window, GPU and asset-loading layers had no tests — **fixed**

`engine/tests/gpu_headless.rs` runs the real `shader.wgsl` through the real
pipelines into an offscreen target and reads the pixels back — no window, so it
runs in CI, and it skips rather than fails where no adapter exists. It covers the
sRGB round trip, the premultiply, the resolve pass in both directions, and fully
transparent output.

`asset.rs` gained tests over real files on disk: spritesheet slicing in sheet
order, oversized frames, zero dimensions, the frame and byte budgets, a corrupt
image, and a missing path. `atlas.rs` tests packing, non-overlap, pixel fidelity
against the source frame, mirroring and out-of-range wrap. `gpu.rs` tests the
entity-to-draw-call mapping without a GPU. `platform.rs` tests the click-through
error message.

### S3 — No lint or format gate — **fixed**

CI gained a `lint` job running `cargo fmt --check`, `cargo clippy --all-targets
-- -D warnings`, `luacheck` and `stylua --check`; the release job depends on it.
`.stylua.toml` and `.luacheckrc` added. `.pre-commit-config.yaml` now runs the
same gates, and its Lua test hook lost the trailing `-c "q"` that made a failing
suite exit 0.

### S3 — No `doc/distract.txt` — **fixed**

`:help distract` now works. Covers configuration, commands, both backends, the
manifest schema and its units, events, and troubleshooting.

---

## 5. Overlay sprite art

**Fixed.** `sprite_gen` and all three assets are ported to Rust
(`engine/src/sprite_gen.rs`, `engine/src/sprites/`), so both backends draw from
one design language — the same pose curves and the same hemisphere shading. The
overlay went from 4 frames per asset to 29/25/25, and the Rust default manifests
derive their frame lists from the ported layout rather than hardcoding indices,
so art and manifest cannot drift.

The two ports agree to within 2% of pixels — Lua computes in f64 and Rust in f32,
so a handful of pixels on the exact boundary of a shaded ellipse fall on
different sides. `engine/tests/parity_dump.rs` dumps the Rust frames in the Lua
format for diffing when either side changes.

---

## 6. Second-pass findings

### S2 — GPU output premultiplied into a straight-alpha surface — **fixed**

See §2 above.

### S2 — `ScaleFactorChanged` never handled — **fixed**

`main.rs` handled `CloseRequested` and `Resized` only, so a DPI change left the
surface, framebuffer and viewport at the old size — and, with `Outdated`
swallowed, with no recovery path. Both are handled now.

### S2 — The overlay cat drew the PNG, not the procedural set — **fixed**

`assets/cat_sprite.png` is 192×48 and the manifest declared `frame_width = 48,
columns = 4`, so it sliced to exactly 4 frames and all 29 layout indices
collapsed modulo 4 — idle, sleep, yawn and jump all drew the same picture. The
cat manifest declares no spritesheet now and is `asset_type = "procedural"` on
both sides.

### S3 — Release pipeline output unreachable — **fixed**

`get_binary_path` looked only in `engine/target/{release,debug}`, so the
published per-platform archives could not be installed anywhere the plugin would
find them. `binary_candidates()` now searches `engine/bin/distract-engine` first,
which is where a release archive should be unpacked; documented in the README and
the help file.

### S3 — Every `require("distract")` rasterised every sprite — **fixed**

Requiring a sprite module now only builds its pose curves and layout; drawing
happens on first use. `plugin/distract.lua` also requires the module lazily.

Measured on the same machine: **10.5 ms → 0.73 ms** at startup, with the ~4 ms
per-asset rasterisation moved to first spawn. A regression test fails if
`require("distract")` exceeds 5 ms.

### S3 — One float window per entity — **fixed, differently**

See the correction in §2.

### S3 — `WrapMode::Wrap` gated on velocity — **fixed**

`vx` lerps toward its target, so a state whose target is 0 decayed it through
zero and an entity already off-screen never wrapped back — it sat there
invisible forever, and `Wrap` never despawns. Both backends now gate on position
alone. Regression tests on both sides.

### S3 — The two backends ran different physics — **fixed**

Rust scaled by `dt * 60`; Lua by `dt * 30` for gravity and `dt * 15` for
position, with different lerp floors, in different units.

A single unit is now defined and documented: **positions and velocities are in
sprite pixels, velocities per frame at 60 FPS, and one sprite pixel is one
terminal cell wide and half a cell tall.** Both backends convert from it —
terminal to cells, overlay by its pixels-per-sprite-pixel scale. The cat's jump
was retuned once (`jump_impulse_y` −4.0 → −2.2, `gravity` 0.15 → 0.32) so the
arc is sane in the shared unit.

### S3 — Lua hardcoded sprite width; ignored two wrap modes — **fixed**

Sizes come from `sprites.get_dimensions` (24 cells for cat and crab, 16 for sun,
against a hardcoded 16). `despawn` and `none` are handled; `despawn` reports the
removal.

### S3 — `send_command` restarted a stopped engine — **fixed**

`send_command` returns `false` when nothing is running. Only `spawn` and
`trigger_action`, which are meant to bring the overlay to life, start it.

### S3 — Stale backend names in user-facing text — **fixed**

`float` is gone from the `:DistractBackend` description and the config comment.
`is_overlay()` no longer tests `"external"`, which `normalize_backend` can never
return.

### S3 — CI ran the Lua suite twice — **fixed**

`run_tests.lua` enumerates every spec file, so the `PlenaryBustedDirectory` pass
over the same directory was removed.

---

## 7. Against the stated goal

The art layer already delivered the "intelligence and personality" half; the
behaviour layer did not. Two of the three cheap wins are now in, on both
backends:

- **Per-entity desynchronisation at spawn.** Frame index, frame timer and path
  phase are seeded from a small PRNG, so entities of the same type spawned
  together are no longer a chorus line. Regression tests on both sides.
- **Cursor attention.** `EditorEvent.context` was wired end to end and carried
  `{}`; `main.rs` destructured it away. Events now carry the cursor's screen
  position, and an entity picking up a moving state turns to face it, so it looks
  like it noticed where you are working.

### Deliberately not done

**Weighted random selection among several `on_event` targets.** This was the
third suggestion in the review and is the one thing from §7 left open. It needs a
manifest schema change — `on_event` maps a name to a single state string on both
sides — which ripples through the Rust serde types, the Lua manifests and any
user manifest already written against the current shape. It was not an S1–S3
finding, so it is left for a deliberate schema revision rather than folded into a
fix pass. The repeated-stimulus behaviour is unchanged from before.

---

## Resolved in the earlier pass

For context — details in `CHANGELOG.md`.

| Area | Issue |
|---|---|
| Renderer | `nil` transparent cells truncated every sprite row; sprites rendered as fragments |
| Renderer | `nvim_open_win` raised `Invalid 'width'` every tick at 30 FPS, unguarded, hanging Neovim |
| Renderer | Extmark columns were character indices, not byte offsets; colour landed in the wrong cells |
| Renderer | `animation.frames` was ignored; states drew each other's art |
| Engine | Repeated render failures now stop the engine and report once |
| Backends | `kitty` was advertised but never implemented; it silently rendered ASCII |
| Backends | ASCII `float` backend removed at user request |
| Sprites | Replaced hand-authored matrices with a procedural generator; 4 frames per asset → 29/25/25 |
| Compositor | Out-of-range frame indices silently drew nothing; now wrap |
| CI | Lua suite exited 0 on failure and hung headless Neovim on a failed report |
| Cleanup | Removed dead `lua/distract/pets/cat.lua` |

---

## 8. Standards Compliance

Re-measured at `v0.1.0`. The size-cap violations this section used to list are
resolved; the naming and `pcall` ones are not, and are stated as what they are
rather than as a plan.

### Resolved — strict size caps

Every file under `lua/`, `plugin/` and `engine/src/` is now within the 400-line
cap, including the eight this section previously named (`ecs.rs` at 1348 lines,
`manifest.rs` at 1155, `engine.lua` at 782, `gpu.rs` at 775, and the rest). The
decomposition landed across the sprite-parity and importer refactors. Verify
rather than trust:

```bash
for f in $(find lua plugin engine/src -name '*.lua' -o -name '*.rs'); do
  n=$(wc -l < "$f"); [ "$n" -gt 400 ] && printf "%5d  %s\n" "$n" "$f"
done
```

### Open — S3, single-letter names

`GEMINI.md` permits single letters only for mathematical coordinates. Several
locals outside that carve-out remain, among them `local w = active_windows[...]`
in `renderer.lua`, `local p = phys.path_params or {}` in `kinematics.lua`, and
the `local g = require("distract.sprite_gen")` alias each sprite module opens
with. The sprite modules are the largest group and the least mechanical to
change, because `g.` prefixes most of their drawing calls.

### Open — S2, `pcall` results discarded

Seven sites drop the result of a `pcall`. Not all are defects — a best-effort
teardown that must not fail a shutdown path is a legitimate use, and
`overlay_grid.lua`'s guarded `io.stdout:write` is deliberate — but each needs
that judgement recorded rather than assumed:

- `external.lua:354` and `:362` — shutdown send and `jobstop`
- `renderer_overlay.lua:32` — extmark deletion
- `renderer_float.lua:78` and `:79` — cursor placement in a foreign window
- `highlights.lua:61` — clearing a highlight group
- `overlay_grid.lua:63` — the `CSI 16 t` probe, documented as best effort

### Open — S3, explanatory comments

The standard asks for the "why", never the "what". Density has come down a long
way, and what remains is mostly load-bearing — the parity harnesses and the
kitty describer explain constraints that are genuinely not visible in the code.
This is a judgement call per comment rather than a sweep, and is not tracked as
a count.

