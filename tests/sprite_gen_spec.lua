require("tests.test_harness")

local gen = require("distract.sprite_gen")

local function count_filled(canvas)
  local n = 0
  for y = 1, canvas.h do
    for x = 1, canvas.w do
      if canvas.rows[y][x] then n = n + 1 end
    end
  end
  return n
end

local function distinct_colors(canvas)
  local seen, n = {}, 0
  for y = 1, canvas.h do
    for x = 1, canvas.w do
      local c = canvas.rows[y][x]
      if c then
        local key = c[1] .. "," .. c[2] .. "," .. c[3]
        if not seen[key] then seen[key] = true; n = n + 1 end
      end
    end
  end
  return n
end

local function luminance(c)
  return 0.2126 * c[1] + 0.7152 * c[2] + 0.0722 * c[3]
end

describe("distract.sprite_gen canvas", function()
  it("creates a fully transparent canvas of the requested size", function()
    local c = gen.canvas(24, 16)
    assert.are_equal(24, c.w)
    assert.are_equal(16, c.h)
    assert.are_equal(16, #c.rows)
    assert.are_equal(0, count_filled(c))
  end)

  it("converts to a matrix with one full length row per pixel row", function()
    local c = gen.canvas(24, 16)
    local m = gen.to_matrix(c)
    assert.are_equal(16, #m)
    for y = 1, 16 do
      assert.are_equal(24, #m[y])
    end
  end)

  it("uses false, never nil, for transparent cells so rows keep their length", function()
    local m = gen.to_matrix(gen.canvas(8, 4))
    for y = 1, 4 do
      for x = 1, 8 do
        assert.is_false(m[y][x])
      end
    end
  end)

  it("ignores writes outside the canvas instead of erroring", function()
    local c = gen.canvas(8, 4)
    assert.has_no.errors(function()
      gen.set(c, -5, -5, { 1, 2, 3 })
      gen.set(c, 100, 100, { 1, 2, 3 })
      gen.set(c, 0, 0, { 1, 2, 3 })
    end)
    assert.are_equal(0, count_filled(c))
  end)
end)

describe("distract.sprite_gen primitives", function()
  it("rect fills exactly width times height pixels", function()
    local c = gen.canvas(24, 16)
    gen.rect(c, 3, 4, 6, 5, { 200, 100, 50 })
    assert.are_equal(30, count_filled(c))
    assert.is_not_nil(gen.get(c, 3, 4))
    assert.is_not_nil(gen.get(c, 8, 8))
    assert.is_nil(gen.get(c, 9, 8))
    assert.is_nil(gen.get(c, 3, 9))
  end)

  it("rect clips at the canvas edge without erroring", function()
    local c = gen.canvas(8, 8)
    gen.rect(c, -2, -2, 20, 20, { 10, 20, 30 })
    assert.are_equal(64, count_filled(c))
  end)

  it("ellipse is symmetric about its centre", function()
    local c = gen.canvas(21, 21)
    gen.ellipse(c, 11, 11, 8, 8, { 255, 0, 0 })
    for dy = -8, 8 do
      for dx = -8, 8 do
        local a = gen.get(c, 11 + dx, 11 + dy) ~= nil
        local b = gen.get(c, 11 - dx, 11 + dy) ~= nil
        assert(a == b, string.format("ellipse asymmetric at dx=%d dy=%d", dx, dy))
      end
    end
  end)

  it("line connects both endpoints", function()
    local c = gen.canvas(16, 16)
    gen.line(c, 2, 2, 13, 9, { 0, 255, 0 })
    assert.is_not_nil(gen.get(c, 2, 2))
    assert.is_not_nil(gen.get(c, 13, 9))
  end)
end)

describe("distract.sprite_gen colour", function()
  it("shade darkens with a negative amount and lightens with a positive one", function()
    local base = { 120, 120, 120 }
    assert(luminance(gen.shade(base, -0.5)) < luminance(base))
    assert(luminance(gen.shade(base, 0.5)) > luminance(base))
  end)

  it("shade never leaves the 0..255 range", function()
    for _, amount in ipairs({ -3, -1, 0, 1, 3 }) do
      for _, base in ipairs({ { 0, 0, 0 }, { 255, 255, 255 }, { 12, 200, 90 } }) do
        local out = gen.shade(base, amount)
        for i = 1, 3 do
          assert(out[i] >= 0 and out[i] <= 255,
            string.format("channel %d = %s out of range", i, tostring(out[i])))
        end
      end
    end
  end)

  it("mix interpolates between two colours", function()
    assert.are.same({ 0, 0, 0 }, gen.mix({ 0, 0, 0 }, { 100, 100, 100 }, 0))
    assert.are.same({ 100, 100, 100 }, gen.mix({ 0, 0, 0 }, { 100, 100, 100 }, 1))
    assert.are.same({ 50, 50, 50 }, gen.mix({ 0, 0, 0 }, { 100, 100, 100 }, 0.5))
  end)
end)

describe("distract.sprite_gen volumetric orb", function()
  it("renders a shaded volume rather than a flat fill", function()
    local c = gen.canvas(24, 24)
    gen.orb(c, 12, 12, 9, 9, { 200, 120, 60 })
    assert(distinct_colors(c) >= 6, string.format(
      "an orb should produce a gradient, got %d distinct colours", distinct_colors(c)))
  end)

  it("is brighter on the side facing the light", function()
    local c = gen.canvas(24, 24)
    gen.orb(c, 12, 12, 9, 9, { 200, 120, 60 }, { light = { -0.6, -0.6, 0.5 } })
    local lit = gen.get(c, 12 - 4, 12 - 4)
    local dark = gen.get(c, 12 + 4, 12 + 4)
    assert.is_not_nil(lit)
    assert.is_not_nil(dark)
    assert(luminance(lit) > luminance(dark),
      "the pixel facing the light must be brighter than the one facing away")
  end)

  it("follows the light direction when it is reversed", function()
    local c = gen.canvas(24, 24)
    gen.orb(c, 12, 12, 9, 9, { 200, 120, 60 }, { light = { 0.6, 0.6, 0.5 } })
    local upper_left = gen.get(c, 12 - 4, 12 - 4)
    local lower_right = gen.get(c, 12 + 4, 12 + 4)
    assert(luminance(lower_right) > luminance(upper_left),
      "reversing the light must reverse which side is lit")
  end)

  it("stays inside its own bounding ellipse", function()
    local c = gen.canvas(24, 24)
    gen.orb(c, 12, 12, 6, 6, { 200, 120, 60 })
    assert.is_nil(gen.get(c, 12, 12 - 8))
    assert.is_nil(gen.get(c, 12 - 8, 12))
    assert.is_not_nil(gen.get(c, 12, 12))
  end)
end)

describe("distract.sprite_gen pose interpolation", function()
  it("samples a looping cycle without repeating the first pose at the end", function()
    local poses = gen.cycle(4, function(t) return { t = t } end)
    assert.are_equal(4, #poses)
    assert.are_equal(0, poses[1].t)
    assert(poses[4].t < 1, "a looping cycle must not sample t = 1, which duplicates t = 0")
  end)

  it("samples a one shot sequence across the full range", function()
    local poses = gen.sequence(5, function(t) return { t = t } end)
    assert.are_equal(5, #poses)
    assert.are_equal(0, poses[1].t)
    assert.are_equal(1, poses[5].t)
  end)

  it("advances monotonically", function()
    local poses = gen.sequence(6, function(t) return { t = t } end)
    for i = 2, #poses do
      assert(poses[i].t > poses[i - 1].t, "pose parameter must increase")
    end
  end)
end)
