--- Which pixels one frame of one asset is drawn from.
---
--- Every backend asks the same question and has to get the same answer: a flat
--- asset's frame comes off its sheet, and a voxel asset's is rasterised from the
--- model `distract.voxel` extrudes. Held here rather than in `terminal_sprites`
--- because it is a responsibility of its own -- the render mode, which assets
--- pinned themselves to one, and the caches a mode change invalidates -- and
--- because both the half-block renderer and the kitty describer need it.

local M = {}

local quantise = require("distract.quantise")
local raster3d = require("distract.raster3d")
local render = require("distract.render")
local sources = require("distract.sprite_sources")

--- Frames drawn in half-blocks are one sprite pixel per canvas cell, rather than
--- a manifest's native-resolution sidecar.
local CELL_GRID = { native_resolution = false }

local settings = render.DEFAULTS
--- Asset name -> the mode its manifest pinned, if it pinned one.
local declared_modes = {}
--- Called when the settings change, for backends holding a frame cache of their
--- own. The kitty describer cannot be reached from `terminal_sprites` without a
--- circular require, so it subscribes rather than being called.
local listeners = {}

---@param callback fun(asset_name: string|nil)
function M.on_change(callback)
  table.insert(listeners, callback)
end

local function announce(asset_name)
  for _, listener in ipairs(listeners) do
    listener(asset_name)
  end
end

--- Applies the render settings frames are drawn under.
---
--- Every cached frame is dropped: a mode, yaw or light change repaints all of them.
---@param new_settings table validated `render` settings
function M.configure(new_settings)
  settings = new_settings or render.DEFAULTS
  raster3d.configure(settings)
  announce(nil)
end

--- The render settings in force.
---@return table
function M.settings()
  return settings
end

--- Whether this asset is drawn as a voxel model.
---@param asset_name string
---@return boolean
function M.is_voxel(asset_name)
  return render.is_voxel(settings, {
    name = asset_name,
    render = declared_modes[asset_name],
  })
end

--- Warms up 3D voxel poses in background slices.
---@param asset_name string
function M.warm_voxel_asset(asset_name)
  if not M.is_voxel(asset_name) then
    return
  end
  require("distract.warmup").request("voxel:" .. asset_name, function()
    local frames = sources.get_pixel_frames(asset_name, CELL_GRID)
    if not frames then
      return
    end
    for frame_index = 1, #frames do
      for _, flip_x in ipairs({ false, true }) do
        raster3d.matrix(asset_name, frame_index, flip_x)
        coroutine.yield()
      end
    end
  end)
end

--- Records an asset's declared art, and the render mode its manifest pins.
---@param asset_name string
---@param manifest table|nil
function M.bind_manifest(asset_name, manifest)
  local declared = manifest and manifest.render or nil
  if declared ~= declared_modes[asset_name] then
    declared_modes[asset_name] = declared
    announce(asset_name)
  end
  sources.bind_manifest(asset_name, manifest)
  if M.is_voxel(asset_name) then
    M.warm_voxel_asset(asset_name)
  end
end

--- The pixels one frame of an asset is drawn from, ready to render.
---
--- A voxel asset takes its facing as a yaw rather than a mirror, matching the
--- overlay: mirroring a model would swap which side the light falls on, so a pet
--- turning round would appear to move the sun. Quantising is unconditional there,
--- because shading multiplies every source colour by one factor per face
--- orientation and the highlight-group count would grow by the same multiple.
---@param asset_name string
---@param frame_idx integer 1-based
---@param request table `{ flip_x = boolean, max_colours = integer }`
---@return table[]|nil
function M.matrix(asset_name, frame_idx, request)
  if M.is_voxel(asset_name) then
    local model = raster3d.matrix(asset_name, frame_idx, request.flip_x)
    if not model then
      return nil
    end
    return quantise.reduce(model, request.max_colours)
  end

  local frames = sources.get_pixel_frames(asset_name, CELL_GRID)
  local matrix = frames and (frames[frame_idx] or frames[1])
  if not matrix then
    return nil
  end
  if request.flip_x then
    matrix = M.mirror_matrix(matrix)
  end
  if sources.needs_quantising(asset_name) then
    return quantise.reduce(matrix, request.max_colours)
  end
  return matrix
end

--- Widest row in the matrix.
---
--- Rows are expected to be uniform, but a custom matrix may be ragged; padding to
--- the maximum keeps every rendered line rectangular so a float window's width
--- stays correct.
---@param pixel_rows table[]
---@return integer
function M.matrix_width(pixel_rows)
  local width = 0
  for _, row in ipairs(pixel_rows) do
    if #row > width then
      width = #row
    end
  end
  return width
end

--- Mirrors a pixel matrix horizontally.
---@param pixel_rows table[]
---@return table[]
function M.mirror_matrix(pixel_rows)
  local width = M.matrix_width(pixel_rows)
  local mirrored = {}
  for row_idx = 1, #pixel_rows do
    local row = pixel_rows[row_idx]
    local flipped = {}
    for col = 1, width do
      flipped[col] = row[width - col + 1] or false
    end
    mirrored[row_idx] = flipped
  end
  return mirrored
end

return M
