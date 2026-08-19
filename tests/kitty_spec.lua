require("tests.test_harness")

local backends = require("distract.backends")
local detect = require("distract.kitty.detect")
local diacritics = require("distract.kitty.diacritics")
local frames = require("distract.kitty.frames")
local kitty = require("distract.kitty")
local kitty_renderer = require("distract.kitty.renderer")
local protocol = require("distract.kitty.protocol")
local renderer = require("distract.renderer")
local writer = require("distract.kitty.writer")

--- Runs `fn` with the terminal replaced by a capture, and returns what was
--- written. The seam the whole backend is testable through: escape generation
--- is pure, so nothing here needs a tty.
local function captured(fn)
  local sequences = {}
  writer.set_writer(function(sequence)
    sequences[#sequences + 1] = sequence
    return true
  end)
  local ok, err = pcall(fn)
  writer.reset_writer()
  if not ok then
    error(err)
  end
  return sequences
end

--- Registers the backend as though the terminal had answered the query.
local function with_kitty(fn)
  local truecolor = vim.o.termguicolors
  vim.o.termguicolors = true
  detect.override(true)
  kitty.setup()

  local sequences = captured(fn)

  kitty.reset()
  backends.reset()
  vim.o.termguicolors = truecolor
  return sequences
end

local function payload_of(sequence)
  return sequence:match("^\27_G[^;]*;(.*)\27\\$")
end

local function keys_of(sequence)
  return sequence:match("^\27_G([^;]*);")
end

describe("distract.kitty diacritics", function()
  it("carries kitty's whole row/column table", function()
    assert.are_equal(297, diacritics.LIMIT)
    assert.are_equal(0x0305, diacritics.CODEPOINTS[1])
    assert.are_equal(0x030D, diacritics.CODEPOINTS[2])
  end)

  it("encodes an index as the combining character at that position", function()
    assert.are_equal(vim.fn.nr2char(0x0305), diacritics.char(0))
    assert.are_equal(vim.fn.nr2char(0x030D), diacritics.char(1))
  end)

  it("refuses an index past the end rather than wrapping to a wrong cell", function()
    assert.is_false(pcall(diacritics.char, diacritics.LIMIT))
    assert.is_false(pcall(diacritics.char, -1))
  end)
end)

describe("distract.kitty protocol", function()
  it("builds a placeholder cell from the reserved codepoint and two diacritics", function()
    local cell = protocol.cell(0, 1)
    assert.are_equal(protocol.PLACEHOLDER .. diacritics.char(0) .. diacritics.char(1), cell)
    assert.are_equal(3, vim.fn.strchars(cell))
  end)

  it("runs cells left to right across a row", function()
    local run = protocol.cell_run(2, 0, 2)
    assert.are_equal(protocol.cell(2, 0) .. protocol.cell(2, 1) .. protocol.cell(2, 2), run)
  end)

  it("names an image by its id as a 24 bit foreground colour", function()
    assert.are_equal("#0000ff", protocol.image_colour(255))
    assert.are_equal("#010000", protocol.image_colour(65536))
  end)

  it("refuses an image id that cannot fit two diacritics", function()
    assert.is_false(pcall(protocol.image_colour, 0))
    assert.is_false(pcall(protocol.image_colour, protocol.MAX_IMAGE_ID + 1))
  end)

  it("transmits a small image as one raw RGBA chunk", function()
    local rgba = string.rep("\255\0\0\255", 4)
    local escapes = protocol.transmit({
      id = 7,
      pixel_w = 2,
      pixel_h = 2,
      cols = 2,
      rows = 1,
      rgba = rgba,
    })

    assert.are_equal(1, #escapes)
    local keys = keys_of(escapes[1])
    assert.is_not_nil(keys:find("a=T", 1, true))
    assert.is_not_nil(keys:find("U=1", 1, true))
    assert.is_not_nil(keys:find("f=32", 1, true))
    assert.is_not_nil(keys:find("i=7", 1, true))
    assert.is_not_nil(keys:find("s=2,v=2,c=2,r=1", 1, true))
    assert.is_not_nil(keys:find("m=0", 1, true))
    assert.are_equal(rgba, vim.base64.decode(payload_of(escapes[1])))
  end)

  it("splits a large image at the protocol's chunk size and reassembles", function()
    local rgba = string.rep("\1\2\3\4", 3000)
    local escapes = protocol.transmit({
      id = 9,
      pixel_w = 100,
      pixel_h = 120,
      cols = 100,
      rows = 60,
      rgba = rgba,
    })

    assert.is_true(#escapes > 1)
    local parts = {}
    for index, escape in ipairs(escapes) do
      local chunk = payload_of(escape)
      assert.is_true(#chunk <= protocol.CHUNK_BYTES)
      local expected_more = index < #escapes and "m=1" or "m=0"
      assert.is_not_nil(keys_of(escape):find(expected_more, 1, true))
      parts[#parts + 1] = chunk
    end

    assert.are_equal(rgba, vim.base64.decode(table.concat(parts)))
    assert.is_not_nil(keys_of(escapes[1]):find("i=9", 1, true))
    assert.is_nil(keys_of(escapes[2]):find("i=9", 1, true))
  end)

  it("suppresses the terminal's reply on everything but the probe", function()
    local escapes = protocol.transmit({
      id = 3,
      pixel_w = 1,
      pixel_h = 2,
      cols = 1,
      rows = 1,
      rgba = string.rep("\0", 8),
    })
    for _, escape in ipairs(escapes) do
      assert.is_not_nil(keys_of(escape):find("q=2", 1, true))
    end
    assert.is_not_nil(keys_of(protocol.delete(3)):find("q=2", 1, true))
    assert.is_nil(keys_of(protocol.probe()):find("q=2", 1, true))
  end)

  it("frees the image data when deleting, not just the placement", function()
    local keys = keys_of(protocol.delete(12))
    assert.is_not_nil(keys:find("a=d", 1, true))
    assert.is_not_nil(keys:find("d=I", 1, true))
    assert.is_not_nil(keys:find("i=12", 1, true))
  end)

  it("recognises the probe being answered, and nothing else", function()
    assert.is_true(protocol.is_probe_ok("\27_Gi=31;OK\27\\"))
    assert.is_false(protocol.is_probe_ok("\27_Gi=31;ENOTSUPPORTED\27\\"))
    assert.is_false(protocol.is_probe_ok("\27[0n"))
    assert.is_false(protocol.is_probe_ok(nil))
  end)
end)

describe("distract.kitty frames", function()
  it("occupies the same cells the half-block renderer would", function()
    local frame = frames.describe("cat", 1, false)
    local _, _, halfblock_w, halfblock_h =
      require("distract.terminal_sprites").get_rendered_frame("cat", 1, false)

    assert.are_equal(halfblock_w, frame.cols)
    assert.are_equal(halfblock_h, frame.rows)
  end)

  it("encodes four bytes per pixel over the whole padded canvas", function()
    local frame = frames.describe("cat", 1, false)
    assert.are_equal(frame.pixel_w * frame.pixel_h * 4, #frame.rgba)
    assert.are_equal(0, frame.pixel_h % 2)
  end)

  it("gives a transparent pixel a zero alpha", function()
    local frame = frames.describe("cat", 1, false)
    assert.is_not_nil(frame.rgba:find("\0\0\0\0", 1, true))
  end)

  it("mirrors the art for a flipped frame", function()
    local facing = frames.describe("cat", 1, false)
    local flipped = frames.describe("cat", 1, true)
    assert.are_equal(facing.cols, flipped.cols)
    assert.are_not_equal(facing.rgba, flipped.rgba)
  end)

  it("describes only the cells that have a pixel in them", function()
    local frame = frames.describe("cat", 1, false)
    local spans = frames.spans(frame, frame.cols, frame.rows)

    local drawn = 0
    for row = 0, frame.rows - 1 do
      for _, span in ipairs(spans[row]) do
        assert.is_true(span[1] >= 0 and span[2] < frame.cols)
        drawn = drawn + (span[2] - span[1] + 1)
      end
    end

    assert.is_true(drawn > 0)
    assert.is_true(drawn < frame.cols * frame.rows)
  end)

  it("resamples the drawn cells onto a parallaxed grid", function()
    local frame = frames.describe("cat", 1, false)
    local half = frames.spans(frame, math.floor(frame.cols / 2), math.floor(frame.rows / 2))

    for row = 0, math.floor(frame.rows / 2) - 1 do
      for _, span in ipairs(half[row]) do
        assert.is_true(span[2] < math.floor(frame.cols / 2))
      end
    end
  end)

  -- Pins the assumption that this module and `protocol.lua` are resolution-
  -- agnostic (spec:
  -- docs/superpowers/specs/2026-08-16-sprite-import-pipeline-design.md § 3.4).
  -- If this needs a code change to pass, the placement or transmission math
  -- assumed a cell-grid-sized frame somewhere -- fix it there, not here.
  it("encodes a native-resolution frame the same way as a cell-grid one", function()
    local sources = require("distract.sprite_sources")
    local native_sprite = require("distract.native_sprite")
    local asset_name = "native_res_characterization_test"
    local width, height = 24, 16

    local function u32(value)
      return string.char(
        value % 256,
        math.floor(value / 256) % 256,
        math.floor(value / 65536) % 256,
        math.floor(value / 16777216) % 256
      )
    end

    local pixels = {}
    for _ = 1, width * height do
      pixels[#pixels + 1] = string.char(10, 20, 30, 255)
    end
    local fixture_path = vim.fn.tempname() .. ".rgba"
    local file = io.open(fixture_path, "wb")
    file:write(
      "DRGB" .. string.char(1) .. u32(width) .. u32(height) .. u32(1) .. table.concat(pixels)
    )
    file:close()

    sources.bind_manifest(asset_name, { spritesheet = { native_path = fixture_path } })

    local described = frames.describe(asset_name, 1, false)

    assert.is_not_nil(described)
    assert.are_equal(width, described.pixel_w)
    assert.are_equal(height, described.pixel_h)
    assert.are_equal(width, described.cols)
    assert.are_equal(height / 2, described.rows)
    assert.are_equal(width * height * 4, #described.rgba)

    sources.unbind_manifest(asset_name)
    native_sprite.reset()
    os.remove(fixture_path)
  end)

  -- The regression: `cols` used to be the image's own pixel width, so a 128-pixel
  -- import was spread across 128 columns while `sprites.get_dimensions` reported
  -- 24 and the engine wrapped and anchored against that. Fidelity belongs in the
  -- payload; the footprint has to be the one number every consumer shares.
  it("fills the shared cell footprint with native pixels, not its own width", function()
    local sources = require("distract.sprite_sources")
    local native_sprite = require("distract.native_sprite")
    local asset_name = "native_res_footprint_test"
    local width, height = 128, 72

    local function u32(value)
      return string.char(
        value % 256,
        math.floor(value / 256) % 256,
        math.floor(value / 65536) % 256,
        math.floor(value / 16777216) % 256
      )
    end

    -- Only the bottom half is opaque, on purpose. A fully opaque frame makes
    -- every candidate mask identical, so it cannot tell a mask built on the
    -- footprint grid from one built on the image's own grid.
    local fixture_path = vim.fn.tempname() .. ".rgba"
    local file = io.open(fixture_path, "wb")
    file:write("DRGB" .. string.char(1) .. u32(width) .. u32(height) .. u32(1))
    file:write(string.rep(string.char(0, 0, 0, 0), width * height / 2))
    file:write(string.rep(string.char(10, 20, 30, 255), width * height / 2))
    file:close()

    sources.bind_manifest(asset_name, { spritesheet = { native_path = fixture_path } })

    local footprint_w, footprint_h = sources.get_dimensions(asset_name)
    local described = frames.describe(asset_name, 1, false)

    assert.are_equal(width, described.pixel_w, "the payload must keep native width")
    assert.are_equal(height, described.pixel_h, "the payload must keep native height")
    assert.are_equal(
      footprint_w,
      described.cols,
      "kitty must occupy the footprint the engine measures against"
    )
    assert.are_equal(math.ceil(footprint_h / 2), described.rows)

    local mask_rows = 0
    for _ in pairs(described.mask) do
      mask_rows = mask_rows + 1
    end
    assert.are_equal(
      described.rows,
      mask_rows,
      "spans() indexes the mask by frame.rows, so a mask on the image's grid tears"
    )

    -- The mask must describe the bottom half of the *art*. Built on the image's
    -- own grid instead, these nine cell rows would read the top 17 pixel rows of
    -- 72 -- entirely transparent -- and the sprite would vanish.
    local spans = frames.spans(described, described.cols, described.rows)
    assert.are_equal(0, #spans[0], "the top of the frame is transparent and must not be claimed")
    assert.is_true(
      #spans[described.rows - 1] > 0,
      "the bottom of the frame is opaque and must draw; an empty mask here means "
        .. "it was built on the image's pixel grid rather than the footprint's"
    )

    sources.unbind_manifest(asset_name)
    native_sprite.reset()
    os.remove(fixture_path)
  end)
end)

describe("distract.kitty writer", function()
  it("sends every sequence of a transmission in order", function()
    local sent = captured(function()
      writer.write_all({ "one", "two", "three" })
    end)
    assert.are.same({ "one", "two", "three" }, sent)
  end)

  it("refuses to write nothing", function()
    assert.is_false(pcall(writer.write, ""))
    assert.is_false(pcall(writer.write, nil))
  end)

  it("restores the terminal when the capture is removed", function()
    writer.set_writer(function()
      return true
    end)
    writer.reset_writer()
    -- Headless, so there is no terminal to reach; the contract is that it
    -- reports failure rather than raising.
    assert.are_equal("boolean", type(writer.write("\27_Gq=2;\27\\")))
  end)
end)

describe("distract.kitty detection", function()
  after_each(function()
    detect.reset()
  end)

  it("recognises the terminals confirmed to answer the query", function()
    local term, program = vim.env.TERM, vim.env.TERM_PROGRAM
    vim.env.TERM = "xterm-ghostty"
    vim.env.TERM_PROGRAM = nil
    assert.is_true(detect.env_says_yes())

    vim.env.TERM = "xterm-256color"
    vim.env.TERM_PROGRAM = "ghostty"
    assert.is_true(detect.env_says_yes())

    vim.env.TERM_PROGRAM = nil
    assert.is_false(detect.env_says_yes())

    vim.env.TERM, vim.env.TERM_PROGRAM = term, program
  end)

  it("fails closed with no UI attached", function()
    assert.are_equal(0, #vim.api.nvim_list_uis())
    assert.is_false(detect.is_available())
  end)

  it("answers once and keeps the answer until reset", function()
    detect.override(true)
    assert.is_true(detect.is_available())
    detect.reset()
    assert.is_false(detect.is_available())
  end)
end)

describe("distract.kitty backend registration", function()
  it("does not offer itself until the terminal has answered", function()
    assert.is_false(kitty.is_registered())
    assert.is_false(renderer.supports("kitty"))
    assert.are.same({ "halfblock", "overlay" }, backends.names())
    assert.are_equal("halfblock", backends.resolve("kitty", true))
  end)

  it("declines without termguicolors, because the image id would be rounded", function()
    local truecolor = vim.o.termguicolors
    vim.o.termguicolors = false
    detect.override(true)

    assert.is_false(kitty.setup())
    assert.is_false(kitty.is_registered())

    kitty.reset()
    vim.o.termguicolors = truecolor
  end)

  it("resolves its own name and its terminals once registered", function()
    with_kitty(function()
      assert.is_true(kitty.is_registered())
      assert.is_true(renderer.supports("kitty"))
      assert.are_equal("kitty", backends.resolve("kitty"))
      assert.are_equal("kitty", backends.resolve("ghostty"))
      assert.are_equal("kitty", backends.resolve("wezterm"))
    end)
  end)

  it("advertises per-pixel alpha and the scaling parallax needs", function()
    with_kitty(function()
      assert.are.same(
        { scale = true, alpha = "pixel", native_resolution = true },
        backends.capabilities("kitty")
      )
      assert.is_true(backends.supports_parallax("kitty"))
    end)
  end)

  it("offers itself unasked to a terminal the environment already names", function()
    local term = vim.env.TERM
    local truecolor = vim.o.termguicolors
    vim.env.TERM = "xterm-ghostty"
    vim.o.termguicolors = true
    detect.override(true)

    assert.is_true(kitty.ensure_offered())
    assert.is_true(vim.tbl_contains(backends.names(), "kitty"))

    kitty.reset()
    backends.reset()
    vim.env.TERM = term
    vim.o.termguicolors = truecolor
  end)

  it("sends nothing at all in a session that never asks for it", function()
    local term, program, window = vim.env.TERM, vim.env.TERM_PROGRAM, vim.env.KITTY_WINDOW_ID
    vim.env.TERM = "xterm-256color"
    vim.env.TERM_PROGRAM = nil
    vim.env.KITTY_WINDOW_ID = nil
    detect.reset()

    local sent = captured(function()
      assert.is_false(kitty.ensure_offered())
      assert.is_false(kitty.ensure_registered("halfblock"))
    end)
    assert.are.same({}, sent)

    detect.reset()
    vim.env.TERM, vim.env.TERM_PROGRAM, vim.env.KITTY_WINDOW_ID = term, program, window
  end)

  it("puts the registry back when it resets", function()
    with_kitty(function() end)
    assert.are.same({ "halfblock", "overlay" }, backends.names())
    assert.is_false(kitty.is_registered())
  end)
end)

describe("distract.kitty drawing", function()
  local function entity(overrides)
    return vim.tbl_extend("force", {
      id = 1,
      asset_name = "cat",
      current_state = "idle",
      frame_idx = 1,
      x = 4,
      y = 2,
      flip_x = false,
      z_index = 10,
      manifest = require("distract.manifests.cat"),
    }, overrides or {})
  end

  it("transmits a frame once, however many times it is drawn", function()
    local sent = with_kitty(function()
      kitty_renderer.surface(entity())
      kitty_renderer.surface(entity({ id = 2 }))
      kitty_renderer.surface(entity({ id = 3 }))
    end)

    local transmissions = 0
    for _, escape in ipairs(sent) do
      if keys_of(escape):find("a=T", 1, true) then
        transmissions = transmissions + 1
      end
    end
    assert.are_equal(1, transmissions)
  end)

  it("gives each frame and each facing its own image", function()
    local ids
    with_kitty(function()
      kitty_renderer.surface(entity())
      kitty_renderer.surface(entity({ flip_x = true }))
      kitty_renderer.surface(entity({ frame_idx = 2 }))
      ids = kitty_renderer.transmitted_ids()
    end)

    assert.are_equal(3, #ids)
    assert.are_not_equal(ids[1], ids[2])
    assert.are_not_equal(ids[2], ids[3])
  end)

  it("produces a surface the shared placement path can use", function()
    local surface
    with_kitty(function()
      surface = kitty_renderer.surface(entity())
    end)

    local frame = frames.describe("cat", 1, false)
    assert.are_equal(frame.cols, surface.width)
    assert.are_equal(frame.rows, surface.height)
    assert.are_equal(surface.buf, surface.key)
    assert.are_equal("function", type(surface.runs))
  end)

  it("draws a distant sprite into fewer cells, as its own placement", function()
    local near, far, sent
    sent = with_kitty(function()
      near = kitty_renderer.surface(entity())
      far = kitty_renderer.surface(entity({ id = 2, parallax = 0.5 }))
    end)

    assert.are_equal(math.floor(near.width * 0.5 + 0.5), far.width)
    assert.are_equal(math.floor(near.height * 0.5 + 0.5), far.height)
    assert.are_not_equal(near.buf, far.buf)

    local placements = 0
    for _, escape in ipairs(sent) do
      if keys_of(escape):find("a=T", 1, true) then
        placements = placements + 1
      end
    end
    assert.are_equal(2, placements)
  end)

  it("places through the same path the half-block backend uses", function()
    renderer.clear_all()
    with_kitty(function()
      renderer.draw({ entity({ x = 6, y = 3 }) }, "kitty")
    end)

    local placed = renderer.window_state(1)
    assert.is_not_nil(placed)
    assert.are_equal(6, placed.col)
    assert.are_equal(3, placed.row)
    assert.are_equal(frames.describe("cat", 1, false).cols, placed.width)
    renderer.clear_all()
  end)

  it("deletes every image it transmitted when the cache is dropped", function()
    local sequences = {}
    local truecolor = vim.o.termguicolors
    vim.o.termguicolors = true
    detect.override(true)
    kitty.setup()

    writer.set_writer(function(sequence)
      sequences[#sequences + 1] = sequence
      return true
    end)
    kitty_renderer.surface(entity())
    kitty_renderer.surface(entity({ frame_idx = 2 }))
    local ids = kitty_renderer.transmitted_ids()
    kitty_renderer.reset()
    writer.reset_writer()

    local deleted = {}
    for _, escape in ipairs(sequences) do
      local id = keys_of(escape):match("^a=d,d=I,i=(%d+)")
      if id then
        deleted[#deleted + 1] = tonumber(id)
      end
    end
    table.sort(deleted)

    assert.are.same(ids, deleted)
    assert.are.same({}, kitty_renderer.transmitted_ids())

    kitty.reset()
    backends.reset()
    vim.o.termguicolors = truecolor
  end)

  it("restarts id allocation from the beginning of the range after reset", function()
    local truecolor = vim.o.termguicolors
    vim.o.termguicolors = true
    detect.override(true)
    kitty.setup()

    writer.set_writer(function()
      return true
    end)

    kitty_renderer.surface(entity())
    local first_batch_ids = kitty_renderer.transmitted_ids()

    kitty_renderer.reset()

    kitty_renderer.surface(entity())
    local second_batch_ids = kitty_renderer.transmitted_ids()

    writer.reset_writer()

    assert.are_equal(#first_batch_ids, #second_batch_ids)
    assert.are.same(first_batch_ids, second_batch_ids)

    kitty.reset()
    backends.reset()
    vim.o.termguicolors = truecolor
  end)
end)
