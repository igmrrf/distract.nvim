--- Area-averaging a pixel canvas down to a sprite-sized matrix.
---
--- Used by two callers with the same need and different inputs: a decoded GIF
--- arrives as a flat canvas, and an imported `.rgba` sidecar arrives as a nested
--- matrix. Both shrink through the same filter so one asset does not read
--- differently depending on which file it came from.
---
--- A GIF authored for a screen is far larger than the 24x16-ish canvas a
--- terminal sprite occupies, and the unit contract fixes that footprint: one
--- sprite pixel is one cell wide and half a cell tall on every backend. So the
--- canvas is resampled here rather than drawn at its own size.
---
--- The filter is a true area average: each source pixel contributes only the
--- fraction of its area that actually falls inside the target cell, not a
--- flat 1/n share. That is what keeps a shrunk sprite from flickering or
--- popping at cell boundaries the way nearest-neighbour or unweighted
--- integer-range averaging would.

local M = {}

--- Alpha is weighted by covered *area*, not by opaque pixel count: a cell
--- straddling a sprite's edge is mostly transparent by area even when most of
--- the source pixels it touches are opaque slivers, so the bar sits lower than
--- a naive per-pixel-count threshold would need. Below this fraction of
--- covered weight the cell renders fully transparent rather than blending
--- toward whatever colour happens to be under the edge.
local OPAQUE_COVERAGE_THRESHOLD = 0.35

local function compute_bounds(target_index, target_size, source_size)
  local start_pos = target_index * source_size / target_size
  local end_pos = (target_index + 1) * source_size / target_size
  local first_idx = math.floor(start_pos)
  local last_idx = math.min(math.floor(end_pos), source_size - 1)
  return start_pos, end_pos, first_idx, last_idx
end

local function accumulate_cell(canvas, source, bounds)
  local start_x, end_x, first_x, last_x =
    bounds.start_x, bounds.end_x, bounds.first_x, bounds.last_x
  local start_y, end_y, first_y, last_y =
    bounds.start_y, bounds.end_y, bounds.first_y, bounds.last_y
  local total_red, total_green, total_blue, covered_weight, total_weight = 0, 0, 0, 0, 0

  for source_row = first_y, last_y do
    local y_weight = math.min(end_y, source_row + 1) - math.max(start_y, source_row)
    local row_offset = source_row * source.width
    for source_col = first_x, last_x do
      local x_weight = math.min(end_x, source_col + 1) - math.max(start_x, source_col)
      local pixel_weight = y_weight * x_weight
      local pixel_idx = row_offset + source_col + 1
      total_weight = total_weight + pixel_weight
      if canvas.opaque[pixel_idx] then
        covered_weight = covered_weight + pixel_weight
        total_red = total_red + canvas.red[pixel_idx] * pixel_weight
        total_green = total_green + canvas.green[pixel_idx] * pixel_weight
        total_blue = total_blue + canvas.blue[pixel_idx] * pixel_weight
      end
    end
  end

  if total_weight > 0 and (covered_weight / total_weight) >= OPAQUE_COVERAGE_THRESHOLD then
    return {
      math.floor(total_red / covered_weight + 0.5),
      math.floor(total_green / covered_weight + 0.5),
      math.floor(total_blue / covered_weight + 0.5),
    }
  end
  return false
end

--- Area-averages `canvas` down (or up) to `target.width` x `target.height`.
---@param canvas table `{ red, green, blue, opaque }`, flat and 1-based
---@param source table `{ width, height }`
---@param target table `{ width, height }`
---@return table<integer, table<integer, integer[]|false>> rows 1-based `[row][col]`
function M.to_matrix(canvas, source, target)
  local rows = {}
  for target_row = 0, target.height - 1 do
    local start_y, end_y, first_y, last_y = compute_bounds(target_row, target.height, source.height)
    local row = {}
    for target_col = 0, target.width - 1 do
      local start_x, end_x, first_x, last_x = compute_bounds(target_col, target.width, source.width)
      local bounds = {
        start_x = start_x,
        end_x = end_x,
        first_x = first_x,
        last_x = last_x,
        start_y = start_y,
        end_y = end_y,
        first_y = first_y,
        last_y = last_y,
      }
      row[target_col + 1] = accumulate_cell(canvas, source, bounds)
    end
    rows[target_row + 1] = row
  end
  return rows
end

--- Area-averages a nested pixel matrix down to `target`.
---
--- The sidecar reader produces `[row][col]` matrices rather than the flat canvas
--- a GIF decodes into, so this adapts one to the other instead of duplicating
--- the filter.
---@param rows table<integer, table<integer, integer[]|false>> 1-based `[row][col]`
---@param target table `{ width, height }`
---@return table<integer, table<integer, integer[]|false>> rows 1-based `[row][col]`
function M.shrink_matrix(rows, target)
  local source = { height = #rows, width = #rows[1] }
  local canvas = { red = {}, green = {}, blue = {}, opaque = {} }

  for row_index = 1, source.height do
    local row = rows[row_index]
    local offset = (row_index - 1) * source.width
    for column = 1, source.width do
      local pixel = row[column]
      local index = offset + column
      if pixel then
        canvas.red[index] = pixel[1]
        canvas.green[index] = pixel[2]
        canvas.blue[index] = pixel[3]
        canvas.opaque[index] = true
      else
        canvas.opaque[index] = false
      end
    end
  end

  return M.to_matrix(canvas, source, target)
end

return M
