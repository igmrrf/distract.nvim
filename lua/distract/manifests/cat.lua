-- Frame indices come from the generated sprite layout rather than being
-- written out by hand, so the manifest and the art cannot drift apart.
local layout = require("distract.terminal_sprites").get_layout("cat")

local M = {
  name = "cat",
  asset_type = "procedural",
  -- No spritesheet: the cat is drawn procedurally by `distract.sprites.cat` on
  -- the terminal backend and by `engine/src/sprites/cat.rs` on the overlay,
  -- from the same pose curves.
  --
  -- This used to point at `assets/cat_sprite.png`, which is a 4-frame sheet.
  -- The overlay loaded those 4 frames and every one of the 29 indices in
  -- `layout` collapsed onto them modulo 4, so idle, sleep, yawn and jump all
  -- drew the same picture. Set `spritesheet.path` only for genuinely custom art.
  spritesheet = {},
  initial_state = "idle",
  z_index = 10,
  states = {
    idle = {
      animation = { frames = layout.idle, fps = 2.0, loop_anim = true, flip_x = false },
      physics = { target_vx = 0.0, target_vy = 0.0, friction = 0.1, wrap_mode = "clamp" },
      transitions = {
        on_event = {
          typing = "walk_fast",
          moving = "walk",
          scrolling = "yawn",
          idle = "sleep",
        },
        timeout_ms = 6000,
        on_timeout = "sleep",
      },
    },
    walk = {
      animation = { frames = layout.walk, fps = 6.0, loop_anim = true, flip_x = false },
      physics = { target_vx = 1.5, target_vy = 0.0, wrap_mode = "wrap" },
      transitions = {
        on_event = {
          typing = "walk_fast",
          idle = "idle",
          scrolling = "yawn",
        },
      },
    },
    walk_fast = {
      animation = { frames = layout.walk_fast, fps = 12.0, loop_anim = true, flip_x = false },
      physics = { target_vx = 3.5, target_vy = 0.0, wrap_mode = "wrap" },
      transitions = {
        on_event = {
          idle = "idle",
          moving = "walk",
        },
        timeout_ms = 1500,
        on_timeout = "walk",
      },
    },
    jump = {
      animation = { frames = layout.jump, fps = 10.0, loop_anim = false, flip_x = false },
      physics = {
        target_vx = 2.0,
        target_vy = 0.0,
        jump_impulse_y = -2.2,
        gravity = 0.32,
        wrap_mode = "bounce",
      },
      is_locked = true,
      transitions = {
        timeout_ms = 1200,
        on_timeout = "idle",
      },
    },

    yawn = {
      animation = { frames = layout.yawn, fps = 3.0, loop_anim = false, flip_x = false },
      physics = { target_vx = 0.0, target_vy = 0.0 },
      transitions = {
        on_finish = "sleep",
        timeout_ms = 2000,
        on_timeout = "sleep",
      },
    },
    sleep = {
      animation = { frames = layout.sleep, fps = 1.0, loop_anim = true, flip_x = false },
      physics = { target_vx = 0.0, target_vy = 0.0, friction = 0.2 },
      transitions = {
        on_event = {
          typing = "yawn",
          moving = "idle",
        },
      },
    },
  },
  custom_actions = {
    jump = {
      target_state = "jump",
      duration_ms = 1200,
      return_state = "idle",
    },
    yawn = {
      target_state = "yawn",
      duration_ms = 2000,
      return_state = "sleep",
    },
    sleep = {
      target_state = "sleep",
    },
    wake = {
      target_state = "idle",
    },
    sit = {
      target_state = "idle",
    },
  },
}

return M
