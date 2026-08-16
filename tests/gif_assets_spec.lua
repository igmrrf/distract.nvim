require("tests.test_harness")

local builder = require("tests.gif_builder")
local engine = require("distract.engine")
local kitty_frames = require("distract.kitty.frames")
local sprites = require("distract.terminal_sprites")

local RED = { 255, 0, 0 }
local GREEN = { 0, 255, 0 }
local PALETTE = { RED, GREEN }

--- A two-frame 2x2 GIF: the first frame all red, the second all green.
local function two_frame_bytes(delay_cs)
  return builder.header({ width = 2, height = 2, palette = PALETTE })
    .. builder.graphic_control({ delay_cs = delay_cs, disposal = 1 })
    .. builder.image({ width = 2, height = 2, indices = { 0, 0, 0, 0 } })
    .. builder.graphic_control({ delay_cs = delay_cs, disposal = 1 })
    .. builder.image({ width = 2, height = 2, indices = { 1, 1, 1, 1 } })
    .. builder.TRAILER
end

local written = {}

local function write_gif(bytes)
  local path = vim.fn.tempname() .. ".gif"
  local handle = assert(io.open(path, "wb"))
  handle:write(bytes)
  handle:close()
  written[#written + 1] = path
  return path
end

local function gif_manifest(path, size)
  return {
    name = "probe_gif",
    spritesheet = {
      path = path,
      frame_width = size and size.width or nil,
      frame_height = size and size.height or nil,
    },
    states = {},
  }
end

--- Counts vim.notify calls at or above WARN while `fn` runs.
local function warnings_during(fn)
  local original = vim.notify
  local count = 0
  vim.notify = function(_, level)
    if (level or vim.log.levels.INFO) >= vim.log.levels.WARN then
      count = count + 1
    end
  end
  local ok, err = pcall(fn)
  vim.notify = original
  if not ok then
    error(err)
  end
  return count
end

describe("GIF-backed terminal art", function()
  after_each(function()
    sprites.unbind_manifest("probe_gif")
    sprites.unbind_manifest("probe_png")
    kitty_frames.reset()
  end)

  it("draws the frames a manifest's GIF declares", function()
    local path = write_gif(two_frame_bytes(10))
    sprites.bind_manifest("probe_gif", gif_manifest(path, { width = 2, height = 2 }))

    local frames = sprites.get_pixel_frames("probe_gif")
    assert.are_equal(2, #frames)
    assert.are.same(RED, frames[1][1][1])
    assert.are.same(GREEN, frames[2][2][2])
  end)

  it("reports the declared frame size as the sprite's dimensions", function()
    local path = write_gif(two_frame_bytes(10))
    sprites.bind_manifest("probe_gif", gif_manifest(path, { width = 4, height = 6 }))

    local width, height = sprites.get_dimensions("probe_gif")
    assert.are_equal(4, width)
    assert.are_equal(6, height)
  end)

  it("carries the file's own frame delays", function()
    local path = write_gif(two_frame_bytes(9))
    sprites.bind_manifest("probe_gif", gif_manifest(path, { width = 2, height = 2 }))

    assert.are_equal(90, sprites.frame_delay_ms("probe_gif", 1))
    assert.are_equal(90, sprites.frame_delay_ms("probe_gif", 2))
  end)

  it("has no source timing for procedurally drawn art", function()
    assert.is_nil(sprites.frame_delay_ms("cat", 1))
  end)

  it("re-decodes when the manifest points somewhere else", function()
    local first = write_gif(two_frame_bytes(10))
    sprites.bind_manifest("probe_gif", gif_manifest(first, { width = 2, height = 2 }))
    assert.are.same(RED, sprites.get_pixel_frames("probe_gif")[1][1][1])

    local swapped = write_gif(
      builder.header({ width = 2, height = 2, palette = PALETTE })
        .. builder.image({ width = 2, height = 2, indices = { 1, 1, 1, 1 } })
        .. builder.TRAILER
    )
    sprites.bind_manifest("probe_gif", gif_manifest(swapped, { width = 2, height = 2 }))

    local frames = sprites.get_pixel_frames("probe_gif")
    assert.are_equal(1, #frames)
    assert.are.same(GREEN, frames[1][1][1])
  end)

  it("reports an unreadable GIF once and keeps drawing something", function()
    sprites.bind_manifest(
      "probe_gif",
      gif_manifest("/definitely/not/here.gif", { width = 2, height = 2 })
    )

    local warnings = warnings_during(function()
      sprites.get_pixel_frames("probe_gif")
      sprites.get_pixel_frames("probe_gif")
    end)

    assert.are_equal(1, warnings)
    assert.is_not_nil(sprites.get_pixel_frames("probe_gif")[1])
  end)

  it("leaves a spritesheet the terminal cannot decode to the overlay", function()
    sprites.bind_manifest("probe_png", {
      name = "probe_png",
      spritesheet = { path = "assets/cat_sprite.png" },
      states = {},
    })

    assert.is_false(sprites.has_sprite("probe_png"))
  end)

  it("quantises imported art before it becomes highlight groups", function()
    -- Sixteen distinct greys, one row, drawn as half-blocks against a cap of 4.
    local palette, indices = {}, {}
    for index = 1, 16 do
      local level = (index - 1) * 17
      palette[index] = { level, level, level }
      indices[index] = index - 1
    end

    local path = write_gif(
      builder.header({ width = 16, height = 1, palette = palette })
        .. builder.image({ width = 16, height = 1, indices = indices, min_code_size = 4 })
        .. builder.TRAILER
    )
    sprites.configure({ max_sprite_colours = 4 })
    sprites.bind_manifest("probe_gif", gif_manifest(path, { width = 16, height = 1 }))

    local _, spans = sprites.get_rendered_frame("probe_gif", 1, false)
    local distinct = {}
    for _, span in ipairs(spans) do
      distinct[span.hl] = true
    end

    assert.are_equal(4, vim.tbl_count(distinct))
    sprites.configure({ max_sprite_colours = 128 })
  end)

  it("reaches the kitty backend through the same frames", function()
    local path = write_gif(two_frame_bytes(10))
    sprites.bind_manifest("probe_gif", gif_manifest(path, { width = 2, height = 2 }))

    local frame = kitty_frames.describe("probe_gif", 2, false)
    assert.is_not_nil(frame)
    assert.are_equal(2, frame.cols)
    assert.are_equal(1, frame.rows)
    assert.are_equal(2 * 2 * 4, #frame.rgba)
  end)
end)

describe("GIF frame timing", function()
  after_each(function()
    sprites.unbind_manifest("probe_timed")
    engine.clear()
    engine.stop()
  end)

  --- A spawn desynchronises new entities by randomising the frame index and
  --- timer, so an assertion about elapsed time has to start them from zero.
  local function spawn_at_first_frame()
    engine.spawn("probe_timed", {})
    local entity = engine.get_entities()[1]
    entity.frame_idx = 1
    entity.frame_timer = 0
    return entity
  end

  --- A manifest with one state whose animation deliberately omits `fps`.
  local function timed_manifest(path)
    return {
      name = "probe_timed",
      spritesheet = { path = path, frame_width = 2, frame_height = 2 },
      initial_state = "idle",
      states = {
        idle = {
          animation = { frames = { 0, 1 }, loop_anim = true },
          physics = { target_vx = 0.0, target_vy = 0.0 },
          transitions = {},
        },
      },
    }
  end

  it("advances on the file's delays when the state declares no fps", function()
    local path = write_gif(two_frame_bytes(20))
    engine.setup({ assets = { probe_timed = timed_manifest(path) } })
    local entity = spawn_at_first_frame()

    engine.step(0.15, { columns = 80, lines = 24 })
    assert.are_equal(1, entity.frame_idx, "0.15s is short of the file's 0.2s delay")

    engine.step(0.1, { columns = 80, lines = 24 })
    assert.are_equal(2, entity.frame_idx)
  end)

  it("lets an explicit fps override the file's delays", function()
    local path = write_gif(two_frame_bytes(50))
    local manifest = timed_manifest(path)
    manifest.states.idle.animation.fps = 20.0
    engine.setup({ assets = { probe_timed = manifest } })
    local entity = spawn_at_first_frame()

    engine.step(0.06, { columns = 80, lines = 24 })
    assert.are_equal(2, entity.frame_idx, "20 fps is one frame every 0.05s")
  end)
end)
