# Configuration reference

Everything configurable in the plugin and in the overlay engine, in one place.

This is the reference companion to the in-editor help, which stays the canonical
source for prose and examples:

```vim
:help distract
:help distract-config
:help distract-overlay
```

To create new assets, see [`importing-assets.md`](importing-assets.md).

---

## Setup

```lua
require("distract").setup({
  backend = nil,
  fps = 30,
  idle_timeout_ms = 5000,
  debounce_ms = 50,
  cell_width = nil,
  cell_height = nil,
  max_sprite_colours = 128,
  max_highlight_groups = 4096,
  position = { anchor = "auto", ground = "screen", parallax = { per_unit = 0.0, min = 0.4, max = 1.6 } },
  assets = {},
})
```

`setup()` merges over the defaults with `vim.tbl_deep_extend("force", …)`, so
partial tables are fine. Calling it again with no `backend` keeps the backend
already resolved — to get the automatic choice back, set
`require("distract").config.backend = nil` first.

### Options

| Option | Default | Meaning |
|---|---|---|
| `backend` | `nil` | `"halfblock"`, `"kitty"` or `"overlay"`. Unset picks the best backend the terminal can actually draw. |
| `fps` | `30` | Tick rate of the animation and physics loop. |
| `idle_timeout_ms` | `5000` | How long without activity before the `idle` event fires. |
| `debounce_ms` | `50` | Activity-event debounce. |
| `cell_width` | `nil` | Overlay only. Terminal cell width in physical pixels. |
| `cell_height` | `nil` | Overlay only. Terminal cell height in physical pixels. |
| `max_sprite_colours` | `128` | Halfblock only. Imported art is quantised to this many colours, because every distinct colour pair becomes a Neovim highlight group. |
| `max_highlight_groups` | `4096` | Ceiling on live highlight groups. At the limit the least recently drawn asset's groups are cleared; the asset being drawn is never the victim. |
| `restrict_to_instance` | `true` | Hide sprites while this Neovim instance does not have focus. The simulation keeps running. `false` keeps drawing regardless, which is what a standalone desktop animation wants. |
| `position` | see below | Where entities sit and what they stand on. |
| `positioning` | see below | Which rectangle entities may move in, what they must not cover, and float stacking. |
| `assets` | built-ins | Asset name → manifest. Built-ins resolve lazily on first access. |

### `position`

| Field | Default | Accepted |
|---|---|---|
| `anchor` | `"auto"` | `"auto"`, `"bottom"`, `"top"`, `"free"`, or `{x, y, z}` |
| `ground` | `"screen"` | `"screen"` (below the statusline and cmdline) or `"text"` (the last buffer line) |
| `parallax.per_unit` | `0.0` | Scale change per unit of `z`. `0.0` disables parallax, making every factor exactly `1`. |
| `parallax.min` | `0.4` | Lower clamp on the parallax factor. |
| `parallax.max` | `1.6` | Upper clamp. |

### `positioning`

| Key | Default | Meaning |
|---|---|---|
| `scope` | `"editor"` | `"editor"` (the whole grid), `"window"` (the current window), `"buffer"` (its text area, gutter excluded) or `"absolute"` (the grid with no exclusions at all). |
| `exclude_floating` | `true` | Hide a sprite for as long as its footprint would cover a floating window. |
| `exclude_filetypes` | `{ "toggleterm", "lazy", "TelescopePrompt", "fzf", "help" }` | Windows a sprite must never cover, floating or not. Setting this **replaces** the list rather than adding to it. |
| `z_index_offset` | `40` | Neovim float stacking for sprite surfaces. LSP hovers and completion menus sit at 50 and above, so the default draws sprites underneath them. |

`scope` is what wrapping, bouncing and clamping measure against, on both
backends: the overlay is told the rectangle over IPC, because only the editor can
see where a window's text area is. It is also where a spawn with no explicit
position lands.

`z_index_offset` is not `position.z`. That one is depth and parallax; this one is
what draws over what. Two different numbers, deliberately not shared.

Parallax needs sprite scaling, so it has no meaning on `halfblock`: a half-block
cell is a fixed size. Configuring it there reports the degradation once and
honours draw order only. `z` still sets draw order on every backend.

---

## Backends

| Backend | Sprite scaling | Transparency | `z` | Native resolution |
|---|---|---|---|---|
| `halfblock` | no | per cell | draw order | no |
| `kitty` | yes | per pixel | draw order + parallax | yes |
| `overlay` | yes | per pixel | draw order + parallax | yes |

