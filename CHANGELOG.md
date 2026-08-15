# Changelog

All notable changes to **distract.nvim** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
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

## [0.2.0] - earlier review pass

### Added
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

### Fixed
- **Half-block renderer was non-functional.** Transparent pixel-matrix cells used `nil`, which truncated every sprite row at the first hole, so `#lines[1]` was `0` and `nvim_open_win` raised `Invalid 'width'` on every tick at 30 FPS with no error handling.
- Extmark highlight columns were character indices rather than byte offsets; each half-block glyph is 3 bytes, so all colour landed in the first few cells and `end_col` split codepoints.
- `animation.frames` was ignored by the in-terminal renderer, which indexed art by animation position instead of sheet index — a sleeping cat drew its idle art and the sun's eclipse drew its shining art.
- The Lua test runner exited 0 on failure (a trailing `-c "q"` in CI masked the error) and hung headless Neovim when the report raised. It now exits non-zero via `:cquit`.
- Repeated render failures now stop the engine and report once instead of looping at the frame rate.
- The overlay compositor silently skipped drawing when a manifest frame index exceeded the loaded sheet, rendering the entity invisible; it now wraps.

### Changed
- **Sprites are generated procedurally** by `distract.sprite_gen` and `distract.sprites.*` instead of being stored as hand-authored pixel matrices. States are pose curves, so animation is smooth by construction; volume comes from a lit-hemisphere shading model with rim and specular terms. Frame counts went from 4 per asset to 29 (cat), 25 (crab), 25 (sun), giving every state its own art.
- Manifests reference the generated `layout` table rather than hand-written frame indices, so the two cannot drift apart.
- Sun `rising`, `setting` and `eclipse` are one-shot transitions rather than looping animations, which previously snapped back to the start pose.

### Removed
- The ASCII `float` backend and all text-art sprite data. `float`/`ascii` resolve to `halfblock` with a warning.
- The `kitty` backend is no longer advertised — the graphics protocol was never implemented, and selecting it silently rendered plain ASCII. The alias resolves to `halfblock` with a warning.
- Dead `lua/distract/pets/cat.lua`, which nothing referenced.
- Modernized Rust crate dependencies (`wgpu 0.16`, `bytemuck 1.25`, `pollster 0.3`, `serde 1.0.229`, `image 0.24.9`).
- Switched default rendering backend to `halfblock` for seamless, out-of-the-box compatibility with Ghostty, tmux, SSH, and all terminal environments.
- Optimized process teardown during `VimLeavePre` with synchronous `jobwait` to prevent orphaned background processes.

---

## [0.1.0] - 2026-08-15

### Added
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
