require("tests.test_harness")

local renderer = require("distract.renderer")
local sprites = require("distract.terminal_sprites")

--- Minimal entity stub matching what distract.engine produces.
local function entity(manifest, state, frame_idx)
  return {
    id = 1,
    asset_name = manifest.name,
    manifest = manifest,
    current_state = state,
    frame_idx = frame_idx,
    x = 0,
    y = 0,
  }
end

describe("distract.renderer frame resolution", function()
  local cat = require("distract.manifests.cat")
  local cat_count = #sprites.get_pixel_frames("cat")

  --- The 1-based pixel frame a state's Nth animation step should resolve to.
  local function expected(manifest, state, step)
    return manifest.states[state].animation.frames[step] + 1
  end

  it("maps the animation position through the manifest frame list", function()
    local e = entity(cat, "sleep", 1)
    assert.are_equal(
      expected(cat, "sleep", 1),
      renderer.resolve_pixel_frame(e, cat_count),
      "a sleeping cat must draw its own sleep art, not the idle frame"
    )
  end)

  it("walks the frame list as the animation advances", function()
    for step = 1, #cat.states.yawn.animation.frames do
      assert.are_equal(
        expected(cat, "yawn", step),
        renderer.resolve_pixel_frame(entity(cat, "yawn", step), cat_count)
      )
    end
  end)

  it("maps the walk cycle to its declared sheet frames", function()
    for step = 1, #cat.states.walk.animation.frames do
      assert.are_equal(
        expected(cat, "walk", step),
        renderer.resolve_pixel_frame(entity(cat, "walk", step), cat_count)
      )
    end
  end)

  it("resolves distinct art for every cat state", function()
    local seen = {}
    for state, _ in pairs(cat.states) do
      local idx = renderer.resolve_pixel_frame(entity(cat, state, 1), cat_count)
      assert.is_nil(
        seen[idx],
        string.format(
          "states '%s' and '%s' both open on pixel frame %d",
          tostring(seen[idx]),
          state,
          idx
        )
      )
      seen[idx] = state
    end
  end)

  it("resolves distinct art for the sun eclipse state", function()
    local sun = require("distract.manifests.sun")
    local sun_count = #sprites.get_pixel_frames("sun")
    local shining = renderer.resolve_pixel_frame(entity(sun, "shining", 1), sun_count)
    local eclipse = renderer.resolve_pixel_frame(entity(sun, "eclipse", 1), sun_count)
    assert(shining ~= eclipse, "eclipse must not resolve to the same pixel frame as shining")
  end)

  it("wraps an out of range animation position instead of erroring", function()
    local steps = #cat.states.walk.animation.frames
    assert.are_equal(
      expected(cat, "walk", 1),
      renderer.resolve_pixel_frame(entity(cat, "walk", steps + 1), cat_count),
      "stepping past the end of a cycle must wrap back to its first frame"
    )
  end)

  it("clamps a sheet index that exceeds the available pixel frames", function()
    local manifest = {
      name = "cat",
      states = { odd = { animation = { frames = { 99 } } } },
    }
    local idx = renderer.resolve_pixel_frame(entity(manifest, "odd", 1), 4)
    assert(idx >= 1 and idx <= 4, string.format("index %d out of pixel frame range", idx))
  end)

  it("falls back to the first frame for an unknown state", function()
    assert.are_equal(1, renderer.resolve_pixel_frame(entity(cat, "no_such_state", 1), 4))
  end)
end)

describe("distract.renderer facing", function()
  local cat = require("distract.manifests.cat")

  --- An entity stub with an explicit heading.
  local function facing(state, flip_x)
    local e = entity(cat, state, 1)
    e.flip_x = flip_x
    return e
  end

  it("mirrors the art when the entity is heading left", function()
    assert.is_false(renderer.resolve_flip(facing("walk", false)))
    assert.is_true(
      renderer.resolve_flip(facing("walk", true)),
      "a cat walking left must be drawn mirrored, not facing right"
    )
  end)

  it("does not mirror twice when the art is already authored facing left", function()
    local manifest = {
      name = "cat",
      states = { walk = { animation = { frames = { 0 }, flip_x = true } } },
    }
    local e = entity(manifest, "walk", 1)
    e.flip_x = true
    assert.is_false(
      renderer.resolve_flip(e),
      "entity heading and authored facing must cancel, as they do on the overlay"
    )
  end)

  it("renders a mirrored frame as the column reverse of the facing one", function()
    local frames = sprites.get_pixel_frames("cat")
    local matrix = frames[1]
    local mirrored = sprites.mirror_matrix(matrix)
    local width = #matrix[1]

    for r = 1, #matrix do
      for c = 1, width do
        assert.are.same(
          matrix[r][c],
          mirrored[r][width - c + 1],
          string.format("pixel (%d,%d) did not survive the mirror", r, c)
        )
      end
    end
  end)

  it("caches facing and mirrored art separately", function()
    local lines = sprites.get_rendered_frame("cat", 1, false)
    local flipped = sprites.get_rendered_frame("cat", 1, true)

    local differs = false
    for i = 1, #lines do
      if lines[i] ~= flipped[i] then
        differs = true
        break
      end
    end
    assert.is_true(differs, "mirrored art must not come back from the facing cache slot")
    assert.are.same(lines, sprites.get_rendered_frame("cat", 1, false))
  end)
end)

describe("distract.renderer window geometry", function()
  it("computes a positive integer window width for every built in frame", function()
    for _, name in ipairs({ "cat", "crab", "sun" }) do
      for frame_no, matrix in ipairs(sprites.get_pixel_frames(name)) do
        local _, _, w, h = sprites.render_halfblock_frame(matrix)
        assert(
          type(w) == "number" and w > 0 and w == math.floor(w),
          string.format(
            "%s frame %d width %s is not a positive integer",
            name,
            frame_no,
            tostring(w)
          )
        )
        assert(
          type(h) == "number" and h > 0 and h == math.floor(h),
          string.format(
            "%s frame %d height %s is not a positive integer",
            name,
            frame_no,
            tostring(h)
          )
        )
      end
    end
  end)
end)

describe("distract.renderer draw loop", function()
  local engine = require("distract.engine")

  local function drain_scheduled()
    -- Let vim.schedule_wrap callbacks queued by the engine timer run.
    vim.wait(150, function()
      return false
    end)
  end

  it("draws every asset in halfblock mode without raising an error", function()
    local distract = require("distract")
    distract.setup({ backend = "halfblock" })
    engine.clear()

    local errors = {}
    local orig = vim.notify
    for _, name in ipairs({ "cat", "crab", "sun" }) do
      engine.spawn(name)
    end

    local ok, err = pcall(engine.tick)
    vim.notify = orig
    assert(ok, string.format("halfblock tick raised: %s", tostring(err)))
    assert.are_equal(0, #errors)

    drain_scheduled()
    engine.clear()
  end)

  it("draws every asset in float mode without raising an error", function()
    local distract = require("distract")
    distract.setup({ backend = "float" })
    engine.clear()

    for _, name in ipairs({ "cat", "crab", "sun" }) do
      engine.spawn(name)
    end
    local ok, err = pcall(engine.tick)
    assert(ok, string.format("float tick raised: %s", tostring(err)))

    drain_scheduled()
    engine.clear()
    distract.setup({ backend = "halfblock" })
  end)
end)
