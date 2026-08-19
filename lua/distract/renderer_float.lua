local M = {}

local api = vim.api
local viewport = require("distract.viewport")

local BACKGROUND_GROUP = "DistractSpriteNormal"
local background_defined = false

function M.background_group()
  if not background_defined then
    api.nvim_set_hl(0, BACKGROUND_GROUP, { bg = "NONE", fg = "NONE" })
    background_defined = true
  end
  return BACKGROUND_GROUP
end

function M.refresh_highlights()
  background_defined = false
  M.background_group()
end

function M.windows_are_valid(entry)
  for _, slice in ipairs(entry.slices or {}) do
    if slice.win and not api.nvim_win_is_valid(slice.win) then
      return false
    end
  end
  return true
end

function M.place_float(entity, previous, surface_buf, slice)
  local win = previous and previous.win
  if not win or not api.nvim_win_is_valid(win) then
    win = api.nvim_open_win(surface_buf, false, {
      relative = "editor",
      width = slice.width,
      height = slice.height,
      row = slice.row,
      col = slice.col,
      style = "minimal",
      border = "none",
      focusable = false,
      noautocmd = true,
      zindex = (entity.z_index or 0) + viewport.z_index_offset(),
    })
    api.nvim_set_option_value("winblend", 0, { win = win })
    api.nvim_set_option_value("wrap", false, { win = win })
    api.nvim_set_option_value(
      "winhighlight",
      "Normal:" .. M.background_group() .. ",NormalNC:" .. M.background_group(),
      { win = win }
    )
  elseif
    previous.row ~= slice.row
    or previous.col ~= slice.col
    or previous.width ~= slice.width
    or previous.height ~= slice.height
  then
    api.nvim_win_set_config(win, {
      relative = "editor",
      row = slice.row,
      col = slice.col,
      width = slice.width,
      height = slice.height,
    })
  end

  if not previous or previous.buf ~= surface_buf then
    api.nvim_win_set_buf(win, surface_buf)
  end

  if
    not previous
    or previous.buf ~= surface_buf
    or previous.src_row ~= slice.src_row
    or previous.src_col ~= slice.src_col
  then
    pcall(api.nvim_win_set_cursor, win, { slice.src_row + 1, 0 })
    pcall(api.nvim_win_call, win, function()
      vim.fn.winrestview({
        topline = slice.src_row + 1,
        lnum = slice.src_row + 1,
        leftcol = slice.src_col,
      })
    end)
  end

  return win
end

return M
