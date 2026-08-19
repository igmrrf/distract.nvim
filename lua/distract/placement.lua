--- Where one sprite's surface goes, in terminal cells.
---
--- Pure geometry: it reads the screen map to find where buffer text is and
--- decides the clamped rectangle plus the split between the rows that can be
--- drawn *onto* text and the tail that needs a floating window. It calls no
--- window API, which is what makes it testable without a UI and is why the
--- renderer keeps the API calls and nothing else.
---
--- Bounds carry an origin, because a scoped viewport is a rectangle somewhere
--- inside the editor grid rather than the grid itself.
---
--- A wrapping entity is drawn in **slices**. `wrap_mode = "wrap"` lets an entity
--- hang off an edge — physics only teleports it once it is entirely past — and
--- clamping the surface back onto the screen is what made the sprite appear to
--- stop at the edge and then pop. The departing part is instead drawn at the
--- complementary coordinate, and a sprite leaving a corner needs four slices.
--- The same 1D rule is mirrored by `engine/src/wrap.rs` for the GPU overlay.

local M = {}

local screen_map = require("distract.screen_map")

---@class DistractSlice
---@field row integer screen row to draw at
---@field col integer screen column to draw at
---@field width integer
---@field height integer
---@field src_row integer first surface row this slice shows
---@field src_col integer first surface column this slice shows

---@class DistractPlacement
---@field row integer top screen row of the primary slice
---@field col integer left screen column of the primary slice
---@field width integer
---@field height integer
---@field overlay_limit integer sprite rows drawn onto buffer text
---@field float_row integer first screen row the float covers
---@field float_height integer rows left for the float
---@field slices DistractSlice[] every piece to draw, primary first

--- The first sprite row that cannot be drawn onto buffer text.
---
--- Everything from there down goes to the float. It is a tail rather than a set
--- of individual rows because the rows that fail are almost always the ones
--- below the end of the file, which are contiguous and at the bottom; treating
--- an isolated failure in the middle as the start of the tail costs a few rows
--- of occluded text, which is what every row cost before.
---@return integer
function M.first_unmappable_row(rect)
  for offset = 0, rect.height - 1 do
    local slot = screen_map.slot(rect.row + offset)
    if not slot or rect.col < slot.text_left or rect.col + rect.width - 1 > slot.text_right then
      return offset
    end
  end
  return rect.height
end

--- One axis of a wrapping surface, as the pieces that are actually on screen.
---
--- The axis is a circle: the position is first brought into the rectangle
--- modulo its extent, which is what makes an entity two cells past the right
--- edge and one that has not been teleported yet describe the same picture. What
--- is left is at most two pieces — the part at that position, and whatever runs
--- past the far edge and reappears at the near one.
---@return table[] `{ at = integer, len = integer, src = integer }`
local function wrapped_spans(position, size, min, max)
  local extent = max - min
  if extent <= 0 then
    return {}
  end

  local start = min + ((math.floor(position) - min) % extent)
  local spans = {}

  local function push(at, len, src)
    if len > 0 then
      table.insert(spans, { at = at, len = math.min(len, extent), src = src })
    end
  end

  push(start, math.min(size, max - start), 0)

  local past_far_edge = (start + size) - max
  if past_far_edge > 0 then
    push(min, past_far_edge, size - past_far_edge)
  end

  return spans
end

M.wrapped_spans = wrapped_spans

--- The one clamped piece a non-wrapping surface is drawn as.
local function clamped_span(position, size, min, max)
  local length = math.min(size, math.max(1, max - min))
  local at = math.max(min, math.min(math.floor(position), max - length))
  return { { at = at, len = length, src = 0 } }
end

--- Clamps or slices a surface into the bounds and splits it between text and a
--- float.
---@param request { x: number, y: number, width: integer, height: integer, bounds: table, wrap: boolean|nil }
---@return DistractPlacement
function M.resolve(request)
  local bounds = request.bounds
  local min_col = bounds.col or 0
  local min_row = bounds.row or 0
  local max_col = min_col + bounds.columns
  -- One row is reserved at the bottom: the command line is not a surface a
  -- float may occupy, and `bounce` and `clamp` measure against the same edge.
  local max_row = min_row + math.max(1, bounds.lines - 1)

  local horizontal, vertical
  if request.wrap then
    horizontal = wrapped_spans(request.x, request.width, min_col, max_col)
    vertical = wrapped_spans(request.y, request.height, min_row, max_row)
  else
    horizontal = clamped_span(request.x, request.width, min_col, max_col)
    vertical = clamped_span(request.y, request.height, min_row, max_row)
  end

  local slices = {}
  for _, row_span in ipairs(vertical) do
    for _, col_span in ipairs(horizontal) do
      table.insert(slices, {
        row = row_span.at,
        col = col_span.at,
        width = col_span.len,
        height = row_span.len,
        src_row = row_span.src,
        src_col = col_span.src,
      })
    end
  end

  -- A surface entirely outside the bounds produces no slice at all, which is
  -- not a rectangle any window API will accept. Reported as an empty list
  -- rather than a degenerate one so the caller drops the frame.
  local primary = slices[1]
  if not primary then
    return { slices = {} }
  end

  -- Drawing onto buffer text is only attempted for a surface drawn in one
  -- piece. A wrapped sprite is transient — a second or two while it crosses the
  -- seam — and reconciling a column-sliced extmark run with the gutter would
  -- buy nothing but a much harder redraw guard.
  local overlay_limit = 0
  if #slices == 1 and primary.src_col == 0 and primary.src_row == 0 then
    overlay_limit = M.first_unmappable_row(primary)
  end

  return {
    row = primary.row,
    col = primary.col,
    width = primary.width,
    height = primary.height,
    overlay_limit = overlay_limit,
    float_row = primary.row + overlay_limit,
    float_height = primary.height - overlay_limit,
    slices = slices,
  }
end

return M
