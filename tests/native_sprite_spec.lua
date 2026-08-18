require("tests.test_harness")

local native_sprite = require("distract.native_sprite")

local function u32(value)
  return string.char(
    value % 256,
    math.floor(value / 256) % 256,
    math.floor(value / 65536) % 256,
    math.floor(value / 16777216) % 256
  )
end

--- Builds a minimal valid .rgba buffer: header + one 1x1 opaque red pixel.
local function build_fixture(path)
  local body = "DRGB" .. string.char(1) .. u32(1) .. u32(1) .. u32(1) .. string.char(255, 0, 0, 255)
  local file = io.open(path, "wb")
  file:write(body)
  file:close()
end

describe("distract.native_sprite", function()
  local fixture_path = vim.fn.tempname() .. ".rgba"

  after_each(function()
    os.remove(fixture_path)
    native_sprite.reset()
  end)

  it("source_of returns nil when the manifest has no native_path", function()
    assert.is_nil(native_sprite.source_of({ spritesheet = { path = "x.png" } }))
    assert.is_nil(native_sprite.source_of(nil))
  end)

  it("source_of returns the native_path when present", function()
    local source = native_sprite.source_of({ spritesheet = { native_path = "assets/x/x.rgba" } })
    assert.are.same({ native_path = "assets/x/x.rgba" }, source)
  end)

  it("same_source compares by native_path, nil-safe", function()
    assert.is_true(native_sprite.same_source(nil, nil))
    assert.is_false(native_sprite.same_source(nil, { native_path = "a" }))
    assert.is_true(native_sprite.same_source({ native_path = "a" }, { native_path = "a" }))
    assert.is_false(native_sprite.same_source({ native_path = "a" }, { native_path = "b" }))
  end)

  it("load decodes a valid fixture into a one-pixel frame", function()
    build_fixture(fixture_path)

    local frames, err = native_sprite.load(fixture_path)

    assert.is_nil(err)
    assert.are_equal(1, #frames)
    assert.are.same({ 255, 0, 0 }, frames[1][1][1])
  end)

  it("load returns nil, err for a missing file instead of throwing", function()
    local frames, err = native_sprite.load("/does/not/exist.rgba")
    assert.is_nil(frames)
    assert.is_not_nil(err)
  end)

  it("load returns nil, err for bad magic instead of throwing", function()
    local file = io.open(fixture_path, "wb")
    file:write(
      "XXXX" .. string.char(1) .. u32(1) .. u32(1) .. u32(1) .. string.char(255, 0, 0, 255)
    )
    file:close()

    local frames, err = native_sprite.load(fixture_path)

    assert.is_nil(frames)
    assert.is_not_nil(err)
  end)

  it("load returns nil, err when the declared length does not match the file", function()
    local file = io.open(fixture_path, "wb")
    file:write(
      "DRGB" .. string.char(1) .. u32(4) .. u32(4) .. u32(9) .. string.char(255, 0, 0, 255)
    )
    file:close()

    local frames, err = native_sprite.load(fixture_path)

    assert.is_nil(frames)
    assert.is_not_nil(err)
  end)

  it("load reads a fully transparent pixel as the false sentinel", function()
    local file = io.open(fixture_path, "wb")
    file:write("DRGB" .. string.char(1) .. u32(1) .. u32(1) .. u32(1) .. string.char(9, 9, 9, 0))
    file:close()

    local frames = native_sprite.load(fixture_path)

    assert.is_false(frames[1][1][1])
  end)

  it("load caches by path so a second call does not re-read the file", function()
    build_fixture(fixture_path)
    local first = native_sprite.load(fixture_path)
    os.remove(fixture_path)
    local second = native_sprite.load(fixture_path)
    assert.are_equal(first, second)
  end)
end)
