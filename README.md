# distract.nvim 🐾✨

A high-performance, data-driven rendering engine for **Neovim** and terminal environments capable of rendering smooth, animated entities and environmental concepts with custom capabilities and state machines.

---

## 🌟 Features & Multi-Backend Architecture

`distract.nvim` offers multiple rendering backends to suit your terminal environment:

The backend is chosen for you when you name none: a terminal that speaks the
graphics protocol gets `kitty`, everything else gets `halfblock`. Naming one in
`setup` or with `:DistractBackend` always wins.

1. 🎨 **`halfblock` (In-Terminal Truecolor)**:
   - Renders 24-bit RGB pixel-art sprites directly inside Neovim using Unicode half-blocks (`▀` / `▄`).
   - **Genuinely transparent.** Rows of a sprite that sit over your code are drawn
     as overlay virtual text, so only the cells with a pixel in them are touched
     and the characters around each pixel survive. Rows below the end of the
     buffer — where virtual text cannot be placed — fall back to a float whose
     `Normal` has no background of its own. See [Transparency](#-transparency).
   - **Zero OS window overlays**, works in any truecolor terminal emulator (Ghostty, WezTerm, Kitty, Alacritty, iTerm2, tmux, SSH).
2. 🐱 **`kitty` (Terminal Graphics Protocol)**:
   - Real RGBA sprites with **per-pixel alpha**, drawn by the terminal itself
     rather than approximated with half-block glyphs.
   - Occupies exactly the same cells as `halfblock` — a 24×16 sprite is 24
     columns by 8 rows either way. The fidelity is in the pixel density inside
     that rectangle, not in a bigger rectangle, so placement is unchanged.
   - Can scale a sprite, so `z` means parallax here as well as draw order.
   - Only offered when the terminal answers the protocol's `a=q` query, and only
     with `termguicolors` set. Anything else falls back to `halfblock`.
   - Confirmed against Ghostty; `ghostty` and `wezterm` are aliases for it.
3. 🖥️ **`overlay` (Hardware-Accelerated GPU Window)**:
   - Transparent, borderless, click-through wgpu desktop window.
   - Draws one instanced quad per entity from a sprite atlas uploaded once, and
     skips the frame entirely when nothing has moved, so an idle overlay costs
     approximately nothing.
   - Needs a compiled engine binary — see [Overlay backend](#-overlay-backend).
   - Not available on X11: click-through is unsupported there, and a fullscreen
     always-on-top window without it would capture every mouse click on your
     desktop. The overlay refuses to start rather than trapping you.

---

## 🧊 2D or 3D

Every asset draws two ways, on every backend:

```vim
:DistractRender 3d
:DistractRender yaw=45 slab=8
:DistractRender               " report what is in force
```

```lua
require("distract").setup({ render = { mode = "3d", yaw_degrees = 22.0 } })
require("distract").set_render("3d")   -- or change it live, no respawn needed
```

**There is no second set of 3D assets, and no mesh format.** Every asset already
resolves to RGBA frames, so a frame's opaque pixels are extruded into a slab of
cubes — a real model of that frame, built from the art the asset already has. The
built-ins, imported spritesheets, GIFs and anything you register all work in 3D
with nothing authored twice. A face is only emitted where the neighbour that would
hide it is transparent, so a solid frame costs two quads a pixel plus its
silhouette.

**Nothing about the simulation changes.** Placement, floors, obstacles, wrapping
and an asset's cell footprint are identical in both modes — a model is fitted into
the sprite's own canvas, because the footprint is what the physics measures
against. A model drawn face-on covers *exactly* the pixels its sprite does, which
the suite asserts, so turning 3D on never moves or reshapes a pet. What `z` means
is the one thing that changes: in 2D it is draw order plus the `parallax` size
damping, and in 3D the perspective projection performs the shrink instead.

| Backend | How a model is drawn |
|---|---|
| `overlay` | On the GPU, perspective camera, real depth buffer, lit per face |
| `halfblock` | Rasterised in Lua into the sprite canvas, z-buffered, orthographic |
| `kitty` | The same, transmitted through the graphics protocol |

The orthographic projection in the terminal is deliberate: a model spans about
thirty sprite pixels, so a perspective divide across it moves nothing by a whole
pixel — and an orthographic one lets a rasterised frame be cached and reused
wherever the pet is on screen. In the steady state a 3D pet costs a table lookup
per draw, exactly as a 2D one does: 200 walking cats step and draw in 4.9 ms a
frame against 4.2 ms in 2D (`tools/bench_render3d.lua`).

A manifest can pin its own mode with `render = "2d"`, which wins over the
configuration in both directions — flat overlay furniture stays flat in a 3D
session, and the two passes composite together.

Full reference: [`docs/configuration.md`](docs/configuration.md#render) and
`:help distract-render`.

---

## 🎞️ GIF assets

Point a manifest's `spritesheet.path` at a `.gif` and every backend draws it:
full pixel fidelity on `kitty` and `overlay`, half-block fidelity in the
terminal. Nothing else in the manifest changes.

```lua
require("distract").setup({
  assets = {
    walking_cat = {
      name = "walking_cat",
      spritesheet = {
        path = "assets/cat_walking_1.gif",  -- relative paths are plugin-relative
        frame_width = 32,                   -- sprite pixels; required for a
        frame_height = 24,                  -- GIF larger than the sprite
      },
      initial_state = "walk",
      states = {
        walk = {
          -- No `fps`: the GIF's own per-frame delays time the animation.
          animation = { frames = { 0, 1, 2, 3 }, loop_anim = true },
          physics = { target_vx = 1.5, wrap_mode = "wrap" },
        },
      },
    },
  },
})
```

- **Decoding is pure Lua** ([`lua/distract/gif/`](lua/distract/gif)) — GIF87a and
  GIF89a, LZW, interlacing, local palettes, the transparency index and disposal
  methods 0–3. No external process, no dependency.
- **`frame_width` / `frame_height` are the size the sprite is drawn at**, in
  sprite pixels, on every backend. A GIF authored at screen size is resampled to
  them; without them the source size is the sprite size, and a canvas over the
  budget is refused with a message naming the fields.
- **Timing:** a state's `fps` wins. A state that declares none is timed by the
  delays in the file itself.
- **Colours:** imported art is reduced to `max_sprite_colours` (default 128)
  before the half-block renderer turns colour pairs into highlight groups.
  `kitty` and `overlay` take it at full colour.

---

> **Removed:** the ASCII `float` backend. Sprites are truecolor pixel art only;
> `backend = "float"` resolves to `halfblock` and emits a warning.
>
> `backend = "kitty"` on a terminal that does not answer the graphics query also
> resolves to `halfblock`, with a warning saying why.

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

Every one of them also has a 3D form, extruded from these same frames — see
[2D or 3D](#-2d-or-3d).

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
      -- Unset picks the best backend this terminal can draw: "kitty" where the
      -- graphics protocol is available, "halfblock" otherwise. "overlay" is the
      -- GPU window.
      backend = nil,
      idle_timeout_ms = 5000,   -- Time before pets fall asleep
      debounce_ms = 50,         -- Keystroke event debounce
      max_sprite_colours = 128, -- Half-block only: palette cap for imported art
      max_highlight_groups = 4096, -- Ceiling on live sprite highlight groups
      position = {
        anchor = "auto",      -- "auto" | "bottom" | "top" | "free" | { x =, y =, z = }
        ground = "screen",    -- "screen" | "text"
        parallax = { per_unit = 0.0, min = 0.4, max = 1.6 },
      },
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
| `:DistractBackend [name]` | View or switch active rendering backend | `halfblock`, `kitty`, `overlay` |
| `:DistractSpawn [asset] [opts]` | Spawn an entity onto the screen | `cat`, `crab`, `sun`, or custom; `x=`, `y=`, `z=`, `anchor=`, `flip_x=` |
| `:DistractAction <action> [target]` | Trigger a custom capability on an entity | `jump`, `yawn`, `clip`, `burrow`, `eclipse`, `rise`, `set`, `flare`, `sleep`, `wake` |
| `:DistractClear` | Clear all active entities from the screen | |
| `:DistractStatus` | Print active entities, states, and coordinates | |
| `:DistractRender [args]` | View or change how entities are drawn | `2d`, `3d`, or `yaw=`, `fov=`, `depth=`, `slab=`, `ambient=` |
| `:DistractBuild` | Build the overlay engine binary in the background | |

---

## 📍 Placement

Entities are placed against a **floor**, and the floor is measured by Neovim —
only the editor can see `cmdheight`, the statusline and where the buffer text
ends — then pushed to whichever backend is running. Both engines are told the
same number, so a manifest lands in the same place on either one.

```lua
require("distract").spawn("cat", { anchor = "bottom", ground = "text" })
```

```vim
:DistractSpawn sun anchor=top z=-3
```

- **`anchor`** — `"auto"` asks the asset first and its physics second. An asset
  may declare an anchor of its own (the sun declares `"top"`, because a sun
  belongs in the sky); failing that, gravity binds the cat and the crab to the
  floor while anything omnidirectional drifts wherever it is put. `"bottom"`,
  `"top"` and `"free"` say so explicitly and override the asset, and
  `{ x, y, z }` is an exact position in terminal cells.
- **`ground`** — `"screen"` is the bottom of the usable screen; `"text"` is the
  row the last line of your buffer starts on, so entities walk along the end of
  the file. A wrapped or folded last line has no addressable row, and the text
  floor falls back to the screen floor rather than guessing.
- **`z`** — depth. It sets the draw order, overriding a manifest's `z_index`,
  and drives parallax: `scale = clamp(1 + z * per_unit, min, max)` damps both
  velocities and shrinks the sprite. `per_unit` is `0.0` by default, so parallax
  is off until asked for.

The half-block renderer cannot scale a sprite, so it honours draw order and
reports once that it is ignoring parallax rather than diverging in silence. The
`kitty` and `overlay` backends scale, and honour both. `:DistractBackend` prints
what the running backend can do.

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

## 👻 Transparency

A Neovim float paints *every* screen cell it covers, transparent ones included,
so a sprite-sized float blanks a sprite-sized rectangle of your code. Overlay
virtual text touches only the cells it is given, but cannot be placed where
there is no buffer line — which is exactly where a pet usually walks.

So each sprite is drawn on both surfaces at once:

| Sprite rows | Surface | Result |
|---|---|---|
| over buffer text | overlay extmarks (`virt_text_win_col`) | code around each pixel is untouched |
| past the last line | float with `Normal` = `bg=NONE` | terminal background shows through |

Nothing is redrawn unless the picture, the placement, or the editor layout under
it actually changed, so a sleeping pet costs no API calls at all.

Two cases still fall back to the float, because a buffer line cannot address
them: the continuation rows of a wrapped line, and folded lines.

---

## 🐾 Defining Custom Assets

Assets are fully data-driven. You can define custom animations, physics, and state transitions in your Neovim configuration.

**Units.** Manifest velocities are in *sprite pixels* per frame at 60 FPS. One
sprite pixel is one terminal cell wide and half a cell tall, and each backend
scales the two axes separately, so one manifest describes one behaviour
everywhere. Spawn coordinates (`x`, `y`) are in *terminal cells* on both
backends. See `:help distract-units`.

A custom asset needs art as well as a manifest. Without registered art the
terminal backend has nothing to draw and says so:

```lua
require("distract").register_asset("my_pet", {
  manifest = require("my_pet.manifest"),
  sprites = require("my_pet.sprites"), -- { frames, layout, width, height }
})
```

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

## 🔌 Writing a plugin

Three registration surfaces, and everything a companion knows about your editor
arrives through one of them. Working examples are in
[`examples/plugins/`](examples/plugins/); the full contract is
`:help distract-extending`.

```lua
local distract = require("distract")

distract.register_plugin("my-plugin", {
  on_init = function(world) end,           -- the handle everything is asked for
  on_tick = function(entity, dt) end,      -- per entity, per simulated frame
  on_state_change = function(entity, from_state, to_state) end,
  on_collision = function(entity, collision) end,  -- edges and obstacles alike
  on_editor_event = function(event_name, context) end,
  on_draw = function(layers) end,          -- where each sprite was drawn, in cells
  on_teardown = function() end,
})

distract.register_obstacle_provider(function(win_id, buf_id)
  return {
    { x = 10, y = 15, width = 40, height = 1, type = "solid_platform" },
    { x = 0,  y = 25, width = 80, height = 1, type = "hazard" },
  }
end)
```

The entity a hook receives is **read-only** — assigning to it raises. Changes go
through the `world` handle instead (`request_state`, `apply_impulse`, `despawn`,
`mark_dirty`), which queues a command applied at the top of the next step. That
is not ceremony: the in-terminal backends simulate in Lua while `overlay`
simulates in its own process, so a hook that wrote `entity.vx` would move the
sprite on two backends out of three. Queued commands are forwarded over IPC, so
one plugin produces one behaviour everywhere.

A `solid_platform` is a one-way floor — it catches an entity falling onto it and
is what a grounded pet walks along, while a jump passes up through it. A `hazard`
turns an entity around. Providers are called on a debounced cadence, never per
tick per entity, because a Tree-sitter query per frame is a performance trap.

A hook that raises is reported once and its plugin is disabled for the session.
One broken plugin cannot silence the others.

## 🧪 Testing

Run the Rust engine tests — unit tests, headless GPU tests that exercise the
real shader, and the screenshot integration test:
```bash
cargo test --manifest-path engine/Cargo.toml
```

Run the Neovim Lua test suite (exits non-zero on failure):
```bash
nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua
```

Both suites also carry cross-engine parity harnesses: `physics_parity` pins one
manifest to one trajectory on both engines, `sprite_parity` pins the generated art
pixel for pixel, and `voxel_parity` pins the 3D models vertex for vertex — exactly,
with no tolerance, because nothing in a mesh goes through a float computation whose
width matters. Regenerate their goldens only for an intentional change, and
re-measure the drift budgets when you do:
```bash
UPDATE_GOLDEN=1 cargo test --manifest-path engine/Cargo.toml --test physics_parity
UPDATE_GOLDEN=1 cargo test --manifest-path engine/Cargo.toml --test sprite_parity
UPDATE_GOLDEN=1 cargo test --manifest-path engine/Cargo.toml --test voxel_parity
```

Look at an asset's frames as text, which is how the art is judged headless:
```bash
nvim --headless --noplugin -u tests/minimal_init.lua -l tools/preview_sprite.lua cat
```

What one tick costs, at the scale a particle system would want:
```bash
cargo test --manifest-path engine/Cargo.toml --test tick_budget -- --nocapture
nvim --headless --noplugin -u tests/minimal_init.lua -l tools/bench_tick.lua 200
```

What the 3D mode costs, and what the models actually look like — the mesh pass
writes real PNGs through the real pipeline:
```bash
nvim --headless --noplugin -u tests/minimal_init.lua -l tools/bench_render3d.lua 200
cargo test --manifest-path engine/Cargo.toml --test gpu3d_headless
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
