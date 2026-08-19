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

local raster3d = require("distract.raster3d")
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

--- The same request the half-block backend makes, used here only for its size.
---
--- Kitty draws real pixels but must occupy the cell footprint every other
--- consumer agrees on, and the fitted frames are footprint-sized by
--- construction. For an asset with no sidecar both requests return the same
--- frames, so this costs nothing and changes nothing.
local FOOTPRINT_CAPABILITY = { native_resolution = false }

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
--- The pixels a frame is transmitted from, and the grid its footprint is measured
--- on.
---
--- The image and the box it fills are sized separately. `c`/`r` tell the terminal
--- how many cells to resample the transmitted pixels into, so fidelity and
--- footprint are independent -- and the footprint has to be the one
--- `sprites.get_dimensions` reports, or kitty draws a 128-column cat while the
--- engine wraps and anchors a 32-column one.
---
--- A voxel model is one grid rather than two: its fidelity is bounded by the voxel
--- grid, not by the source image, so there is no native-resolution form of it to
--- transmit. Raise `render.voxel_max_width` for a denser model.
local function source_matrices(asset_name, frame_idx, flip_x)
  if sprites.is_voxel(asset_name) then
    local model = raster3d.matrix(asset_name, frame_idx, flip_x)
    return model, model
  end

  local pixel_frames = sprites.get_pixel_frames(asset_name, KITTY_CAPABILITY)
  local footprint_frames = sprites.get_pixel_frames(asset_name, FOOTPRINT_CAPABILITY)
  local matrix = pixel_frames[frame_idx] or pixel_frames[1]
  local footprint = footprint_frames[frame_idx] or footprint_frames[1]
  if not matrix or not footprint then
    return nil, nil
  end
  if flip_x then
    return sprites.mirror_matrix(matrix), sprites.mirror_matrix(footprint)
  end
  return matrix, footprint
end

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

  local matrix, footprint = source_matrices(asset_name, frame_idx, flip_x)
  if not matrix or not footprint then
    return nil
  end

  local width = matrix_width(matrix)
  local cols = matrix_width(footprint)
  local cell_rows = math.ceil(#footprint / PIXEL_ROWS_PER_CELL)
  if width < 1 or cols < 1 or cell_rows < 1 then
    return nil
  end
  -- The payload keeps whole cells of its own pixels; `spans` never indexes it.
  local height = math.ceil(#matrix / PIXEL_ROWS_PER_CELL) * PIXEL_ROWS_PER_CELL

  entry = {
    key = frame_idx .. ":" .. facing,
    pixel_w = width,
    pixel_h = height,
    cols = cols,
    rows = cell_rows,
    rgba = encode_rgba(matrix, width, height),
    -- Built on the footprint grid because `spans` resamples it from
    -- `frame.cols` x `frame.rows`; a mask on the image's grid would be indexed
    -- with the footprint's dimensions and tear.
    mask = opacity_mask(footprint, cols, cell_rows),
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

-- A settings change repaints every frame, and these are described once and cached
-- for the process lifetime otherwise.
sprites.on_render_change(function()
  M.reset()
end)

return M
