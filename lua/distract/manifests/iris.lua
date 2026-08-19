local M = {
  name = "iris",
  asset_type = "sprite",
  spritesheet = {
    path = "assets/iris/iris_sheet.png",
    native_path = "assets/iris/iris_frames.rgba",
    frame_width = 192,
    frame_height = 208,
    columns = 8,
    rows = 10,
  },
  anchor = "bottom",
  initial_state = "idle",
  locomotion = "grounded",
  capabilities = { locomotion = { "grounded", "ballistic" } },
  z_index = 10,
  states = {
    idle = {
      animation = { frames = { 0, 1, 2, 3, 4, 5, 6 }, fps = 3.0, loop_anim = true, flip_x = false },
      physics = { target_vx = 0.0, target_vy = 0.0, friction = 0.1, wrap_mode = "clamp" },
      transitions = {
        on_event = { typing = "running", moving = "running", idle = "waiting" },
        timeout_ms = 6000,
        on_timeout = "waiting",
      },
    },
    running = {
      animation = {
        frames = { 46, 47, 48, 49, 50, 51 },
        fps = 10.0,
        loop_anim = true,
        flip_x = false,
      },
      physics = { target_vx = 2.0, target_vy = 0.0, wrap_mode = "wrap" },
      transitions = { on_event = { idle = "idle" } },
    },
    ["running-right"] = {
      animation = {
        frames = { 7, 8, 9, 10, 11, 12, 13, 14 },
        fps = 12.0,
        loop_anim = true,
        flip_x = false,
      },
      physics = { target_vx = 2.5, target_vy = 0.0, wrap_mode = "wrap" },
      transitions = { on_event = { idle = "idle" } },
    },
    ["running-left"] = {
      animation = {
        frames = { 15, 16, 17, 18, 19, 20, 21, 22 },
        fps = 12.0,
        loop_anim = true,
        flip_x = false,
      },
      physics = { target_vx = -2.5, target_vy = 0.0, wrap_mode = "wrap" },
      transitions = { on_event = { idle = "idle" } },
    },
    waving = {
      animation = { frames = { 23, 24, 25, 26 }, fps = 8.0, loop_anim = false, flip_x = false },
      physics = { target_vx = 0.0, target_vy = 0.0 },
      transitions = { on_finish = "idle", timeout_ms = 500, on_timeout = "idle" },
    },
    jumping = {
      animation = { frames = { 27, 28, 29, 30, 31 }, fps = 10.0, loop_anim = false, flip_x = false },
      physics = {
        target_vx = 0.0,
        target_vy = 0.0,
        jump_impulse_y = -2.2,
        gravity = 0.32,
        wrap_mode = "bounce",
        locomotion = "ballistic",
      },
      is_locked = true,
      transitions = { on_land = "idle", timeout_ms = 1200, on_timeout = "idle" },
    },
    failed = {
      animation = {
        frames = { 32, 33, 34, 35, 36, 37, 38, 39 },
        fps = 8.0,
        loop_anim = false,
        flip_x = false,
      },
      physics = { target_vx = 0.0, target_vy = 0.0 },
      transitions = { on_finish = "idle", timeout_ms = 1000, on_timeout = "idle" },
    },
    waiting = {
      animation = {
        frames = { 40, 41, 42, 43, 44, 45 },
        fps = 4.0,
        loop_anim = true,
        flip_x = false,
      },
      physics = { target_vx = 0.0, target_vy = 0.0, friction = 0.1 },
      transitions = { on_event = { moving = "running" } },
    },
    review = {
      animation = {
        frames = { 52, 53, 54, 55, 56, 57 },
        fps = 6.0,
        loop_anim = false,
        flip_x = false,
      },
      physics = { target_vx = 0.0, target_vy = 0.0 },
      transitions = { on_finish = "idle", timeout_ms = 1000, on_timeout = "idle" },
    },
  },
  custom_actions = {
    idle = { target_state = "idle" },
    run = { target_state = "running" },
    run_right = { target_state = "running-right" },
    run_left = { target_state = "running-left" },
    wave = { target_state = "waving", duration_ms = 500, return_state = "idle" },
    jump = { target_state = "jumping", duration_ms = 1200, return_state = "idle" },
    fail = { target_state = "failed", duration_ms = 1000, return_state = "idle" },
    wait = { target_state = "waiting" },
    review = { target_state = "review", duration_ms = 1000, return_state = "idle" },
  },
}

return M
