--- Turning a sprite's pixel matrix into what the graphics protocol wants.
---
--- Two things come out of one pass over the matrix: the raw RGBA the terminal
--- is sent, and which cells have anything in them at all.
---
--- The footprint is deliberately the same as the half-block renderer's -- a
--- W x H sprite occupies W columns and H/2 rows either way. Fidelity comes from
--- pixel density inside that rectangle, not from a larger one, so positions,
--- `ground_y` and the unit contract are untouched by which backend draws.

local M = {}

local sprites = require("distract.terminal_sprites")

--- Sprite pixel rows stacked into one terminal cell, as the half-block glyphs
--- do. Any other value would make a kitty sprite a different size from the same
--- sprite drawn in half-blocks.
local PIXEL_ROWS_PER_CELL = 2

local TRANSPARENT_PIXEL = "\0\0\0\0"

--- The graphics protocol transmits real pixels, so this backend asks for an
--- asset's native-resolution art where its manifest declares one. Passed as a
--- literal rather than looked up through `distract.backends`: this module is the
--- kitty backend's own internals, and `kitty/init.lua` already requires it, so
--- reading the registry back would be a circular require.
---@type table
local KITTY_CAPABILITY = { native_resolution = true }

--- One frame of one asset, ready to transmit and to place.
---@class DistractKittyFrame
---@field key string identifies the frame and its facing, not its placement size
---@field pixel_w integer
---@field pixel_h integer padded to a whole number of cells
---@field cols integer
---@field rows integer
---@field rgba string `pixel_w * pixel_h * 4` raw bytes
---@field mask table<integer, table<integer, boolean>> 0-based `[cell_row][cell_col]`

local function matrix_width(pixel_rows)
  local width = 0
  for _, row in ipairs(pixel_rows) do
    if #row > width then
      width = #row
    end
  end
  return width
end

--- Which cells of the unscaled frame have at least one pixel in them.
---
--- Transparent cells are left out of the drawn output rather than covered by
--- empty placeholders. On the buffer-overlay path a placeholder cell replaces
--- the editor text under it, so covering the sprite's whole bounding box would
--- blank exactly the code the per-pixel alpha exists to preserve. Each
--- placeholder carries its own row and column, so a gap costs nothing: the
--- terminal still draws every remaining cell in the right place.
local function opacity_mask(pixel_rows, width, cell_rows)
  local mask = {}
  for cell_row = 0, cell_rows - 1 do
    local top = pixel_rows[cell_row * PIXEL_ROWS_PER_CELL + 1] or {}
    local bottom = pixel_rows[cell_row * PIXEL_ROWS_PER_CELL + 2] or {}
    local row = {}
    for col = 0, width - 1 do
      row[col] = (top[col + 1] or bottom[col + 1]) and true or false
    end
    mask[cell_row] = row
  end
  return mask
end

--- Raw RGBA for the whole padded canvas.
---
--- Padding to a whole number of cells matters: the terminal scales the image to
--- fill `c` x `r` cells, so an odd-height sprite sent unpadded would be
--- stretched by half a cell and stop lining up with the same sprite in
--- half-blocks.
local function encode_rgba(pixel_rows, width, height)
  local out = {}
  for row = 1, height do
    local pixels = pixel_rows[row]
    for col = 1, width do
      local colour = pixels and pixels[col]
      if colour then
        out[#out + 1] = string.char(colour[1], colour[2], colour[3], 255)
      else
        out[#out + 1] = TRANSPARENT_PIXEL
      end
    end
  end
  return table.concat(out)
end

local cache = {}

--- Builds, once, everything the kitty backend needs for one frame.
---
--- Keyed on `(asset, frame, flip_x)` -- the same key the half-block render and
--- frame-buffer caches use, and invalidated by the same `reset` call, so the
--- two backends cannot end up disagreeing about what frame 3 looks like.
---@param asset_name string
---@param frame_idx integer 1-based index into the asset's pixel frames
---@param flip_x boolean
---@return DistractKittyFrame|nil
function M.describe(asset_name, frame_idx, flip_x)
  local facing = flip_x and "flipped" or "facing"
  local by_asset = cache[asset_name]
  if not by_asset then
    by_asset = { facing = {}, flipped = {} }
    cache[asset_name] = by_asset
  end

  local entry = by_asset[facing][frame_idx]
  if entry then
    return entry
  end

  local pixel_frames = sprites.get_pixel_frames(asset_name, KITTY_CAPABILITY)
  local matrix = pixel_frames[frame_idx] or pixel_frames[1]
  if not matrix then
    return nil
  end
  if flip_x then
    matrix = sprites.mirror_matrix(matrix)
  end

  local width = matrix_width(matrix)
  local cell_rows = math.ceil(#matrix / PIXEL_ROWS_PER_CELL)
  if width < 1 or cell_rows < 1 then
    return nil
  end
  local height = cell_rows * PIXEL_ROWS_PER_CELL

  entry = {
    key = frame_idx .. ":" .. facing,
    pixel_w = width,
    pixel_h = height,
    cols = width,
    rows = cell_rows,
    rgba = encode_rgba(matrix, width, height),
    mask = opacity_mask(matrix, width, cell_rows),
  }
  by_asset[facing][frame_idx] = entry
  return entry
end

--- Runs of drawn cells on a placement grid of `cols` x `rows` cells.
---
--- A parallaxed sprite occupies fewer or more cells than it was authored with,
--- and the terminal resamples the image to fill them. The mask has to be
--- resampled the same way or a shrunk sprite would leave placeholder cells over
--- parts of the image that are now empty, and a grown one would clip its own
--- edges.
---@param frame DistractKittyFrame
---@param cols integer
---@param rows integer
---@return table<integer, integer[][]> 0-based cell row -> `{from, to}` column pairs
function M.spans(frame, cols, rows)
  local spans = {}
  for row = 0, rows - 1 do
    local source = frame.mask[math.floor(row * frame.rows / rows)] or {}
    local row_spans = {}
    local start = nil

    for col = 0, cols - 1 do
      local filled = source[math.floor(col * frame.cols / cols)]
      if filled and not start then
        start = col
      elseif not filled and start then
        row_spans[#row_spans + 1] = { start, col - 1 }
        start = nil
      end
    end
    if start then
      row_spans[#row_spans + 1] = { start, cols - 1 }
    end

    spans[row] = row_spans
  end
  return spans
end

--- Forgets the described frames, for one asset or for all of them.
---@param asset_name string|nil
function M.reset(asset_name)
  if asset_name then
    cache[asset_name] = nil
  else
    cache = {}
  end
end

return M
