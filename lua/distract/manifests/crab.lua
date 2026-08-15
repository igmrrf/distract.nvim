local M = {
  name = "crab",
  asset_type = "procedural",
  spritesheet = {},
  initial_state = "idle",
  z_index = 10,
  states = {
    idle = {
      animation = { frames = { 0 }, fps = 2.0, loop_anim = true, flip_x = false },
      physics = { target_vx = 0.0, target_vy = 0.0, wrap_mode = "clamp" },
      transitions = {
        on_event = {
          typing = "walk_fast",
          moving = "walk",
          scrolling = "clip_claws",
          idle = "sleep",
        },
        timeout_ms = 8000,
        on_timeout = "clip_claws",
      },
    },
    walk = {
      animation = { frames = { 0, 1 }, fps = 5.0, loop_anim = true, flip_x = false },
      physics = { target_vx = 1.2, target_vy = 0.0, wrap_mode = "bounce" },
      transitions = {
        on_event = {
          typing = "walk_fast",
          idle = "idle",
        },
      },
    },
    walk_fast = {
      animation = { frames = { 0, 1 }, fps = 10.0, loop_anim = true, flip_x = false },
      physics = { target_vx = 2.8, target_vy = 0.0, wrap_mode = "bounce" },
      transitions = {
        timeout_ms = 2000,
        on_timeout = "walk",
      },
    },
    clip_claws = {
      animation = { frames = { 2, 3, 2, 3 }, fps = 6.0, loop_anim = false, flip_x = false },
      physics = { target_vx = 0.0, target_vy = 0.0 },
      transitions = {
        on_finish = "idle",
        timeout_ms = 1500,
        on_timeout = "idle",
      },
    },
    burrow = {
      animation = { frames = { 0, 3 }, fps = 4.0, loop_anim = false, flip_x = false },
      physics = { target_vx = 0.0, target_vy = 0.5 },
      transitions = {
        timeout_ms = 3000,
        on_timeout = "sleep",
      },
    },
    sleep = {
      animation = { frames = { 3 }, fps = 1.0, loop_anim = true, flip_x = false },
      physics = { target_vx = 0.0, target_vy = 0.0 },
      transitions = {
        on_event = {
          typing = "clip_claws",
          moving = "idle",
        },
      },
    },
  },
  custom_actions = {
    clip = {
      target_state = "clip_claws",
      duration_ms = 1500,
      return_state = "idle",
    },
    burrow = {
      target_state = "burrow",
      duration_ms = 3000,
      return_state = "sleep",
    },
    sleep = {
      target_state = "sleep",
    },
    wake = {
      target_state = "idle",
    },
    walk = {
      target_state = "walk",
    },
  },
}

return M
