--- Walks the block structure of a GIF stream.
---
--- This module reads bytes and hands back palette indices; it never resolves a
--- colour or composites a frame. Composition is `distract.gif`'s job, because
--- disposal methods relate frames to each other and nothing here has more than
--- one image in view at a time.

local lzw = require("distract.gif.lzw")

local M = {}

local SIGNATURE = "GIF"
local SUPPORTED_VERSIONS = { ["87a"] = true, ["89a"] = true }

local EXTENSION_INTRODUCER = 0x21
local IMAGE_SEPARATOR = 0x2C
local TRAILER = 0x3B
local GRAPHIC_CONTROL_LABEL = 0xF9

local GLOBAL_COLOUR_TABLE_FLAG = 0x80
local LOCAL_COLOUR_TABLE_FLAG = 0x80
local INTERLACE_FLAG = 0x40
local COLOUR_TABLE_SIZE_MASK = 0x07

local TRANSPARENCY_FLAG = 0x01
--- Disposal occupies bits 2-4 of the graphic control extension's packed field.
local DISPOSAL_SHIFT = 2
local DISPOSAL_MASK = 0x07

local CENTISECONDS_TO_MS = 10

--- Rows of an interlaced image arrive in four passes, each with its own first
--- row and stride.
local INTERLACE_PASSES = {
  { start_row = 0, step = 8 },
  { start_row = 4, step = 8 },
  { start_row = 2, step = 4 },
  { start_row = 1, step = 2 },
}

local function read_u16(bytes, offset)
  local low, high = bytes:byte(offset, offset + 1)
  if not high then
    return nil
  end
  return low + high * 256, offset + 2
end

local function read_colour_table(bytes, offset, entry_count)
  local palette = {}
  local last = offset + entry_count * 3 - 1
  if last > #bytes then
    return nil, "GIF colour table is truncated"
  end
  for index = 1, entry_count do
    local base = offset + (index - 1) * 3
    palette[index] = { bytes:byte(base, base + 2) }
  end
  return palette, last + 1
end

--- The logical screen descriptor: canvas size and the global palette.
---@param bytes string
---@return table|nil screen, string|nil error_message
function M.read_header(bytes)
  if #bytes < 13 or bytes:sub(1, 3) ~= SIGNATURE then
    return nil, "not a GIF: the stream does not start with a GIF signature"
  end
  local version = bytes:sub(4, 6)
  if not SUPPORTED_VERSIONS[version] then
    return nil, string.format("unsupported GIF version '%s'", version)
  end

  local width, offset = read_u16(bytes, 7)
  local height
  height, offset = read_u16(bytes, offset)
  local packed = bytes:byte(offset)
  local background = bytes:byte(offset + 1)
  offset = offset + 3

  if not width or not height or not packed or not background then
    return nil, "GIF header is truncated"
  end

  local palette = nil
  if packed >= GLOBAL_COLOUR_TABLE_FLAG then
    local entry_count = 2 ^ ((packed % 8) + 1)
    local err
    palette, err = read_colour_table(bytes, offset, entry_count)
    if not palette then
      return nil, err
    end
    offset = offset + entry_count * 3
  end

  return {
    width = width,
    height = height,
    palette = palette,
    background = background,
    offset = offset,
  }
end

