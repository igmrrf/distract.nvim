# distract.nvim 🐾✨

A high-performance, data-driven graphical rendering engine for **Neovim** (and terminal environments) capable of rendering smooth, animated entities and environmental concepts with custom capabilities and state machines.

---

## 🌟 Features

- 🏎️ **Fixed 60 FPS Delta-Time Game Loop**: Battery-friendly, VSync-aligned frame rendering via Rust (`winit` + `pixels`).
- 🎨 **Data-Driven Asset Manifests**: Define custom entities (e.g. Cats, Crabs, Celestial Suns, Weather) with declarative JSON / Lua schemas.
- 🎭 **Custom Entity Capabilities & Actions**:
  - **Cat**: Walk, sprint on typing, jump (with parabolic gravity physics), yawn, sleep, sit, wake.
  - **Crab**: Sideways scuttle, snap pincers / clip claws, burrow into sand, sleep.
  - **Sun**: Shining with pulsing corona, arc pathing, solar flares, sunrise/sunset, solar eclipses.
- ⚡ **Bi-directional JSON-RPC IPC**: Instant communication between Neovim and the Rust background engine.
- 🛡️ **Autocmd Event Throttling**: Debounced and throttled editor event emission (`TextChanged`, `CursorMoved`, `WinScrolled`, `VimResized`).
- 🖼️ **Porter-Duff Alpha Compositing**: Correct multi-layer sprite transparency blending.
- 🖥️ **Cross-Platform**: Designed for macOS, Linux (X11 / Wayland), and Windows.

---

## 📦 Installation

Using [lazy.nvim](https://github.com/folke/lazy.nvim):

```lua
{
  "igmrrf/distract.nvim",
  build = "cargo build --release --manifest-path engine/Cargo.toml",
  config = function()
    require("distract").setup({
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
| `:DistractStart` | Start the background render engine | |
| `:DistractStop` | Stop the background render engine | |
| `:DistractToggle` | Toggle the engine on / off | |
| `:DistractSpawn [asset]` | Spawn an entity onto the screen | `cat`, `crab`, `sun`, or custom |
| `:DistractAction <action> [target]` | Trigger a custom capability on an entity | `jump`, `yawn`, `clip`, `eclipse`, `rise`, `set`, `flare`, `sleep`, `wake` |
| `:DistractClear` | Clear all active entities from the screen | |
| `:DistractStatus` | Print active entities, states, and coordinates | |

---

## 🐾 Defining Custom Assets

Assets are fully data-driven. You can provide your own spritesheets and define animations, physics, and state transitions in your Neovim configuration:

```lua
require("distract").setup({
  assets = {
    my_pet = {
      name = "my_pet",
      asset_type = "sprite",
      spritesheet = {
        path = vim.fn.expand("~/.config/nvim/assets/my_pet.png"),
        frame_width = 32,
        frame_height = 32,
        columns = 4,
        rows = 2,
      },
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
          physics = { target_vx = 1.5, wrap_mode = "wrap" },
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
        pet = { target_state = "idle" },
        sleep = { target_state = "sleep" },
      },
    },
  },
})
```

---

## 🧪 Testing

Run the full Rust engine unit tests (25 tests):
```bash
cargo test --manifest-path engine/Cargo.toml
```

Run the consolidated Neovim Lua test suite (30 assertions):
```bash
nvim --headless -u NONE -c "set rtp+=." -c "runtime plugin/distract.lua" -c "luafile tests/run_tests.lua" -c "q"
```

Or run any individual section/file test spec:
```bash
# Core Module (init.lua)
nvim --headless -u NONE -c "set rtp+=." -c "runtime plugin/distract.lua" -c "luafile tests/init_spec.lua" -c "q"

# External IPC & JSON-RPC (external.lua)
nvim --headless -u NONE -c "set rtp+=." -c "runtime plugin/distract.lua" -c "luafile tests/external_spec.lua" -c "q"

# Autocmd Event Emitter & Debouncing (events.lua)
nvim --headless -u NONE -c "set rtp+=." -c "runtime plugin/distract.lua" -c "luafile tests/events_spec.lua" -c "q"

# Asset Manifests (cat, crab, sun)
nvim --headless -u NONE -c "set rtp+=." -c "runtime plugin/distract.lua" -c "luafile tests/manifests_spec.lua" -c "q"

# User Commands & Completions (plugin/distract.lua)
nvim --headless -u NONE -c "set rtp+=." -c "runtime plugin/distract.lua" -c "luafile tests/plugin_commands_spec.lua" -c "q"
```

---

## 📄 License
MIT

