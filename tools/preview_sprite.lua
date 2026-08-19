-- Prints an asset's frames as text, so a silhouette can be judged without a
-- terminal that draws pictures.
--
-- At 24x16 sprite pixels the silhouette is the only thing that reads, and the
-- half-block renderer's own output cannot be inspected from a headless run. This
-- dumps the pixel grid directly: `#` for opaque, `.` for transparent, and a
-- letter per distinct colour so the tone bands can be counted.
--
--   nvim --headless --noplugin -u tests/minimal_init.lua \
--     -l tools/preview_sprite.lua cat            # every frame
--   nvim --headless --noplugin -u tests/minimal_init.lua \
--     -l tools/preview_sprite.lua cat 0 3 7      # only these frames
--   nvim --headless --noplugin -u tests/minimal_init.lua \
--     -l tools/preview_sprite.lua cat --3d       # the voxel model instead
--   nvim --headless --noplugin -u tests/minimal_init.lua \
--     -l tools/preview_sprite.lua cat --3d=70 0  # turned 70 degrees, frame 0
--
-- `--3d` rasterises the model the 3D mode draws, into the same canvas, so the two
-- forms can be compared side by side. At a yaw of zero they are identical by
-- construction, which is worth seeing rather than trusting.

local frame_source = require("distract.frame_source")
local render = require("distract.render")
local sprites = require("distract.terminal_sprites")

local args = {}
local yaw_degrees = nil
local is_voxel = false
for index = 1, #arg do
  local flag = tostring(arg[index]):match("^%-%-3d=?(.*)$")
  if flag == nil then
    table.insert(args, arg[index])
  else
    is_voxel = true
    yaw_degrees = tonumber(flag) or yaw_degrees
  end
end

local asset = args[1] or "cat"

if is_voxel then
  local settings = { mode = render.VOXEL }
  if yaw_degrees then
    settings.yaw_degrees = yaw_degrees
  end
  frame_source.configure(render.settings(settings))
  local manifests = require("distract").config.assets
  if manifests[asset] then
    frame_source.bind_manifest(asset, manifests[asset])
  end
end

local source_frames = sprites.get_pixel_frames(asset, { native_resolution = false })
if not source_frames or #source_frames == 0 then
  print("no frames for " .. asset)
  vim.cmd("qall!")
end

local frames = {}
for index = 1, #source_frames do
  frames[index] = is_voxel and sprites.pixel_matrix(asset, index, false) or source_frames[index]
end

local wanted = {}
for index = 2, #args do
  wanted[tonumber(args[index])] = true
end

local LETTERS = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"

local function key(pixel)
  return string.format("%d,%d,%d", pixel[1], pixel[2], pixel[3])
end

for frame_index, frame in ipairs(frames) do
  if next(wanted) == nil or wanted[frame_index - 1] then
    local letters = {}
    local order = {}
    for _, row in ipairs(frame) do
      for _, pixel in ipairs(row) do
        if pixel then
          local id = key(pixel)
          if not letters[id] then
            letters[id] = LETTERS:sub(#order + 1, #order + 1)
            table.insert(order, id)
          end
        end
      end
    end

    print(
      string.format(
        "--- %s frame %d (%d colours)%s ---",
        asset,
        frame_index - 1,
        #order,
        is_voxel and string.format(", voxel model turned %d deg", yaw_degrees or 22) or ""
      )
    )
    for _, row in ipairs(frame) do
      local silhouette = {}
      local toned = {}
      for column = 1, #row do
        local pixel = row[column]
        table.insert(silhouette, pixel and "#" or ".")
        table.insert(toned, pixel and letters[key(pixel)] or ".")
      end
      print(table.concat(silhouette) .. "   " .. table.concat(toned))
    end
  end
end

vim.cmd("qall!")
