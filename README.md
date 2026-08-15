# distract.nvim 🐾✨

A high-performance, data-driven rendering engine for **Neovim** and terminal environments capable of rendering smooth, animated entities and environmental concepts with custom capabilities and state machines.

---

## 🌟 Features & Multi-Backend Architecture

`distract.nvim` offers multiple rendering backends to suit your terminal environment:

1. 🎨 **`halfblock` (In-Terminal Truecolor - Default)**:
   - Renders rich 24-bit RGB pixel-art sprites directly inside Neovim using Unicode half-blocks (`▀` / `▄`) and native floating windows.
   - **Zero OS window overlays**, 100% transparent background, works in any terminal emulator (Ghostty, WezTerm, Kitty, Alacritty, iTerm2, tmux, SSH).
2. ⚡ **`kitty` (Ghostty & Kitty Graphics Protocol)**:
   - In-band GPU image streaming supported natively by Ghostty, Kitty, and WezTerm.
3. 📝 **`float` (ASCII / Minimal Unicode)**:
   - Lightweight ASCII floating windows for low-spec or headless sessions.
4. 🖥️ **`overlay` (Hardware-Accelerated GPU Window)**:
   - Transparent, borderless WGPU desktop window overlay with 60 FPS Porter-Duff compositing.

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
      backend = "halfblock",  -- "halfblock" (in-terminal Truecolor), "kitty", "float", or "overlay"
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
| `:DistractBackend [name]` | View or switch active rendering backend | `halfblock`, `kitty`, `float`, `overlay` |
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

Run the full Rust engine unit tests (29 tests):
```bash
cargo test --manifest-path engine/Cargo.toml
```

Run the consolidated Neovim Lua test suite (31 tests):
```bash
nvim --headless -u tests/minimal_init.lua -c "luafile tests/run_tests.lua"
```

---

## 📄 License
MIT
