--- Where each screen row's buffer line lives, so a sprite row can be drawn
--- straight onto it instead of over it.
---
--- This answers a question about the editor's layout, not about sprites, and
--- every in-terminal backend asks it. Rebuilt only when the layout actually
--- changes: building it costs a `screenpos` per visible line, which is not
--- something to pay 30 times a second for a screen that has not scrolled.

local M = {}
local api = vim.api

--- Where one screen row's buffer line is, and how far the window's text spans.
---@class DistractScreenSlot
---@field buf integer
---@field lnum integer 1-based buffer line
---@field text_left integer 0-based screen column of the window's first text cell
---@field text_right integer 0-based screen column of its last

---@type table<integer, DistractScreenSlot>
local rows = {}
local signature = nil
local version = 0

--- A cheap fingerprint of everything that could move a buffer line to a
--- different screen row.
local function layout_signature(wins)
  local parts = {}
  for _, wi in ipairs(wins) do
    parts[#parts + 1] = table.concat({
      wi.winid,
      wi.bufnr,
      wi.winrow,
      wi.wincol,
      wi.width,
      wi.height,
      wi.topline,
      wi.botline,
      wi.textoff,
    }, ":")
  end
  return table.concat(parts, "|")
end

--- Whether a window is one we may draw into: a normal, non-floating window
--- showing an ordinary buffer.
local function is_drawable_window(wi)
  if wi.terminal == 1 then
    return false
  end
  local ok, cfg = pcall(api.nvim_win_get_config, wi.winid)
  if not ok then
    return false
  end
  -- A float of our own, or someone else's popup, is not a drawing surface.
  return cfg.relative == nil or cfg.relative == ""
end

local function rebuild(wins)
  local map = {}

  for _, wi in ipairs(wins) do
    if is_drawable_window(wi) then
      -- `wincol`/`winrow` are 1-based screen coordinates of the window's
      -- top-left cell; `textoff` is the width of the gutter (number column,
      -- signs, folds) that virtual text is positioned relative to.
      local text_left = wi.wincol - 1 + wi.textoff
      local text_right = wi.wincol - 1 + wi.width - 1

      -- Only the row a line *starts* on is mapped. A wrapped line occupies
      -- several screen rows and only the first can be addressed by line number,
      -- so the continuation rows are left out and fall through to the float --
      -- correct rather than merely safe, since an extmark placed for them would
      -- land back on the row the line started on. Folded lines report row 0 and
      -- are skipped for the same reason.
      for lnum = wi.topline, wi.botline do
        local pos = vim.fn.screenpos(wi.winid, lnum, 1)
        if pos.row > 0 then
          map[pos.row - 1] = {
            buf = wi.bufnr,
            lnum = lnum,
            text_left = text_left,
            text_right = text_right,
          }
        end
      end
    end
  end

  return map
end

--- Refreshes the map if the layout moved. Returns its version, which changes
--- whenever the map does and is therefore safe to fold into a redraw guard.
---@return integer version
function M.sync()
  local wins = vim.fn.getwininfo()
  local sig = layout_signature(wins)
  if sig ~= signature then
    signature = sig
    rows = rebuild(wins)
    version = version + 1
  end
  return version
end

--- The version the last `sync` settled on.
---
--- A draw guard folds this in so a sprite standing still over text that moved
--- is still redrawn. It is read rather than threaded through every caller
--- because the two are always talking about the same tick.
---@return integer
function M.version()
  return version
end

--- The buffer line under a 0-based screen row, or nil where none can be
--- addressed -- past the end of the file, a wrapped continuation row, a fold.
---@param screen_row integer
---@return DistractScreenSlot|nil
function M.slot(screen_row)
  return rows[screen_row]
end

--- Drops the cached map. For tests, and for anything that changes the layout
--- without changing the fingerprint.
function M.invalidate()
  signature = nil
end

return M
