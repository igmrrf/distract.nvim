--- Reading the `.rgba` sidecar an asset's spritesheet may declare.
---
--- Deliberately not a PNG decoder: the terminal backends are meant to work
--- with zero dependency on the compiled Rust engine, and this repo has no
--- PNG/zlib parser to reuse. The `.rgba` format is a fixed, uncompressed
--- header + raw pixel dump (see `engine/src/bin/import_sprite/rgba_sidecar.rs`
--- for the writer this must stay byte-compatible with) specifically so this
--- reader never needs to be more than byte arithmetic.

local asset_path = require("distract.asset_path")

local M = {}

local HEADER_SIZE = 17
local MAGIC = "DRGB"
local VERSION = 1
local BYTES_PER_PIXEL = 4

local cache = {}

---@class DistractNativeSpriteSource
---@field native_path string as written in the manifest

---@param manifest table|nil
---@return DistractNativeSpriteSource|nil
function M.source_of(manifest)
  local spritesheet = manifest and manifest.spritesheet
  local native_path = spritesheet and spritesheet.native_path
  if not native_path then
    return nil
  end
  return { native_path = native_path }
end

---@param left DistractNativeSpriteSource|nil
---@param right DistractNativeSpriteSource|nil
---@return boolean
function M.same_source(left, right)
  if left == nil or right == nil then
    return left == right
  end
  return left.native_path == right.native_path
end

local function read_u32_le(bytes, offset)
  local first, second, third, fourth = bytes:byte(offset, offset + 3)
  return first + second * 256 + third * 65536 + fourth * 16777216
end

local function decode_frames(bytes, frame_width, frame_height, frame_count)
  local frames = {}
  local cursor = HEADER_SIZE + 1
  for frame_index = 1, frame_count do
    local rows = {}
    for y = 1, frame_height do
      local row = {}
      for x = 1, frame_width do
        local red, green, blue, alpha = bytes:byte(cursor, cursor + 3)
        if alpha == 0 then
          row[x] = false
        else
          row[x] = { red, green, blue }
        end
        cursor = cursor + BYTES_PER_PIXEL
      end
      rows[y] = row
    end
    frames[frame_index] = rows
  end
  return frames
end

local function parse_header(bytes, path)
  if #bytes < HEADER_SIZE then
    return nil, string.format("'%s' is truncated (missing header)", path)
  end
  if bytes:sub(1, 4) ~= MAGIC then
    return nil, string.format("'%s' has bad magic", path)
  end
  local version = bytes:byte(5)
  if version ~= VERSION then
    return nil, string.format("'%s' has unsupported version %d", path, version)
  end

  local frame_width = read_u32_le(bytes, 6)
  local frame_height = read_u32_le(bytes, 10)
  local frame_count = read_u32_le(bytes, 14)
  local expected_size = HEADER_SIZE + frame_count * frame_width * frame_height * BYTES_PER_PIXEL
  if #bytes ~= expected_size then
    return nil,
      string.format(
        "'%s' declares %d bytes of frame data, has %d",
        path,
        expected_size - HEADER_SIZE,
        #bytes - HEADER_SIZE
      )
  end

  return { width = frame_width, height = frame_height, count = frame_count }
end

--- Frame matrices from a `.rgba` sidecar, in the same shape the rest of the
--- render pipeline consumes: `frames[index][y][x]` is `{red, green, blue}`, or
--- `false` where the pixel is fully transparent.
---
--- A missing or malformed file is an expected failure, reported as `nil, err`
--- rather than raised: a bad asset file must not take down the render loop.
---@param path string manifest-relative or absolute
---@return table[]|nil frames
---@return string|nil error_message
function M.load(path)
  if cache[path] then
    return cache[path]
  end

  local resolved = asset_path.resolve(path)
  local file = io.open(resolved, "rb")
  if not file then
    return nil, string.format("cannot open '%s'", resolved)
  end
  local bytes = file:read("*a")
  file:close()

  local header, err = parse_header(bytes, resolved)
  if not header then
    return nil, err
  end

  local frames = decode_frames(bytes, header.width, header.height, header.count)
  cache[path] = frames
  return frames
end

--- Drops every decoded sidecar. For tests, and for an asset being replaced.
function M.reset()
  cache = {}
end

return M
