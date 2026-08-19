local M = {
  name = "cat_walking",
  asset_type = "sprite",
  spritesheet = {
    path = "assets/cat_walking/cat_walking_sheet.png",
    native_path = "assets/cat_walking/cat_walking_frames.rgba",
    frame_width = 128,
    frame_height = 72,
    columns = 8,
    rows = 4,
  },
  initial_state = "walk",
  locomotion = "grounded",
  capabilities = { locomotion = { "grounded" } },
  z_index = 10,
  states = {
    walk = {
      animation = {
        frames = {
          0,
          1,
          2,
          3,
          4,
          5,
          6,
          7,
          8,
          9,
          10,
          11,
          12,
          13,
          14,
          15,
          16,
          17,
          18,
          19,
          20,
          21,
          22,
          23,
          24,
          25,
          26,
          27,
          28,
          29,
          30,
          31,
        },
        fps = 12.0,
        loop_anim = true,
        flip_x = false,
      },
      physics = { target_vx = 2.0, target_vy = 0.0, wrap_mode = "wrap" },
      transitions = {
        on_event = {
          idle = "idle",
        },
      },
    },
    idle = {
      animation = { frames = { 0 }, fps = 1.0, loop_anim = true, flip_x = false },
      physics = { target_vx = 0.0, target_vy = 0.0, friction = 0.1, wrap_mode = "clamp" },
      transitions = {
        on_event = {
          moving = "walk",
        },
        timeout_ms = 6000,
        on_timeout = "walk",
      },
    },
  },
  custom_actions = {
    walk = {
      target_state = "walk",
    },
    idle = {
      target_state = "idle",
    },
  },
}

return M
