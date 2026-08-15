# Changelog

All notable changes to **distract.nvim** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- Initial preparation for automatic release deployment workflows.
- Contributor documentation and open source community standards.

---

## [0.1.0] - 2026-08-15

### Added
- **Core Engine & Architecture**:
  - Rust background rendering engine with fixed 60 FPS delta-time tick loop via `winit` and `pixels`.
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
  - 30 modular Neovim Lua integration tests for plugin lifecycle, events, IPC, and manifests.
- **CI / CD Automation**:
  - GitHub Actions workflow for cross-platform Rust builds (Linux, macOS, Windows) and Neovim Lua tests.
  - Automated tag-based GitHub Releases packaging cross-platform binaries with SHA-256 checksums.
