require("tests.test_harness")

local sprites = require("distract.terminal_sprites")

local ASSETS = {
  cat = { manifest = require("distract.manifests.cat"), w = 24, h = 16 },
  crab = { manifest = require("distract.manifests.crab"), w = 24, h = 16 },
  sun = { manifest = require("distract.manifests.sun"), w = 16, h = 16 },
}

local function frame_key(matrix)
  local parts = {}
  for _, row in ipairs(matrix) do
    for _, px in ipairs(row) do
      parts[#parts + 1] = px and (px[1] .. "," .. px[2] .. "," .. px[3]) or "-"
    end
  end
  return table.concat(parts, ";")
end

--- Perceptual distance between two frames, 0..1.
---
--- Counting *how many* cells differ is the wrong measure for shaded sprites: a
--- one-unit RGB shift across a gradient would score the same as the whole
--- silhouette moving. This averages how far each cell actually travelled, so a
--- smooth re-shade scores near zero and a real jump cut scores high. A cell that
--- appears or disappears counts as a full unit of change.
local function delta(a, b)
  local sum, total = 0, 0
  for y = 1, #a do
    for x = 1, #a[y] do
      total = total + 1
      local pa, pb = a[y][x], b[y][x]
      if pa and pb then
        sum = sum
          + (math.abs(pa[1] - pb[1]) + math.abs(pa[2] - pb[2]) + math.abs(pa[3] - pb[3]))
            / (3 * 255)
      elseif pa or pb then
        sum = sum + 1
      end
    end
  end
  return total > 0 and (sum / total) or 0
end

--- Whether two frames differ at all, at pixel identity.
local function differs(a, b)
  for y = 1, #a do
    for x = 1, #a[y] do
      local pa, pb = a[y][x], b[y][x]
      if (pa and not pb) or (pb and not pa) then
        return true
      end
      if pa and pb and (pa[1] ~= pb[1] or pa[2] ~= pb[2] or pa[3] ~= pb[3]) then
        return true
      end
    end
  end
  return false
end

local function distinct_colors(matrix)
  local seen, n = {}, 0
  for _, row in ipairs(matrix) do
    for _, px in ipairs(row) do
      if px then
        local k = px[1] .. "," .. px[2] .. "," .. px[3]
        if not seen[k] then
          seen[k] = true
          n = n + 1
        end
      end
    end
  end
  return n
end

local function filled_cells(matrix)
  local n = 0
  for _, row in ipairs(matrix) do
    for _, px in ipairs(row) do
      if px then
        n = n + 1
      end
    end
  end
  return n
end

describe("generated sprite geometry", function()
  it("produces frames at the declared canvas size for every asset", function()
    for name, spec in pairs(ASSETS) do
      local frames = sprites.get_pixel_frames(name)
      assert(#frames > 0, name .. " generated no frames")
      for i, matrix in ipairs(frames) do
        assert.are_equal(spec.h, #matrix)
        for y, row in ipairs(matrix) do
          assert(
            #row == spec.w,
            string.format("%s frame %d row %d has %d cells, expected %d", name, i, y, #row, spec.w)
          )
        end
      end
    end
  end)

  it("caches generated frames instead of redrawing on every call", function()
    for name, _ in pairs(ASSETS) do
      assert(
        sprites.get_pixel_frames(name) == sprites.get_pixel_frames(name),
        name .. " regenerates its frames on every lookup"
      )
    end
  end)

  it("draws something in every frame", function()
    for name, _ in pairs(ASSETS) do
      for i, matrix in ipairs(sprites.get_pixel_frames(name)) do
        assert(
          filled_cells(matrix) > 8,
          string.format(
            "%s frame %d is effectively empty (%d pixels)",
            name,
            i,
            filled_cells(matrix)
          )
        )
      end
    end
  end)
end)

describe("generated sprite shading", function()
  -- This suite used to assert the opposite -- at least twelve colours a frame, on
  -- the grounds that volumetric shading should produce a gradient. At 24x16 a
  -- sprite is 24 columns by eight half-block rows and that gradient read as noise:
  -- the cat read as a fox. The art is now silhouette-first, so the contract this
  -- pins is a *bounded* palette per frame, which is also what keeps the three
  -- built-ins from consuming half of `max_highlight_groups` between them.
  local MAX_COLOURS_PER_FRAME = 10
  local MIN_COLOURS_PER_FRAME = 3

  it("fills every frame flat, from a palette small enough to read", function()
    for name, _ in pairs(ASSETS) do
      for index, matrix in ipairs(sprites.get_pixel_frames(name)) do
        local colours = distinct_colors(matrix)
        assert(
          colours <= MAX_COLOURS_PER_FRAME,
          string.format(
            "%s frame %d uses %d colours; flat art should stay at or under %d",
            name,
            index,
            colours,
            MAX_COLOURS_PER_FRAME
          )
        )
        assert(
          colours >= MIN_COLOURS_PER_FRAME,
          string.format(
            "%s frame %d uses only %d colours; a fill, a band and a contour is the floor",
            name,
            index,
            colours
          )
        )
      end
    end
  end)
end)

describe("generated sprite state coverage", function()
  it("resolves every manifest state to frames that exist", function()
    for name, spec in pairs(ASSETS) do
      local count = #sprites.get_pixel_frames(name)
      for state, def in pairs(spec.manifest.states) do
        local frames = def.animation and def.animation.frames
        assert(
          frames and #frames > 0,
          string.format("%s state '%s' declares no frames", name, state)
        )
        for _, idx in ipairs(frames) do
          assert(
            idx >= 0 and idx < count,
            string.format(
              "%s state '%s' references frame %d, outside 0..%d",
              name,
              state,
              idx,
              count - 1
            )
          )
        end
      end
    end
  end)

  it("gives every state its own art rather than sharing one pose", function()
    for name, spec in pairs(ASSETS) do
      local frames = sprites.get_pixel_frames(name)
      local first_by_state = {}
      for state, def in pairs(spec.manifest.states) do
        first_by_state[state] = frame_key(frames[def.animation.frames[1] + 1])
      end
      for a, key_a in pairs(first_by_state) do
        for b, key_b in pairs(first_by_state) do
          if a < b then
            assert(
              key_a ~= key_b,
              string.format("%s states '%s' and '%s' open on identical art", name, a, b)
            )
          end
        end
      end
    end
  end)

  it("animates every multi frame state instead of holding one pose", function()
    for name, spec in pairs(ASSETS) do
      local frames = sprites.get_pixel_frames(name)
      for state, def in pairs(spec.manifest.states) do
        local list = def.animation.frames
        if #list > 1 then
          local keys = {}
          for _, idx in ipairs(list) do
            keys[frame_key(frames[idx + 1])] = true
          end
          assert(
            vim.tbl_count(keys) > 1,
            string.format(
              "%s state '%s' lists %d frames that are all identical",
              name,
              state,
              #list
            )
          )
        end
      end
    end
  end)
end)

describe("generated sprite animation smoothness", function()
  it("changes something between consecutive frames of a cycle", function()
    for name, spec in pairs(ASSETS) do
      local frames = sprites.get_pixel_frames(name)
      for state, def in pairs(spec.manifest.states) do
        local list = def.animation.frames
        for i = 2, #list do
          if list[i] ~= list[i - 1] then
            assert(
              differs(frames[list[i - 1] + 1], frames[list[i] + 1]),
              string.format(
                "%s state '%s': frames %d and %d are identical",
                name,
                state,
                list[i - 1],
                list[i]
              )
            )
          end
        end
      end
    end
  end)

  it("advances at an even pace instead of stalling then cutting", function()
    -- A jump is meant to move further per frame than a breath, so an absolute
    -- cap on step size would just punish fast animations. What actually reads as
    -- a cut is one step far larger than the steps around it, so this checks the
    -- evenness of the pacing and keeps only a loose absolute ceiling.
    for name, spec in pairs(ASSETS) do
      local frames = sprites.get_pixel_frames(name)
      for state, def in pairs(spec.manifest.states) do
        local list = def.animation.frames
        if #list > 2 then
          local steps = {}
          for i = 2, #list do
            steps[#steps + 1] = delta(frames[list[i - 1] + 1], frames[list[i] + 1])
          end

          local sorted = vim.deepcopy(steps)
          table.sort(sorted)
          local median = sorted[math.ceil(#sorted / 2)]

          for i, d in ipairs(steps) do
            assert(
              d < 0.45,
              string.format(
                "%s state '%s': step %d moves %.3f, past the 0.45 ceiling",
                name,
                state,
                i,
                d
              )
            )
            if median > 0.001 then
              assert(
                d <= median * 3.0,
                string.format(
                  "%s state '%s': step %d moves %.3f against a median of %.3f; "
                    .. "that one frame reads as a cut",
                  name,
                  state,
                  i,
                  d,
                  median
                )
              )
            end
          end
        end
      end
    end
  end)

  it("closes looping cycles so the last frame flows into the first", function()
    for name, spec in pairs(ASSETS) do
      local frames = sprites.get_pixel_frames(name)
      for state, def in pairs(spec.manifest.states) do
        local list = def.animation.frames
        if def.animation.loop_anim ~= false and #list > 2 then
          local d = delta(frames[list[#list] + 1], frames[list[1] + 1])
          assert(
            d < 0.22,
            string.format(
              "%s state '%s' loops with a %.3f jump from the last frame back to the first (limit 0.22)",
              name,
              state,
              d
            )
          )
        end
      end
    end
  end)
end)

describe("distract custom asset registration", function()
  local distract = require("distract")

  --- A 2x2 sprite set, deliberately unlike any built-in.
  local function tiny_sprite()
    local px = { 10, 20, 30 }
    return {
      frames = {
        { { px, false }, { false, px } },
        { { false, px }, { px, false } },
      },
      layout = { idle = { 0, 1 } },
      width = 2,
      height = 2,
    }
  end

  it("warns instead of silently drawing a cat for an unknown asset", function()
    local warned = {}
    local orig = vim.notify
    vim.notify = function(msg, level)
      table.insert(warned, { msg = msg, level = level })
    end

    sprites.get_pixel_frames("no_such_creature")

    vim.notify = orig
    assert.is_true(#warned > 0, "an unknown asset must be reported, not substituted in silence")
    assert.is_true(
      warned[1].msg:find("no_such_creature", 1, true) ~= nil,
      "the warning must name the asset that is missing"
    )
  end)

  it("draws registered art rather than the cat", function()
    distract.register_asset("tiny", { sprites = tiny_sprite() })

    assert.is_true(sprites.has_sprite("tiny"))
    local w, h = sprites.get_dimensions("tiny")
    assert.are_equal(2, w)
    assert.are_equal(2, h)
    assert.are_equal(2, #sprites.get_pixel_frames("tiny"))

    local cat_w = sprites.get_dimensions("cat")
    assert.are_not_equal(cat_w, w, "a registered asset must not fall back to cat art")
  end)

  it("registers a manifest so the asset spawns with its own behaviour", function()
    distract.setup({ backend = "halfblock" })
    distract.register_asset("tiny", {
      sprites = tiny_sprite(),
      manifest = {
        asset_type = "procedural",
        initial_state = "idle",
        states = {
          idle = { animation = { frames = { 0, 1 }, fps = 4.0, loop_anim = true } },
        },
      },
    })

    local manifest = distract.config.assets["tiny"]
    assert.is_not_nil(manifest, "a registered manifest must be visible to the engine")
    assert.are_equal("tiny", manifest.name)
    assert.is_true(vim.tbl_contains(distract.get_asset_names(), "tiny"))
  end)

  it("refuses a sprite set with no frames", function()
    local ok = pcall(sprites.register, "broken", {})
    assert.is_false(ok, "a sprite set without frames must not register")
  end)
end)

describe("native-resolution frames as a fourth art source", function()
  local sources = require("distract.sprite_sources")
  local native_sprite = require("distract.native_sprite")

  local function u32(value)
    return string.char(
      value % 256,
      math.floor(value / 256) % 256,
      math.floor(value / 65536) % 256,
      math.floor(value / 16777216) % 256
    )
  end

  --- Writes a one-pixel `.rgba` sidecar whose single pixel is `{9, 9, 9}`.
  local function write_sidecar(path)
    local file = io.open(path, "wb")
    file:write("DRGB" .. string.char(1) .. u32(1) .. u32(1) .. u32(1) .. string.char(9, 9, 9, 255))
    file:close()
  end

  --- Writes a single opaque `{9, 9, 9}` frame wider than a terminal footprint,
  --- so the backends that cannot draw native resolution have something to fit.
  local function write_wide_sidecar(path, width, height)
    local file = io.open(path, "wb")
    file:write("DRGB" .. string.char(1) .. u32(width) .. u32(height) .. u32(1))
    file:write(string.rep(string.char(9, 9, 9, 255), width * height))
    file:close()
  end

  --- Runs `fn` with `vim.notify` silenced: an asset bound to art that does not
  --- resolve reports itself, and that report is not what these tests are about.
  local function quietly(fn)
    local original = vim.notify
    vim.notify = function() end
    local ok, err = pcall(fn)
    vim.notify = original
    if not ok then
      error(err)
    end
  end

  after_each(function()
    native_sprite.reset()
  end)

  it("ignores native_resolution when the manifest has no native_path", function()
    quietly(function()
      sources.bind_manifest("native_test_no_native", { spritesheet = { path = "x.png" } })

      local without_opts = sources.get_pixel_frames("native_test_no_native")
      local with_native =
        sources.get_pixel_frames("native_test_no_native", { native_resolution = true })

      assert.are_equal(without_opts, with_native)
      sources.unbind_manifest("native_test_no_native")
    end)
  end)

  it("serves native resolution to the backend that can draw it", function()
    quietly(function()
      local fixture_path = vim.fn.tempname() .. ".rgba"
      write_sidecar(fixture_path)

      sources.bind_manifest(
        "native_test_with_native",
        { spritesheet = { path = "x.png", native_path = fixture_path } }
      )

      local native_frames =
        sources.get_pixel_frames("native_test_with_native", { native_resolution = true })
      local standard_frames = sources.get_pixel_frames("native_test_with_native")

      assert.are.same({ 9, 9, 9 }, native_frames[1][1][1])
      -- Already within a terminal footprint, so both forms are the sidecar's
      -- own pixels. The point here is that neither is the fallback cat.
      assert.are.same({ 9, 9, 9 }, standard_frames[1][1][1])

      sources.unbind_manifest("native_test_with_native")
      os.remove(fixture_path)
    end)
  end)

  it("fits an oversized sidecar to a terminal footprint for halfblock", function()
    quietly(function()
      local fixture_path = vim.fn.tempname() .. ".rgba"
      write_wide_sidecar(fixture_path, 64, 32)

      sources.bind_manifest(
        "native_test_fitted",
        { spritesheet = { path = "x.png", native_path = fixture_path } }
      )

      local fitted = sources.get_pixel_frames("native_test_fitted")

      assert.are_equal(32, #fitted[1][1], "64 sprite pixels wide must fit to 32 columns")
      assert.are_equal(16, #fitted[1], "the aspect ratio must survive the fit")
      assert.are.same({ 9, 9, 9 }, fitted[1][1][1])

      sources.unbind_manifest("native_test_fitted")
      os.remove(fixture_path)
    end)
  end)

  it("keeps the two footprints apart, whichever backend asked first", function()
    quietly(function()
      local fixture_path = vim.fn.tempname() .. ".rgba"
      write_wide_sidecar(fixture_path, 64, 32)

      sources.bind_manifest(
        "native_test_order",
        { spritesheet = { path = "x.png", native_path = fixture_path } }
      )

      local native_first =
        sources.get_pixel_frames("native_test_order", { native_resolution = true })
      local halfblock_second = sources.get_pixel_frames("native_test_order")

      assert.are_equal(64, #native_first[1][1], "the native request must keep full width")
      assert.are_equal(
        32,
        #halfblock_second[1][1],
        "a name-keyed cache must not serve halfblock the native form"
      )

      sources.unbind_manifest("native_test_order")
      os.remove(fixture_path)
    end)
  end)

  it("reports the fitted footprint as the asset's one dimension", function()
    quietly(function()
      local fixture_path = vim.fn.tempname() .. ".rgba"
      write_wide_sidecar(fixture_path, 128, 72)

      sources.bind_manifest(
        "native_test_dimensions",
        { spritesheet = { path = "x.png", native_path = fixture_path } }
      )

      local width, height = sources.get_dimensions("native_test_dimensions")

      -- Not 128x72 and not the fallback cat's 24x16. This is what
      -- `engine.sprite_cell_size` measures wrapping and floor anchoring against,
      -- so it has to be the size the art is actually drawn at.
      assert.are_equal(32, width)
      assert.are_equal(18, height)

      sources.unbind_manifest("native_test_dimensions")
      os.remove(fixture_path)
    end)
  end)

  it("quantises sidecar art, which the highlight-group cap depends on", function()
    quietly(function()
      local fixture_path = vim.fn.tempname() .. ".rgba"
      write_wide_sidecar(fixture_path, 64, 32)

      sources.bind_manifest(
        "native_test_quantise",
        { spritesheet = { path = "x.png", native_path = fixture_path } }
      )

      assert.is_true(sources.needs_quantising("native_test_quantise"))

      sources.unbind_manifest("native_test_quantise")
      assert.is_false(sources.needs_quantising("native_test_quantise"))
      os.remove(fixture_path)
    end)
  end)

  it("falls through to the standard chain when the sidecar is unreadable", function()
    quietly(function()
      sources.bind_manifest(
        "native_test_broken",
        { spritesheet = { path = "x.png", native_path = "/does/not/exist.rgba" } }
      )

      local frames = sources.get_pixel_frames("native_test_broken", { native_resolution = true })

      assert.is_not_nil(frames)
      assert.is_true(#frames > 0, "a bad sidecar must not take down the render loop")

      sources.unbind_manifest("native_test_broken")
    end)
  end)
end)
