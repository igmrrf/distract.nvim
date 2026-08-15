# distract.nvim 🐾✨

A high-performance, data-driven rendering engine for **Neovim** and terminal environments capable of rendering smooth, animated entities and environmental concepts with custom capabilities and state machines.

---

## 🌟 Features & Multi-Backend Architecture

`distract.nvim` offers multiple rendering backends to suit your terminal environment:

1. 🎨 **`halfblock` (In-Terminal Truecolor - Default)**:
   - Renders 24-bit RGB pixel-art sprites directly inside Neovim using Unicode half-blocks (`▀` / `▄`) and native floating windows.
   - **Zero OS window overlays**, transparent background, works in any truecolor terminal emulator (Ghostty, WezTerm, Kitty, Alacritty, iTerm2, tmux, SSH).
2. 🖥️ **`overlay` (Hardware-Accelerated GPU Window)**:
   - Transparent, borderless WGPU desktop window overlay with Porter-Duff compositing.

> **Removed:** the ASCII `float` backend. Sprites are truecolor pixel art only;
> `backend = "float"` resolves to `halfblock` and emits a warning.
>
> **Not implemented:** a Kitty/Ghostty graphics-protocol backend. `backend = "kitty"`
> (and the `ghostty` / `wezterm` aliases) also resolve to `halfblock` with a warning.

---

## 🎨 Sprites

Sprites are **drawn procedurally**, not stored as pixel tables. Each asset in
[`lua/distract/sprites/`](lua/distract/sprites) exposes one `draw(pose)` routine
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

---

## 🐾 Defining Custom Assets

Assets are fully data-driven. You can define custom animations, physics, and state transitions in your Neovim configuration:

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

Run the Rust engine tests:
```bash
cargo test --manifest-path engine/Cargo.toml
```

Run the Neovim Lua test suite (exits non-zero on failure):
```bash
nvim --headless -u tests/minimal_init.lua -c "luafile tests/run_tests.lua"
```

---

## 📄 License
MIT
