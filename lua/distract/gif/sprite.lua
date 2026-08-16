--- Turning a manifest's GIF into the sprite set the terminal backends draw.
---
--- The result is the same shape `distract.sprites.*` return -- frames, width,
--- height -- plus the per-frame delays the file carries, so a GIF asset needs
--- no branch anywhere downstream. Decoding is deferred to the first draw: a
--- manifest may be registered in a config that never spawns anything.

local asset_path = require("distract.asset_path")
local gif = require("distract.gif")

local M = {}

---@class DistractGifSource
---@field path string as written in the manifest
---@field width integer|nil declared sprite width, in sprite pixels
---@field height integer|nil declared sprite height, in sprite pixels

--- The GIF an asset's manifest points at, or nil when it points at something
--- else. A spritesheet in any other format belongs to the overlay, which has a
--- real image decoder; refusing it here is what keeps a PNG asset from silently
--- rendering as the fallback cat in the terminal.
---@param manifest table|nil
---@return DistractGifSource|nil
function M.source_of(manifest)
  local spritesheet = manifest and manifest.spritesheet
  local path = spritesheet and spritesheet.path
  if not asset_path.is_gif(path) then
    return nil
  end

  return {
    path = path,
    width = spritesheet.frame_width,
    height = spritesheet.frame_height,
  }
end

--- Whether two sources describe the same art.
---@param left DistractGifSource|nil
---@param right DistractGifSource|nil
---@return boolean
function M.same_source(left, right)
  if left == nil or right == nil then
    return left == right
  end
  return left.path == right.path and left.width == right.width and left.height == right.height
end

--- Decodes a source into a drawable sprite set.
---@param source DistractGifSource
---@return table|nil sprite, string|nil error_message
function M.build(source)
  local decoded, err = gif.decode(asset_path.resolve(source.path), {
    target_width = source.width,
    target_height = source.height,
  })
  if not decoded then
    return nil, err
  end

  local frames, delays_ms = {}, {}
  for index, frame in ipairs(decoded.frames) do
    frames[index] = frame.pixels
    delays_ms[index] = frame.delay_ms
  end

  return {
    frames = frames,
    delays_ms = delays_ms,
    width = decoded.width,
    height = decoded.height,
    -- Imported art arrives with a full 24-bit palette, which the half-block
    -- renderer would turn into a highlight group per colour pair. Procedural
    -- art is already drawn from a small palette and is left alone.
    quantise = true,
  }
end

return M