--- Concatenated sub-block payload, and the offset just past the terminator.
local function read_sub_blocks(bytes, offset)
  local chunks = {}
  while true do
    local size = bytes:byte(offset)
    if not size then
      return nil, "GIF data block is truncated"
    end
    offset = offset + 1
    if size == 0 then
      return table.concat(chunks), offset
    end
    local chunk = bytes:sub(offset, offset + size - 1)
    if #chunk < size then
      return nil, "GIF data block is truncated"
    end
    chunks[#chunks + 1] = chunk
    offset = offset + size
  end
end

local function read_graphic_control(bytes, offset)
  local block_size = bytes:byte(offset)
  local packed = bytes:byte(offset + 1)
  local delay_centiseconds = read_u16(bytes, offset + 2)
  local transparent_index = bytes:byte(offset + 4)
  if not block_size or block_size < 4 or not packed or not delay_centiseconds then
    return nil, "GIF graphic control extension is truncated"
  end

  local control = {
    disposal = math.floor(packed / (2 ^ DISPOSAL_SHIFT)) % (DISPOSAL_MASK + 1),
    delay_ms = delay_centiseconds * CENTISECONDS_TO_MS,
    transparent_index = (packed % 2 == TRANSPARENCY_FLAG) and transparent_index or nil,
  }

  local payload, payload_end = read_sub_blocks(bytes, offset + block_size + 1)
  if not payload then
    return nil, payload_end
  end
  return control, payload_end
end

--- Puts an interlaced image's rows back into screen order.
local function deinterlace(indices, width, height)
  local ordered = {}
  local source_row = 0
  for _, pass in ipairs(INTERLACE_PASSES) do
    local row = pass.start_row
    while row < height do
      local source_base = source_row * width
      local target_base = row * width
      for column = 1, width do
        ordered[target_base + column] = indices[source_base + column]
      end
      source_row = source_row + 1
      row = row + pass.step
    end
  end
  return ordered
end

--- An image descriptor is nine fixed bytes; reading them one at a time from a
--- possibly truncated stream is what the length check here replaces.
local IMAGE_DESCRIPTOR_BYTES = 9

local function read_image(bytes, offset, screen)
  if offset + IMAGE_DESCRIPTOR_BYTES - 1 > #bytes then
    return nil, "GIF image descriptor is truncated"
  end

  local left, next_offset = read_u16(bytes, offset)
  local top, width, height
  top, next_offset = read_u16(bytes, next_offset)
  width, next_offset = read_u16(bytes, next_offset)
  height, next_offset = read_u16(bytes, next_offset)
  local packed = bytes:byte(next_offset)
  next_offset = next_offset + 1

  local palette = screen.palette
  if packed >= LOCAL_COLOUR_TABLE_FLAG then
    local entry_count = 2 ^ ((packed % (COLOUR_TABLE_SIZE_MASK + 1)) + 1)
    local err
    palette, err = read_colour_table(bytes, next_offset, entry_count)
    if not palette then
      return nil, err
    end
    next_offset = next_offset + entry_count * 3
  end
  if not palette then
    return nil, "GIF image has neither a local nor a global colour table"
  end

  local min_code_size = bytes:byte(next_offset)
  if not min_code_size then
    return nil, "GIF image data is truncated"
  end
  local payload, payload_end = read_sub_blocks(bytes, next_offset + 1)
  if not payload then
    return nil, payload_end
  end
  next_offset = payload_end

  local indices, lzw_err = lzw.decode(payload, min_code_size, width * height)
  if not indices then
    return nil, lzw_err
  end
  if packed % (INTERLACE_FLAG * 2) >= INTERLACE_FLAG then
    indices = deinterlace(indices, width, height)
  end

  return {
    left = left,
    top = top,
    width = width,
    height = height,
    palette = palette,
    indices = indices,
  },
    next_offset
end

--- Every image in the stream, in the order they are drawn.
---
--- The graphic control extension that precedes an image belongs to it, so its
--- delay, disposal method and transparent index are folded onto the image
--- rather than left as a separate block for the caller to re-associate.
---@param bytes string
---@param screen table from `read_header`
---@param opts table `{ max_frames = integer, on_frame = fun(count: integer)|nil }`
---@return table[]|nil images, string|nil error_message
function M.read_images(bytes, screen, opts)
  local max_frames = opts.max_frames
  local on_frame = opts.on_frame
  local images = {}
  local offset = screen.offset
  local control = nil

  while offset <= #bytes do
    local marker = bytes:byte(offset)
    if marker == TRAILER then
      break
    elseif marker == EXTENSION_INTRODUCER then
      local label = bytes:byte(offset + 1)
      if label == GRAPHIC_CONTROL_LABEL then
        local parsed, next_offset = read_graphic_control(bytes, offset + 2)
        if not parsed then
          return nil, next_offset
        end
        control, offset = parsed, next_offset
      else
        local _, next_offset = read_sub_blocks(bytes, offset + 2)
        if not next_offset then
          return nil, "GIF extension block is truncated"
        end
        offset = next_offset
      end
    elseif marker == IMAGE_SEPARATOR then
      local image, next_offset = read_image(bytes, offset + 1, screen)
      if not image then
        return nil, next_offset
      end
      image.disposal = control and control.disposal or 0
      image.delay_ms = control and control.delay_ms or 0
      image.transparent_index = control and control.transparent_index or nil
      images[#images + 1] = image
      control = nil
      offset = next_offset
      if #images > max_frames then
        return nil, string.format("GIF has more than %d frames", max_frames)
      end
      if on_frame then
        on_frame(#images)
      end
    else
      return nil, string.format("unexpected GIF block marker 0x%02X", marker)
    end
  end

  return images
end

return M
