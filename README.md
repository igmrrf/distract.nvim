# distract.nvim 🐾✨

A high-performance, data-driven rendering engine for **Neovim** and terminal environments capable of rendering smooth, animated entities and environmental concepts with custom capabilities and state machines.

---

## 🌟 Features & Multi-Backend Architecture

`distract.nvim` offers multiple rendering backends to suit your terminal environment:

1. 🎨 **`halfblock` (In-Terminal Truecolor - Default)**:
   - Renders 24-bit RGB pixel-art sprites directly inside Neovim using Unicode half-blocks (`▀` / `▄`) and native floating windows.
   - **Zero OS window overlays**, transparent background, works in any truecolor terminal emulator (Ghostty, WezTerm, Kitty, Alacritty, iTerm2, tmux, SSH).
2. 🖥️ **`overlay` (Hardware-Accelerated GPU Window)**:
   - Transparent, borderless, click-through wgpu desktop window.
   - Draws one instanced quad per entity from a sprite atlas uploaded once, and
     skips the frame entirely when nothing has moved, so an idle overlay costs
     approximately nothing.
   - Needs a compiled engine binary — see [Overlay backend](#-overlay-backend).
   - Not available on X11: click-through is unsupported there, and a fullscreen
     always-on-top window without it would capture every mouse click on your
     desktop. The overlay refuses to start rather than trapping you.

> **Removed:** the ASCII `float` backend. Sprites are truecolor pixel art only;
> `backend = "float"` resolves to `halfblock` and emits a warning.
>
> **Not implemented:** a Kitty/Ghostty graphics-protocol backend. `backend = "kitty"`
> (and the `ghostty` / `wezterm` aliases) also resolve to `halfblock` with a warning.

---

## 🎨 Sprites

Sprites are **drawn procedurally**, not stored as pixel tables. Both backends
draw the same art: [`lua/distract/sprites/`](lua/distract/sprites) for the
terminal, [`engine/src/sprites/`](engine/src/sprites) for the overlay, from the
same pose curves and the same shading model. Each asset exposes one
`draw(pose)` routine
taking a handful of scalars — body lift, gait phase, claw opening, eclipse
progress — and each state samples those scalars along a curve. Frames are
generated once on first use and cached.

Two things follow from that:

- **Animation is smooth by construction.** A state is a curve, not a set of
  hand-drawn frames that have to line up by eye.
- **Volume comes from lighting.** [`sprite_gen.orb`](lua/distract/sprite_gen.lua)
  shades an ellipse as a lit hemisphere — Lambert diffuse from a shared key
  light, a rim term at grazing angles, and a specular highlight — so flat pixel
  art reads as a rounded, three-dimensional form.

Each module also exports a `layout` mapping state name → frame indices, which
the matching manifest references directly, so art and manifest cannot drift
apart.

| Asset | Canvas | Frames | States |
|---|---|---|---|
| cat | 24×16 px (24×8 cells) | 29 | idle, walk, walk_fast, jump, yawn, sleep |
| crab | 24×16 px (24×8 cells) | 25 | idle, walk, walk_fast, clip_claws, burrow, sleep |
| sun | 16×16 px (16×8 cells) | 25 | shining, eclipse, flare, rising, setting |

Frames are drawn on first use, not at startup: loading the plugin costs well
under a millisecond whether or not you ever spawn anything.

Adding a state means adding a pose curve:

```lua
add("pounce", g.sequence(6, function(t)
  local arc = math.sin(t * math.pi)
  return { lift = arc, stretch = 0.9 * arc, leg = 0.3, eye = 1 }
end))
```

---

## 🎭 Custom Entity Capabilities & Actions

- **Cat**: Walk, sprint on typing, jump (with parabolic gravity physics & floor collision), yawn, sleep with Zzz, sit, wake.
- **Crab**: Sideways scuttle, snap pincers / clip claws, burrow into editor code, sleep.
- **Sun**: Shining with pulsing corona, sine pathing, solar flares, sunrise/sunset, solar eclipses.

---

## 📦 Installation

Using [lazy.nvim](https://github.com/folke/lazy.nvim):

```lua
{
  "igmrrf/distract.nvim",
  config = function()
    require("distract").setup({
      backend = "halfblock",  -- "halfblock" (in-terminal Truecolor) or "overlay" (GPU window)
      idle_timeout_ms = 5000, -- Time before pets fall asleep
      debounce_ms = 50,       -- Keystroke event debounce
    })
  end,
}
```

---

## 🚀 Commands

| Command | Description | Completion / Options |
|---|---|---|
| `:DistractStart` | Start the active render engine | |
| `:DistractStop` | Stop the render engine | |
| `:DistractToggle` | Toggle the engine on / off | |
| `:DistractBackend [name]` | View or switch active rendering backend | `halfblock`, `overlay` |
| `:DistractSpawn [asset]` | Spawn an entity onto the screen | `cat`, `crab`, `sun`, or custom |
| `:DistractAction <action> [target]` | Trigger a custom capability on an entity | `jump`, `yawn`, `clip`, `burrow`, `eclipse`, `rise`, `set`, `flare`, `sleep`, `wake` |
| `:DistractClear` | Clear all active entities from the screen | |
| `:DistractStatus` | Print active entities, states, and coordinates | |
| `:DistractBuild` | Build the overlay engine binary in the background | |

---

## 🖥️ Overlay backend

The overlay runs a separate Rust process. Build it once:

```vim
:DistractBackend overlay
:DistractBuild
```

or from a shell:

```bash
cargo build --release --manifest-path engine/Cargo.toml
```

A binary unpacked to `engine/bin/distract-engine` is also picked up, which is
where a [release](https://github.com/igmrrf/distract.nvim/releases) archive
should go.

### Cell size

The overlay positions entities in screen pixels while Neovim measures in
terminal cells, and there is no portable way to ask a terminal for its cell size
from inside Neovim. Resolution order is: your config, then the terminal's answer
to `CSI 16 t` (kitty, WezTerm, Ghostty, foot, iTerm2), then a 10×20 default.

If entities do not line up — most likely on a HiDPI display, where a real cell
is closer to 16×36 — measure yours and set it:

```lua
require("distract").setup({
  backend = "overlay",
  cell_width = 16,
  cell_height = 36,
})
```

---

## 🐾 Defining Custom Assets

Assets are fully data-driven. You can define custom animations, physics, and state transitions in your Neovim configuration.

**Units.** Positions and velocities are in *sprite pixels*, and velocities are
per frame at 60 FPS. One sprite pixel is one terminal cell wide and half a cell
tall. Both backends convert from that same unit, so one manifest describes one
behaviour everywhere. See `:help distract-units`.

```lua
require("distract").setup({
  backend = "halfblock",
  assets = {
    my_pet = {
      name = "my_pet",
      asset_type = "sprite",
      initial_state = "idle",
      states = {
        idle = {
          animation = { frames = { 0, 1 }, fps = 4.0, loop_anim = true },
          physics = { target_vx = 0.0, wrap_mode = "clamp" },
          transitions = {
            on_event = { typing = "run", moving = "walk" },
            timeout_ms = 8000,
            on_timeout = "sleep",
          },
        },
        walk = {
          animation = { frames = { 2, 3 }, fps = 6.0, loop_anim = true },
          physics = { target_vx = 1.5, wrap_mode = "bounce" },
        },
        run = {
          animation = { frames = { 2, 3 }, fps = 12.0, loop_anim = true },
          physics = { target_vx = 3.5, wrap_mode = "wrap" },
          transitions = { timeout_ms = 2000, on_timeout = "walk" },
        },
        sleep = {
          animation = { frames = { 4 }, fps = 1.0, loop_anim = true },
          transitions = { on_event = { typing = "idle", moving = "idle" } },
        },
      },
      custom_actions = {
        sleep = { target_state = "sleep" },
        wake = { target_state = "idle" },
      },
    },
  },
})
```

---

## 🧪 Testing

Run the Rust engine tests — unit tests, headless GPU tests that exercise the
real shader, and the screenshot integration test:
```bash
cargo test --manifest-path engine/Cargo.toml
```

Run the Neovim Lua test suite (exits non-zero on failure):
```bash
nvim --headless -u tests/minimal_init.lua -c "luafile tests/run_tests.lua"
```

Lint and format gates, all enforced in CI:
```bash
cargo fmt --manifest-path engine/Cargo.toml -- --check
cargo clippy --manifest-path engine/Cargo.toml --all-targets -- -D warnings
stylua --check lua plugin tests
luacheck lua plugin tests
```

---

## 📖 Documentation

```vim
:help distract
```

---

## 📄 License
MIT