`native_resolution` is whether the backend can show a sprite at the source
artwork's own pixel resolution rather than at the character-cell grid. It is what
makes a manifest's `spritesheet.native_path` take effect; see
[`importing-assets.md`](importing-assets.md).

`:DistractBackend` with no argument prints this for the running backend.

**halfblock** works everywhere with no build step. Two sprite pixel rows are
stacked into one terminal cell.

**kitty** registers itself only after confirming the terminal answers the
graphics protocol query. Until that succeeds, `kitty`, `ghostty` and `wezterm`
resolve to `halfblock` and say so once. Its on-screen behaviour is still
unverified by a human — see [`../HANDOFF.md`](../HANDOFF.md).

**overlay** runs a separate Rust process that opens a transparent, always-on-top,
click-through window and draws on the GPU. It **refuses to start on X11**,
because click-through is unavailable there; use `halfblock` or a Wayland session.

Name aliases that resolve to a real backend: `tui`, `terminal`, `truecolor` →
`halfblock`; `external`, `gpu`, `wgpu` → `overlay`; `ghostty`, `wezterm` → `kitty`
once it registers. Removed names (`float`, `ascii`, `lua`, `window`) resolve to
`halfblock` with a one-time notice.

### Overlay cell size

The overlay positions entities in screen pixels while Neovim measures in cells,
and there is no portable way to ask the terminal for its cell size from inside
Neovim. Resolution order:

1. `cell_width` / `cell_height` from your config, if set.
2. The terminal's answer to the `CSI 16 t` query — kitty, WezTerm, Ghostty, foot
   and iTerm2 answer; most others ignore it.
3. A `10x20` default.

If entities do not line up with your editor — most likely on HiDPI, where a real
cell is closer to `16x36` — measure and set it explicitly. Sprite scale follows
`cell_width`, so overlay sprites match the same sprite drawn in the terminal.

---

## Commands

| Command | Effect |
|---|---|
| `:DistractStart` | Start the loop. |
| `:DistractStop` | Stop it. |
| `:DistractToggle` | Toggle. |
| `:DistractSpawn [asset]` | Spawn an asset (defaults to `cat`). |
| `:DistractClear` | Remove every entity. |
| `:DistractAction <name> [target]` | Trigger a custom action. |
| `:DistractBackend [name]` | Switch backend, or print the current one's capabilities. |
| `:DistractBuild` | Build the overlay engine. |
| `:DistractStatus` | Report current state. |

## Lua API

```lua
local distract = require("distract")

distract.setup(opts)
distract.start()                          distract.stop()
distract.is_running()                     distract.status()
distract.spawn(asset_name, opts)          distract.clear()
distract.action(action_name, target)
distract.get_backend()                    distract.set_backend(name)
distract.get_available_backends()         distract.get_backend_capabilities()
distract.is_overlay()                     distract.build()
distract.register_asset(name, spec)       distract.get_asset_names()
distract.get_all_actions()
```

`register_asset(name, spec)` takes `{ sprites = …, manifest = … }` and is how a
custom asset becomes drawable in the terminal without shipping a file into the
plugin.

---

## Extending it

Three registration surfaces, all documented in full at `:help distract-extending`
with working examples in [`examples/plugins/`](../examples/plugins/).

```lua
local distract = require("distract")

-- Custom art, a custom manifest, or both.
distract.register_asset("my-pet", { manifest = ..., sprites = ... })

-- Lifecycle hooks. The entity a hook receives is read-only; changes go through
-- the `world` handle its `on_init` is given, so one plugin behaves the same way
-- on all three backends.
distract.register_plugin("my-plugin", {
  on_init = function(world) end,
  on_tick = function(entity, dt) end,
  on_state_change = function(entity, from_state, to_state) end,
  on_collision = function(entity, collision) end,
  on_editor_event = function(event_name, context) end,
  on_draw = function(layers) end,
  on_teardown = function() end,
})

-- Solid ground and hazards, in terminal cells, collected on a debounced cadence.
distract.register_obstacle_provider(function(win_id, buf_id)
  return {
    { x = 10, y = 15, width = 40, height = 1, type = "solid_platform" },
    { x = 0,  y = 25, width = 80, height = 1, type = "hazard" },
  }
end)
```

