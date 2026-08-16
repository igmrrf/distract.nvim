--- The variable-width LZW decompressor GIF image data is stored in.
---
--- The dictionary is held as parallel `prefix`/`suffix`/`first` arrays rather
--- than as strings: an entry is a previous entry plus one byte, so walking the
--- chain costs a loop and no allocation, while `entry = entry .. char` would
--- copy the whole expanded string on every one of the millions of entries a
--- full-frame image produces.

local bit = require("bit")

local M = {}

--- The protocol's own ceiling: codes are at most 12 bits wide.
local MAX_CODES = 4096
local MAX_CODE_WIDTH = 12

local MIN_CODE_SIZE_FLOOR = 2
local MIN_CODE_SIZE_CEILING = 11

--- Expands one dictionary entry onto the output, innermost byte first.
---
--- Returns the first byte of the expanded string, which is what the next
--- dictionary entry is built from.
local function expand(state, code, out)
  local stack = state.stack
  local depth = 0
  local current = code

  while state.prefix[current] ~= -1 do
    depth = depth + 1
    stack[depth] = state.suffix[current]
    current = state.prefix[current]
    if depth > MAX_CODES then
      return nil
    end
  end
  depth = depth + 1
  stack[depth] = state.suffix[current]

  local count = #out
  for index = depth, 1, -1 do
    count = count + 1
    out[count] = stack[index]
  end
  return stack[depth]
end

--- Rebuilds the dictionary down to its roots.
---
--- Entries defined since the last clear are wiped rather than left to be
--- overwritten: a code above the new `next_code` still answering `suffix[code]`
--- would be decoded as a stale entry from the previous generation, which is
--- how a mid-stream clear used to silently corrupt every frame after it.
local function reset_dictionary(state, clear_code, end_code)
  for code = end_code + 1, state.next_code - 1 do
    state.prefix[code] = nil
    state.suffix[code] = nil
    state.first[code] = nil
  end

  state.next_code = end_code + 1
  state.width = state.min_code_size + 1
  for code = 0, clear_code - 1 do
    state.prefix[code] = -1
    state.suffix[code] = code
    state.first[code] = code
  end
end

local function add_entry(state, previous, first_byte)
  if state.next_code >= MAX_CODES then
    return
  end
  local code = state.next_code
  state.prefix[code] = previous
  state.suffix[code] = first_byte
  state.first[code] = state.first[previous]
  state.next_code = code + 1
  if state.next_code == bit.lshift(1, state.width) and state.width < MAX_CODE_WIDTH then
    state.width = state.width + 1
  end
end

--- Pulls variable-width codes out of the byte stream, least significant bit
--- first. Codes straddle byte boundaries, so the reader owns its own bit
--- accumulator rather than re-deriving one per code.
local function new_reader(data)
  return { data = data, length = #data, offset = 1, accumulator = 0, bits = 0 }
end

local function next_code(reader, width)
  while reader.bits < width do
    if reader.offset > reader.length then
      return nil
    end
    reader.accumulator =
      bit.bor(reader.accumulator, bit.lshift(reader.data:byte(reader.offset), reader.bits))
    reader.bits = reader.bits + 8
    reader.offset = reader.offset + 1
  end

  local code = bit.band(reader.accumulator, bit.lshift(1, width) - 1)
  reader.accumulator = bit.rshift(reader.accumulator, width)
  reader.bits = reader.bits - width
  return code
end

local CYCLIC_ENTRY = "GIF image data contains a cyclic LZW dictionary entry"

--- Handles one code. Returns the code to remember as `previous`, or nil plus a
--- message when the stream is corrupt, or `false` when the stream ends.
local function consume(state, code, previous, out)
  if code == state.end_code then
    return false
  end
  if code == state.clear_code then
    reset_dictionary(state, state.clear_code, state.end_code)
    return nil
  end

  if code < state.next_code and state.suffix[code] ~= nil then
    local first_byte = expand(state, code, out)
    if not first_byte then
      return nil, CYCLIC_ENTRY
    end
    if previous then
      add_entry(state, previous, first_byte)
    end
    return code
  end

  -- The encoder may reference the entry it is about to define, when a sequence
  -- repeats immediately. Defining it first is what makes that legal rather
  -- than corrupt.
  if code == state.next_code and previous then
    add_entry(state, previous, state.first[previous])
    if not expand(state, code, out) then
      return nil, CYCLIC_ENTRY
    end
    return code
  end

  return nil, string.format("GIF image data contains an out-of-range code %d", code)
end

local function new_state(min_code_size)
  local clear_code = bit.lshift(1, min_code_size)
  local end_code = clear_code + 1
  local state = {
    min_code_size = min_code_size,
    clear_code = clear_code,
    end_code = end_code,
    prefix = {},
    suffix = {},
    first = {},
    stack = {},
    width = min_code_size + 1,
    next_code = end_code + 1,
  }
  reset_dictionary(state, clear_code, end_code)
  return state
end

--- Decompresses one image's pixel indices.
---
--- `pixel_count` is what the image descriptor declares; decoding stops there
--- even when the stream carries more, which real encoders do produce as
--- padding. Falling short of it is an error rather than a short frame: a
--- partially decoded image is corrupt data, not a smaller picture.
---@param data string the image's concatenated sub-block payload
---@param min_code_size integer
---@param pixel_count integer
---@return integer[]|nil indices, string|nil error_message
function M.decode(data, min_code_size, pixel_count)
  if min_code_size < MIN_CODE_SIZE_FLOOR or min_code_size > MIN_CODE_SIZE_CEILING then
    return nil, string.format("LZW minimum code size %d is out of range", min_code_size)
  end

  local state = new_state(min_code_size)
  local reader = new_reader(data)
  local out = {}
  local previous = nil

  while #out < pixel_count do
    local code = next_code(reader, state.width)
    if not code then
      return nil, "GIF image data ended before the frame was complete"
    end

    local resumed, err = consume(state, code, previous, out)
    if err then
      return nil, err
    end
    if resumed == false then
      break
    end
    previous = resumed
  end

  if #out < pixel_count then
    return nil, string.format("GIF image declares %d pixels but decoded %d", pixel_count, #out)
  end

  return out
end

return M
