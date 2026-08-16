local gif = require("distract.gif")
local builder = require("tests.gif_builder")

local RED = { 255, 0, 0 }
local GREEN = { 0, 255, 0 }
local BLUE = { 0, 0, 255 }
local WHITE = { 255, 255, 255 }
local PALETTE = { RED, GREEN, BLUE, WHITE }

--- A one-frame GIF over the four-colour palette above.
local function single_frame(opts)
  return builder.header({ width = opts.width, height = opts.height, palette = PALETTE })
    .. (opts.control or "")
    .. builder.image({
      width = opts.frame_width or opts.width,
      height = opts.frame_height or opts.height,
      left = opts.left,
      top = opts.top,
      indices = opts.indices,
      palette = opts.palette,
      interlace = opts.interlace,
    })
    .. builder.TRAILER
end

describe("distract.gif decoding", function()
  it("refuses bytes that do not start with a GIF signature", function()
    local decoded, err = gif.decode_bytes("not a gif at all")
    assert.is_nil(decoded)
    assert(err:match("signature"), "expected a signature error, got " .. tostring(err))
  end)

  it("refuses a stream that ends mid-header", function()
    local decoded, err = gif.decode_bytes("GIF89a\1\0")
    assert.is_nil(decoded)
    assert.is_not_nil(err)
  end)

  it("resolves palette indices into RGB triples", function()
    local decoded, err = gif.decode_bytes(single_frame({
      width = 2,
      height = 2,
      indices = { 0, 1, 2, 3 },
    }))

    assert.is_nil(err)
    assert.are_equal(2, decoded.width)
    assert.are_equal(2, decoded.height)
    assert.are_equal(1, #decoded.frames)
    assert.are.same({ RED, GREEN }, decoded.frames[1].pixels[1])
    assert.are.same({ BLUE, WHITE }, decoded.frames[1].pixels[2])
  end)

  it("reads GIF87a as well as GIF89a", function()
    local bytes = builder.header({ width = 1, height = 1, palette = PALETTE, version = "87a" })
      .. builder.image({ width = 1, height = 1, indices = { 2 } })
      .. builder.TRAILER

    local decoded, err = gif.decode_bytes(bytes)
    assert.is_nil(err)
    assert.are.same({ BLUE }, decoded.frames[1].pixels[1])
  end)

  it("marks the transparent index as an unpainted pixel", function()
    local decoded = gif.decode_bytes(single_frame({
      width = 2,
      height = 1,
      indices = { 0, 1 },
      control = builder.graphic_control({ transparent_index = 1 }),
    }))

    assert.are.same(RED, decoded.frames[1].pixels[1][1])
    assert.are_equal(false, decoded.frames[1].pixels[1][2])
  end)

  it("prefers a local colour table over the global one", function()
    local decoded = gif.decode_bytes(single_frame({
      width = 1,
      height = 1,
      indices = { 0 },
      palette = { GREEN },
    }))

    assert.are.same(GREEN, decoded.frames[1].pixels[1][1])
  end)

  it("puts interlaced rows back in screen order", function()
    -- Interlace transmits every 8th row from 0, then every 8th from 4, then
    -- every 4th from 2, then every 2nd from 1. Each row here is a single pixel
    -- whose colour names the screen row it belongs on.
    local order = { 0, 4, 2, 6, 1, 3, 5, 7 }
    local indices = {}
    for _, row in ipairs(order) do
      indices[#indices + 1] = row % 4
    end

    local decoded = gif.decode_bytes(single_frame({
      width = 1,
      height = 8,
      indices = indices,
      interlace = true,
    }))

    for row = 1, 8 do
      assert.are.same(PALETTE[(row - 1) % 4 + 1], decoded.frames[1].pixels[row][1])
    end
  end)

  it("places a frame smaller than the canvas at its own offset", function()
    local decoded = gif.decode_bytes(single_frame({
      width = 3,
      height = 2,
      frame_width = 1,
      frame_height = 1,
      left = 2,
      top = 1,
      indices = { 1 },
    }))

    local pixels = decoded.frames[1].pixels
    assert.are_equal(false, pixels[1][1])
    assert.are_equal(false, pixels[2][1])
    assert.are.same(GREEN, pixels[2][3])
  end)

  it("skips application extensions", function()
    local bytes = builder.header({ width = 1, height = 1, palette = PALETTE })
      .. builder.netscape_loop()
      .. builder.image({ width = 1, height = 1, indices = { 1 } })
      .. builder.TRAILER

    local decoded, err = gif.decode_bytes(bytes)
    assert.is_nil(err)
    assert.are.same(GREEN, decoded.frames[1].pixels[1][1])
  end)

  it("reports per-frame delays in milliseconds", function()
    local bytes = builder.header({ width = 1, height = 1, palette = PALETTE })
      .. builder.graphic_control({ delay_cs = 7 })
      .. builder.image({ width = 1, height = 1, indices = { 0 } })
      .. builder.graphic_control({ delay_cs = 12 })
      .. builder.image({ width = 1, height = 1, indices = { 1 } })
      .. builder.TRAILER

    local decoded = gif.decode_bytes(bytes)
    assert.are_equal(70, decoded.frames[1].delay_ms)
    assert.are_equal(120, decoded.frames[2].delay_ms)
  end)
end)

describe("distract.gif frame composition", function()
  it("keeps the previous frame under a partial update when disposal is `none`", function()
    local bytes = builder.header({ width = 2, height = 1, palette = PALETTE })
      .. builder.graphic_control({ disposal = 1 })
      .. builder.image({ width = 2, height = 1, indices = { 0, 0 } })
      .. builder.graphic_control({ disposal = 1 })
      .. builder.image({ left = 1, top = 0, width = 1, height = 1, indices = { 1 } })
      .. builder.TRAILER

    local decoded = gif.decode_bytes(bytes)
    assert.are.same(RED, decoded.frames[1].pixels[1][1])
    assert.are.same(RED, decoded.frames[1].pixels[1][2])
    assert.are.same(RED, decoded.frames[2].pixels[1][1])
    assert.are.same(GREEN, decoded.frames[2].pixels[1][2])
  end)

  it("clears the disposed rectangle when disposal is `restore to background`", function()
    local bytes = builder.header({ width = 2, height = 1, palette = PALETTE })
      .. builder.graphic_control({ disposal = 2, transparent_index = 3 })
      .. builder.image({ width = 2, height = 1, indices = { 0, 0 } })
      .. builder.graphic_control({ disposal = 1, transparent_index = 3 })
      .. builder.image({ left = 1, top = 0, width = 1, height = 1, indices = { 1 } })
      .. builder.TRAILER

    local decoded = gif.decode_bytes(bytes)
    assert.are.same(RED, decoded.frames[1].pixels[1][1])
    assert.are.same(RED, decoded.frames[1].pixels[1][2])
    -- The first frame disposed to background, so only the second frame's own
    -- pixel is painted.
    assert.are_equal(false, decoded.frames[2].pixels[1][1])
    assert.are.same(GREEN, decoded.frames[2].pixels[1][2])
  end)

  it("restores the frame before last when disposal is `restore to previous`", function()
    local bytes = builder.header({ width = 2, height = 1, palette = PALETTE })
      .. builder.graphic_control({ disposal = 1 })
      .. builder.image({ width = 2, height = 1, indices = { 0, 0 } })
      .. builder.graphic_control({ disposal = 3 })
      .. builder.image({ left = 0, top = 0, width = 1, height = 1, indices = { 1 } })
      .. builder.graphic_control({ disposal = 1 })
      .. builder.image({ left = 1, top = 0, width = 1, height = 1, indices = { 2 } })
      .. builder.TRAILER

    local decoded = gif.decode_bytes(bytes)
    assert.are.same({ GREEN, RED }, decoded.frames[2].pixels[1])
    assert.are.same({ RED, BLUE }, decoded.frames[3].pixels[1])
  end)
end)

describe("distract.gif bounds", function()
  it("refuses a canvas larger than the decoder budget", function()
    local decoded, err = gif.decode_bytes(builder.header({
      width = gif.MAX_CANVAS_DIM + 1,
      height = 1,
      palette = PALETTE,
    }) .. builder.TRAILER)

    assert.is_nil(decoded)
    assert(
      err:match(tostring(gif.MAX_CANVAS_DIM)),
      "expected the cap in the message: " .. tostring(err)
    )
  end)

  it("refuses to materialise a sprite larger than the cell budget", function()
    local decoded, err = gif.decode_bytes(single_frame({
      width = 400,
      height = 400,
      indices = {},
    }))

    assert.is_nil(decoded)
    assert(
      err:match("frame_width"),
      "expected the message to name the manifest field: " .. tostring(err)
    )
  end)

  it("refuses a stream with no image at all", function()
    local decoded, err = gif.decode_bytes(
      builder.header({ width = 1, height = 1, palette = PALETTE }) .. builder.TRAILER
    )
    assert.is_nil(decoded)
    assert(err:match("no frames"), "expected an empty-animation error: " .. tostring(err))
  end)
end)

describe("distract.gif resampling", function()
  it("shrinks a decoded frame to the requested sprite size", function()
    local bytes = single_frame({
      width = 4,
      height = 4,
      indices = {
        0,
        0,
        1,
        1,
        0,
        0,
        1,
        1,
        2,
        2,
        3,
        3,
        2,
        2,
        3,
        3,
      },
    })

    local decoded = gif.decode_bytes(bytes, { target_width = 2, target_height = 2 })
    assert.are_equal(2, decoded.width)
    assert.are_equal(2, decoded.height)
    assert.are.same({ RED, GREEN }, decoded.frames[1].pixels[1])
    assert.are.same({ BLUE, WHITE }, decoded.frames[1].pixels[2])
  end)

  it("keeps a cell transparent when the area it averages is empty", function()
    local bytes = single_frame({
      width = 4,
      height = 2,
      indices = {
        0,
        0,
        1,
        1,
        0,
        0,
        1,
        1,
      },
      control = builder.graphic_control({ transparent_index = 1 }),
    })

    local decoded = gif.decode_bytes(bytes, { target_width = 2, target_height = 1 })
    assert.are.same(RED, decoded.frames[1].pixels[1][1])
    assert.are_equal(false, decoded.frames[1].pixels[1][2])
  end)

  it("grows a frame the same way it shrinks one", function()
    local bytes = single_frame({ width = 2, height = 1, indices = { 0, 1 } })

    local decoded = gif.decode_bytes(bytes, { target_width = 4, target_height = 2 })
    for _, row in ipairs({ 1, 2 }) do
      assert.are.same(RED, decoded.frames[1].pixels[row][1])
      assert.are.same(RED, decoded.frames[1].pixels[row][2])
      assert.are.same(GREEN, decoded.frames[1].pixels[row][3])
      assert.are.same(GREEN, decoded.frames[1].pixels[row][4])
    end
  end)
end)

describe("distract.gif on the reference asset", function()
  local path = vim.fn.getcwd() .. "/assets/cat_walking_1.gif"

  it("decodes the animation the design doc names as the fidelity target", function()
    local decoded, err = gif.decode(path, { target_width = 32, target_height = 32 })

    assert.is_nil(err)
    assert.is_not_nil(decoded)
    assert.are_equal(32, decoded.width)
    assert.are_equal(32, decoded.height)
    assert(#decoded.frames > 1, "expected an animation, got " .. #decoded.frames .. " frame(s)")

    for _, frame in ipairs(decoded.frames) do
      assert.are_equal(32, #frame.pixels)
      assert.are_equal(32, #frame.pixels[1])
      assert(frame.delay_ms > 0, "every frame of the reference asset carries a delay")
    end
  end)

  it("reports a missing file rather than erroring", function()
    local decoded, err = gif.decode("/definitely/not/here.gif")
    assert.is_nil(decoded)
    assert.is_not_nil(err)
  end)
end)
