local M = {
  name = "sun",
  asset_type = "procedural",
  spritesheet = {},
  initial_state = "shining",
  z_index = -10,
  states = {
    shining = {
      animation = { frames = { 0, 1 }, fps = 2.0, loop_anim = true, flip_x = false },
      physics = { target_vx = 0.2, target_vy = 0.0, wrap_mode = "wrap", path_type = "sine" },
      transitions = {
        on_event = {
          scrolling = "flare",
        },
      },
    },
    rising = {
      animation = { frames = { 0 }, fps = 2.0, loop_anim = true, flip_x = false },
      physics = { target_vx = 0.5, target_vy = -1.0, wrap_mode = "clamp" },
      transitions = {
        timeout_ms = 4000,
        on_timeout = "shining",
      },
    },
    setting = {
      animation = { frames = { 0 }, fps = 2.0, loop_anim = true, flip_x = false },
      physics = { target_vx = 0.5, target_vy = 1.0, wrap_mode = "clamp" },
      transitions = {
        timeout_ms = 4000,
        on_timeout = "shining",
      },
    },
    eclipse = {
      animation = { frames = { 2, 3 }, fps = 1.0, loop_anim = true, flip_x = false },
      physics = { target_vx = 0.0, target_vy = 0.0 },
      transitions = {
        timeout_ms = 8000,
        on_timeout = "shining",
      },
    },
    flare = {
      animation = { frames = { 1, 0, 1 }, fps = 6.0, loop_anim = false, flip_x = false },
      physics = { target_vx = 0.4, target_vy = 0.0 },
      transitions = {
        on_finish = "shining",
        timeout_ms = 2000,
        on_timeout = "shining",
      },
    },
  },
  custom_actions = {
    eclipse = {
      target_state = "eclipse",
      duration_ms = 8000,
      return_state = "shining",
    },
    rise = {
      target_state = "rising",
      duration_ms = 4000,
      return_state = "shining",
    },
    set = {
      target_state = "setting",
      duration_ms = 4000,
      return_state = "shining",
    },
    flare = {
      target_state = "flare",
      duration_ms = 2000,
      return_state = "shining",
    },
  },
}

return M
