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

local sprites = require("distract.terminal_sprites")

local asset = (vim.v.argv and nil) or nil
local args = {}
for index = 1, #arg do
  table.insert(args, arg[index])
end
asset = args[1] or "cat"

local frames = sprites.get_pixel_frames(asset, { native_resolution = false })
if not frames or #frames == 0 then
  print("no frames for " .. asset)
  vim.cmd("qall!")
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

    print(string.format("--- %s frame %d (%d colours) ---", asset, frame_index - 1, #order))
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
