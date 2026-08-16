--- Turning a decoded GIF canvas into a sprite-sized pixel matrix.
---
--- A GIF authored for a screen is far larger than the 24x16-ish canvas a
--- terminal sprite occupies, and the unit contract fixes that footprint: one
--- sprite pixel is one cell wide and half a cell tall on every backend. So the
--- canvas is resampled here rather than drawn at its own size.
---
--- The filter is an area average, which is what keeps a shrunk sprite from
--- flickering: nearest-neighbour picks one source pixel per cell, so a frame
--- that moves by a pixel changes colour discontinuously.

local M = {}

--- Alpha is per source pixel, not per average: a cell over the edge of a sprite
--- covers both drawn and undrawn pixels, and averaging the drawn ones alone
--- keeps the edge colour true instead of dragging it towards black.
local OPAQUE_COVERAGE_THRESHOLD = 0.5

local function source_range(target_index, target_size, source_size)
  local from = math.floor(target_index * source_size / target_size)
  local to = math.floor((target_index + 1) * source_size / target_size) - 1
  if to < from then
    to = from
  end
  return from, math.min(to, source_size - 1)
end

--- Area-averages `canvas` down (or up) to `target_width` x `target_height`.
---@param canvas table `{ red, green, blue, opaque }`, flat and 1-based
---@param source table `{ width, height }`
---@param target table `{ width, height }`
---@return table<integer, table<integer, integer[]|false>> rows 1-based `[row][col]`
function M.to_matrix(canvas, source, target)
  local rows = {}

  for target_row = 0, target.height - 1 do
    local first_row, last_row = source_range(target_row, target.height, source.height)
    local row = {}

    for target_col = 0, target.width - 1 do
      local first_col, last_col = source_range(target_col, target.width, source.width)
      local red, green, blue, covered, total = 0, 0, 0, 0, 0

      for source_row = first_row, last_row do
        local base = source_row * source.width
        for source_col = first_col, last_col do
          local index = base + source_col + 1
          total = total + 1
          if canvas.opaque[index] then
            covered = covered + 1
            red = red + canvas.red[index]
            green = green + canvas.green[index]
            blue = blue + canvas.blue[index]
          end
        end
      end

      if total > 0 and covered / total >= OPAQUE_COVERAGE_THRESHOLD then
        row[target_col + 1] = {
          math.floor(red / covered + 0.5),
          math.floor(green / covered + 0.5),
          math.floor(blue / covered + 0.5),
        }
      else
        row[target_col + 1] = false
      end
    end

    rows[target_row + 1] = row
  end

  return rows
end

return M
