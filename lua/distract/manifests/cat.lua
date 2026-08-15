local M = {
  name = "cat",
  asset_type = "sprite",
  spritesheet = {
    path = "assets/cat_sprite.png",
    frame_width = 48,
    frame_height = 48,
    columns = 4,
    rows = 1,
  },
  initial_state = "idle",
  z_index = 10,
  states = {
    idle = {
      animation = { frames = { 0 }, fps = 2.0, loop_anim = true, flip_x = false },
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
      animation = { frames = { 1, 2 }, fps = 6.0, loop_anim = true, flip_x = false },
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
      animation = { frames = { 1, 2 }, fps = 12.0, loop_anim = true, flip_x = false },
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
      animation = { frames = { 1 }, fps = 10.0, loop_anim = false, flip_x = false },
      physics = { target_vx = 2.0, target_vy = 0.0, jump_impulse_y = -4.0, gravity = 0.15, wrap_mode = "bounce" },
      is_locked = true,
      transitions = {
        timeout_ms = 1200,
        on_timeout = "idle",
      },
    },

    yawn = {
      animation = { frames = { 0, 3, 0 }, fps = 3.0, loop_anim = false, flip_x = false },
      physics = { target_vx = 0.0, target_vy = 0.0 },
      transitions = {
        on_finish = "sleep",
        timeout_ms = 2000,
        on_timeout = "sleep",
      },
    },
    sleep = {
      animation = { frames = { 3 }, fps = 1.0, loop_anim = true, flip_x = false },
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
