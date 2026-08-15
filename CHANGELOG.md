# Changelog

All notable changes to **distract.nvim** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- **Multi-Backend Rendering Architecture**:
  - `halfblock` (Default): High-fidelity in-terminal 24-bit Truecolor pixel-art renderer using Unicode half-blocks (`▀` / `▄`) and native Neovim floating windows with zero external OS window overlays.
  - `kitty`: In-band graphics protocol streaming supported natively by Ghostty, Kitty, and WezTerm.
  - `float`: Lightweight ASCII/Unicode floating windows for minimal or headless sessions.
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

### Changed
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
