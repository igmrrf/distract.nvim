-- Frame indices come from the generated sprite layout rather than being
-- written out by hand, so the manifest and the art cannot drift apart.
local layout = require("distract.terminal_sprites").get_layout("sun")

local M = {
  name = "sun",
  asset_type = "procedural",
  spritesheet = {},
  initial_state = "shining",
  locomotion = "omnidirectional",
  capabilities = { locomotion = { "omnidirectional" } },
  -- A sun belongs in the sky, and that is a fact about suns rather than about
  -- anyone's configuration. `omnidirectional` only says it is free to leave the
  -- floor, which on its own put it in the middle of the screen. A spawn or a
  -- `position.anchor` still overrides this.
  anchor = "top",
  z_index = -10,
  states = {
    shining = {
      animation = { frames = layout.shining, fps = 2.0, loop_anim = true, flip_x = false },
      physics = { target_vx = 0.2, target_vy = 0.0, wrap_mode = "wrap", path_type = "sine" },
      transitions = {
        on_event = {
          scrolling = "flare",
        },
      },
    },
    rising = {
      -- One-shot transition: it plays through and holds, then times out to
      -- shining. Looping it would snap the sun back to its start pose.
      animation = { frames = layout.rising, fps = 2.0, loop_anim = false, flip_x = false },
      physics = { target_vx = 0.5, target_vy = -1.0, wrap_mode = "clamp" },
      transitions = {
        timeout_ms = 4000,
        on_timeout = "shining",
      },
    },
    setting = {
      -- One-shot transition: it plays through and holds, then times out to
      -- shining. Looping it would snap the sun back to its start pose.
      animation = { frames = layout.setting, fps = 2.0, loop_anim = false, flip_x = false },
      physics = { target_vx = 0.5, target_vy = 1.0, wrap_mode = "clamp" },
      transitions = {
        timeout_ms = 4000,
        on_timeout = "shining",
      },
    },
    eclipse = {
      -- One-shot transition: it plays through and holds, then times out to
      -- shining. Looping it would snap the sun back to its start pose.
      animation = { frames = layout.eclipse, fps = 1.0, loop_anim = false, flip_x = false },
      physics = { target_vx = 0.0, target_vy = 0.0 },
      transitions = {
        timeout_ms = 8000,
        on_timeout = "shining",
      },
    },
    flare = {
      animation = { frames = layout.flare, fps = 6.0, loop_anim = false, flip_x = false },
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