| Surface | Bound by | Failure policy |
|---|---|---|
| `register_plugin` | nothing; hooks are dispatched in registration order | a hook that raises is reported once at `WARN` and its plugin is disabled for the session |
| `register_obstacle_provider` | 128 rectangles, reported once past that | a malformed rectangle is refused with a message; a provider that raises is skipped, the others still contribute |

## Manifest schema

A manifest describes one asset. Built-ins live in `lua/distract/manifests/`.

```lua
local M = {
  name = "cat_walking",
  asset_type = "sprite",              -- "sprite" or "procedural"
  spritesheet = {
    path = "assets/cat_walking/cat_walking_sheet.png",
    native_path = "assets/cat_walking/cat_walking_frames.rgba",
    frame_width = 128,
    frame_height = 72,
    columns = 8,
    rows = 4,
  },
  anchor = "bottom",                  -- auto | bottom | top | free | {x,y,z}
  initial_state = "walk",
  locomotion = "grounded",            -- grounded | ballistic | omnidirectional
  capabilities = { locomotion = { "grounded" } },
  z_index = 10,
  states = { … },
  custom_actions = { walk = { target_state = "walk" } },
}
return M
```

### `spritesheet`

| Field | Meaning |
|---|---|
| `path` | Spritesheet or GIF, relative to the plugin root (or absolute). A `.gif` here is decoded in pure Lua and drawn on every backend. |
| `native_path` | Optional `.rgba` sidecar. Used **only** by backends whose `native_resolution` is true; halfblock never sees it. |
| `frame_width` / `frame_height` | The size the sprite is drawn at, in sprite pixels, on every backend. A screen-sized animation is resampled to this. |
| `columns` / `rows` | Grid layout of `path`. |

An empty `spritesheet = {}` means the asset is drawn procedurally.

### `states`

Each state carries three groups:

```lua
walk = {
  animation = { frames = { 0, 1, 2 }, fps = 6.0, loop_anim = true, flip_x = false },
  physics = { target_vx = 1.5, target_vy = 0.0, friction = 0.1, gravity = 0.32,
              wrap_mode = "wrap", ground_y = nil, path = nil },
  transitions = {
    on_event = { typing = "walk_fast", idle = "idle" },
    timeout_ms = 6000,
    on_timeout = "sleep",
  },
},
```

| Group | Fields |
|---|---|
| `animation` | `frames` (0-based indices into the sheet), `fps`, `loop_anim`, `flip_x` |
| `physics` | `target_vx`, `target_vy`, `friction`, `gravity`, `wrap_mode` (`wrap` \| `clamp` \| `bounce`), `ground_y`, `path` |
| `transitions` | `on_event` (event name → state), `timeout_ms`, `on_timeout` |

`animation.fps` wins when set; a state that declares none is timed by the
per-frame delays stored in the source GIF, where there are any.

`gravity` is per state — a cat in `idle` does not fall because only `jump`
declares it. That is the manifest's contract, not a bug.

Frame indices must be in range for the declared grid; a test asserts this across
every built-in manifest.

---

## The overlay engine

A Rust crate at `engine/`. Edition 2021, no new dependencies beyond what is
already vendored.

### Building

```vim
:DistractBackend overlay
:DistractBuild
```

or directly:

```bash
cargo build --release --manifest-path engine/Cargo.toml
```

A binary placed at `engine/bin/distract-engine` is also picked up, which is where
a release archive should be unpacked.

### Binaries in the crate

| Binary | Purpose |
|---|---|
| `distract-engine` | The overlay renderer process. |
| `export_sprites` | Dumps procedural sprite art. |
| `import_sprite` | The asset import pipeline — see [`importing-assets.md`](importing-assets.md). |

### Engine test and lint gates

```bash
cargo test --manifest-path engine/Cargo.toml --all-targets --all-features
cargo fmt --manifest-path engine/Cargo.toml --all -- --check
cargo clippy --manifest-path engine/Cargo.toml --all-targets --all-features -- -D warnings
```

The only expected clippy output is a future-incompat notice for the transitive
`block v0.1.6` crate, which is not this codebase.

---

## Plugin test and lint gates

```bash
nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua
stylua --check lua plugin tests
luacheck lua plugin tests
```

New spec files must be added to the `SPECS` list in `tests/run_tests.lua` — it is
an explicit list, not a directory scan, so a spec that is not listed silently
never runs.

`luacheck` is currently broken on some machines by an environment mismatch
(luacheck 1.2.0 under Lua 5.5 fails with `attempt to assign to const variable`)
and fails identically on files nobody touched. Confirm against an unmodified file
before treating it as your regression.
