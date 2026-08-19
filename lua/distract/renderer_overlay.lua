local M = {}

local api = vim.api
local screen_map = require("distract.screen_map")

local overlay_ns = api.nvim_create_namespace("distract_sprite_overlay")

function M.overlay_namespace()
  return overlay_ns
end

function M.slice_signature(geom)
  local parts = { geom.overlay_limit }
  for _, slice in ipairs(geom.slices) do
    table.insert(
      parts,
      table.concat(
        { slice.row, slice.col, slice.width, slice.height, slice.src_row, slice.src_col },
        ","
      )
    )
  end
  return table.concat(parts, "|")
end

function M.clear_overlay(entry)
  if not entry or not entry.marks then
    return
  end
  for _, mark in ipairs(entry.marks) do
    if api.nvim_buf_is_valid(mark[1]) then
      pcall(api.nvim_buf_del_extmark, mark[1], overlay_ns, mark[2])
    end
  end
  entry.marks = nil
end

function M.draw_overlay_rows(rows, row, col, limit)
  local marks = {}
  for r = 0, limit - 1 do
    local slot = screen_map.slot(row + r)
    for _, run in ipairs(rows[r] or {}) do
      local ok, id = pcall(api.nvim_buf_set_extmark, slot.buf, overlay_ns, slot.lnum - 1, 0, {
        virt_text = run.chunks,
        virt_text_win_col = col + run.col - slot.text_left,
        hl_mode = "combine",
        priority = 200,
        ephemeral = false,
      })
      if ok then
        marks[#marks + 1] = { slot.buf, id }
      end
    end
  end
  return marks
end

return M
