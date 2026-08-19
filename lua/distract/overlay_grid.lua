--- What the overlay is told about the terminal's geometry.
---
--- The cell size and the floor are the two measurements only Neovim can make,
--- and both reach the engine on the same `UpdateGrid` message. Kept apart from
--- the IPC client because this module *decides* the numbers and holds no process
--- state: it builds the command and the client sends it.

local M = {}

local config = {}

--- The floor last pushed to the engine, in terminal cells.
---
--- Held so `UpdateGrid` can carry it and so a floor that has not moved costs
--- nothing: it is sent on change, never per frame.
local ground_row = nil

--- Terminal cell size in physical pixels.
---
--- There is no portable way to ask a terminal for this from inside Neovim, and
--- it was previously hardcoded to 10x20 on both sides — so on any font that is
--- not exactly that, and never on a HiDPI display, overlay entities were
--- positioned against a coordinate space that matched nothing on screen.
---
--- Resolution order:
---   1. `cell_width` / `cell_height` from user config, if set.
---   2. The terminal's own report, when it answers `CSI 16 t`.
---   3. A documented 10x20 default.
---
--- See `:help distract-overlay`.
local DEFAULT_CELL_W, DEFAULT_CELL_H = 10, 20
local reported_cell = nil

--- Records a cell size reported by the terminal via `CSI 16 t`.
--- @param height number cell height in pixels
--- @param width number cell width in pixels
function M.set_reported_cell_size(height, width)
  if type(width) == "number" and type(height) == "number" and width > 0 and height > 0 then
    reported_cell = { width = width, height = height }
  end
end

function M.cell_size()
  local w = tonumber(config.cell_width)
  local h = tonumber(config.cell_height)
  if w and h and w > 0 and h > 0 then
    return w, h
  end
  if reported_cell then
    return reported_cell.width, reported_cell.height
  end
  return DEFAULT_CELL_W, DEFAULT_CELL_H
end

--- Asks the terminal for its cell size in pixels.
---
--- `CSI 16 t` is answered by kitty, WezTerm, Ghostty, foot and iTerm2, and
--- silently ignored elsewhere — so this is best effort and never blocks.
function M.query_cell_size()
  if vim.fn.has("nvim-0.10") ~= 1 then
    return
  end
  pcall(function()
    io.stdout:write("\27[16t")
  end)
end

--- The message that tells the engine the current geometry.
---@return table
function M.grid_command()
  local cw, ch = M.cell_size()
  return {
    command = "UpdateGrid",
    width = vim.o.columns,
    height = vim.o.lines,
    cell_width = cw,
    cell_height = ch,
    -- The floor is a position, so it converts with the cell height rather than
    -- with the sprite scale. Getting that wrong is the `ground_y` units bug.
    ground_y = ground_row and (ground_row * ch) or nil,
  }
end

--- Records the floor, reporting whether it actually moved.
---
--- Sent on the existing `UpdateGrid` message rather than a new one: the engine
--- already treats that as "the geometry changed", and a floor is geometry.
---@param row number|nil the floor in terminal cells, or nil for none
---@return boolean whether it moved, and so needs pushing
function M.set_ground_row(row)
  if row == ground_row then
    return false
  end
  ground_row = row
  return true
end

--- The floor last pushed, in cells. For tests and diagnostics.
function M.get_ground_row()
  return ground_row
end

--- Holds the user config the cell size resolves against.
function M.configure(opts)
  config = opts or {}
end

return M
