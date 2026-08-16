--- Builds GIF byte streams for the decoder specs.
---
--- The fixtures are assembled here rather than committed as binaries so a test
--- can state the pixels it expects beside the bytes that encode them. The LZW
--- writer is literal-only -- it emits every index as its own code and never
--- reuses a dictionary entry -- which is valid GIF and keeps the encoder small
--- enough to be obviously correct.

local M = {}

local function u16(value)
  return string.char(value % 256, math.floor(value / 256) % 256)
end

--- Packs a palette into the 3-bytes-per-entry table GIF expects, padded to the
--- power-of-two size the colour-table size field can express.
---@param colours integer[][] list of `{r, g, b}`
---@return string bytes, integer size_field
local function colour_table(colours)
  local size_field = 0
  while 2 ^ (size_field + 1) < #colours do
    size_field = size_field + 1
  end
  local entries = {}
  for index = 1, 2 ^ (size_field + 1) do
    local colour = colours[index] or { 0, 0, 0 }
    entries[#entries + 1] = string.char(colour[1], colour[2], colour[3])
  end
  return table.concat(entries), size_field
end

--- Literal-only LZW, mirroring the dictionary growth a decoder performs.
---
--- The decoder adds one entry per code after the first following a clear, and
--- widens its codes when the next free entry no longer fits, so the writer has
--- to track both to stay in step.
local function lzw_encode(indices, min_code_size)
  local clear_code = 2 ^ min_code_size
  local end_code = clear_code + 1
  local width = min_code_size + 1
  local next_code = end_code + 1
  local out, accumulator, accumulated_bits = {}, 0, 0

  local function emit(code)
    accumulator = accumulator + code * 2 ^ accumulated_bits
    accumulated_bits = accumulated_bits + width
    while accumulated_bits >= 8 do
      out[#out + 1] = string.char(accumulator % 256)
      accumulator = math.floor(accumulator / 256)
      accumulated_bits = accumulated_bits - 8
    end
  end

  emit(clear_code)
  local is_first = true
  for _, index in ipairs(indices) do
    emit(index)
    if is_first then
      is_first = false
    else
      next_code = next_code + 1
      if next_code == 2 ^ width and width < 12 then
        width = width + 1
      end
    end
  end
  emit(end_code)

  if accumulated_bits > 0 then
    out[#out + 1] = string.char(accumulator % 256)
  end
  return table.concat(out)
end

local MAX_SUB_BLOCK_BYTES = 255

local function sub_blocks(payload)
  local out = {}
  local offset = 1
  while offset <= #payload do
    local chunk = payload:sub(offset, offset + MAX_SUB_BLOCK_BYTES - 1)
    out[#out + 1] = string.char(#chunk) .. chunk
    offset = offset + #chunk
  end
  out[#out + 1] = "\0"
  return table.concat(out)
end

--- Header plus logical screen descriptor, with an optional global palette.
---@param opts table `{ width, height, palette, background = index, version }`
function M.header(opts)
  local packed = 0
  local table_bytes = ""
  if opts.palette then
    local bytes, size_field = colour_table(opts.palette)
    table_bytes = bytes
    packed = 0x80 + size_field
  end
  return "GIF"
    .. (opts.version or "89a")
    .. u16(opts.width)
    .. u16(opts.height)
    .. string.char(packed)
    .. string.char(opts.background or 0)
    .. "\0"
    .. table_bytes
end

--- Graphic control extension: per-frame delay, disposal and transparency.
---@param opts table `{ delay_cs, transparent_index, disposal }`
function M.graphic_control(opts)
  local transparent_index = opts.transparent_index
  local packed = (opts.disposal or 0) * 4 + (transparent_index and 1 or 0)
  return "\x21\xF9\x04"
    .. string.char(packed)
    .. u16(opts.delay_cs or 0)
    .. string.char(transparent_index or 0)
    .. "\0"
end

--- Image descriptor plus its LZW-compressed indices.
---@param opts table `{ left, top, width, height, indices, palette, interlace, min_code_size }`
function M.image(opts)
  local packed = 0
  local table_bytes = ""
  if opts.palette then
    local bytes, size_field = colour_table(opts.palette)
    table_bytes = bytes
    packed = packed + 0x80 + size_field
  end
  if opts.interlace then
    packed = packed + 0x40
  end

  local min_code_size = opts.min_code_size or 2
  return "\x2C"
    .. u16(opts.left or 0)
    .. u16(opts.top or 0)
    .. u16(opts.width)
    .. u16(opts.height)
    .. string.char(packed)
    .. table_bytes
    .. string.char(min_code_size)
    .. sub_blocks(lzw_encode(opts.indices, min_code_size))
end

--- An application extension, which a decoder must skip without complaint.
function M.netscape_loop()
  return "\x21\xFF\x0BNETSCAPE2.0\x03\x01\x00\x00\x00"
end

M.TRAILER = "\x3B"

return M
