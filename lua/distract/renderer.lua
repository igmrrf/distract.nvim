local M = {}
local api = vim.api
local sprites = require("distract.terminal_sprites")

local ns_id = api.nvim_create_namespace("distract_sprites")

-- Map of entity_id -> { buf = buf_handle, win = win_handle, last_frame_key = string }
local active_windows = {}

--- Draws all entities in terminal mode using the selected backend
function M.draw(entities, backend)
  backend = backend or "halfblock"
  local max_columns = vim.o.columns
  local max_lines = vim.o.lines

  for _, entity in ipairs(entities) do
    if backend == "halfblock" then
      M.draw_halfblock_entity(entity, max_columns, max_lines)
    else
      M.draw_float_entity(entity, max_columns, max_lines)
    end
  end

  -- Clean up windows for despawned entities
  local live_ids = {}
  for _, e in ipairs(entities) do
    live_ids[e.id] = true
  end
  for id, w in pairs(active_windows) do
    if not live_ids[id] then
      M.close_window(id)
    end
  end
end

function M.draw_halfblock_entity(entity, max_columns, max_lines)
  local pixel_frames = sprites.get_pixel_frames(entity.asset_name)
  local frame_idx = ((entity.frame_idx - 1) % #pixel_frames) + 1
  local pixel_matrix = pixel_frames[frame_idx] or pixel_frames[1]

  local lines, highlights = sprites.render_halfblock_frame(pixel_matrix)
  local sprite_w = #lines[1] or 16
  local sprite_h = #lines or 4

  local x = math.floor(entity.x)
  local y = math.floor(entity.y)
  local safe_x = math.max(0, math.min(x, max_columns - sprite_w))
  local safe_y = math.max(0, math.min(y, max_lines - sprite_h - 1))

  local entry = active_windows[entity.id]
  if not entry or not api.nvim_win_is_valid(entry.win) or not api.nvim_buf_is_valid(entry.buf) then
    local buf = api.nvim_create_buf(false, true)
    local win = api.nvim_open_win(buf, false, {
      relative = "editor",
      width = sprite_w,
      height = sprite_h,
      row = safe_y,
      col = safe_x,
      style = "minimal",
      border = "none",
      focusable = false,
      noautocmd = true,
      zindex = (entity.z_index or 0) + 100,
    })

    api.nvim_set_option_value("winblend", 0, { win = win })
    api.nvim_set_option_value("winhighlight", "Normal:NormalFloat,NormalNC:NormalFloat", { win = win })

    entry = { buf = buf, win = win, last_frame_idx = -1 }
    active_windows[entity.id] = entry
  end

  -- Update window position
  api.nvim_win_set_config(entry.win, {
    relative = "editor",
    row = safe_y,
    col = safe_x,
    width = sprite_w,
    height = sprite_h,
  })

  -- Redraw frame content only when frame changes
  if entry.last_frame_idx ~= frame_idx then
    entry.last_frame_idx = frame_idx
    api.nvim_buf_set_lines(entry.buf, 0, -1, false, lines)
    api.nvim_buf_clear_namespace(entry.buf, ns_id, 0, -1)

    for _, hl in ipairs(highlights) do
      api.nvim_buf_set_extmark(entry.buf, ns_id, hl.row, hl.col, {
        end_row = hl.row,
        end_col = hl.col + 1,
        hl_group = hl.hl,
        priority = 100,
      })
    end
  end
end

function M.draw_float_entity(entity, max_columns, max_lines)
  local sprite = sprites.get_ascii_sprite(entity.asset_name, entity.current_state, entity.frame_idx)
  local sprite_w = math.max(1, vim.fn.strdisplaywidth(sprite))
  local sprite_h = 1

  local x = math.floor(entity.x)
  local y = math.floor(entity.y)
  local safe_x = math.max(0, math.min(x, max_columns - sprite_w))
  local safe_y = math.max(0, math.min(y, max_lines - sprite_h - 1))

  local entry = active_windows[entity.id]
  if not entry or not api.nvim_win_is_valid(entry.win) or not api.nvim_buf_is_valid(entry.buf) then
    local buf = api.nvim_create_buf(false, true)
    local win = api.nvim_open_win(buf, false, {
      relative = "editor",
      width = sprite_w,
      height = sprite_h,
      row = safe_y,
      col = safe_x,
      style = "minimal",
      border = "none",
      focusable = false,
      noautocmd = true,
      zindex = (entity.z_index or 0) + 100,
    })

    api.nvim_set_option_value("winblend", 0, { win = win })
    api.nvim_set_option_value("winhighlight", "Normal:NormalFloat,NormalNC:NormalFloat", { win = win })

    entry = { buf = buf, win = win, last_sprite = "" }
    active_windows[entity.id] = entry
  end

  api.nvim_win_set_config(entry.win, {
    relative = "editor",
    row = safe_y,
    col = safe_x,
    width = sprite_w,
    height = sprite_h,
  })

  if entry.last_sprite ~= sprite then
    entry.last_sprite = sprite
    api.nvim_buf_set_lines(entry.buf, 0, -1, false, { sprite })
  end
end

function M.close_window(entity_id)
  local w = active_windows[entity_id]
  if w then
    if api.nvim_win_is_valid(w.win) then
      api.nvim_win_close(w.win, true)
    end
    if api.nvim_buf_is_valid(w.buf) then
      api.nvim_buf_delete(w.buf, { force = true })
    end
    active_windows[entity_id] = nil
  end
end

function M.clear_all()
  for id, _ in pairs(active_windows) do
    M.close_window(id)
  end
  active_windows = {}
end

return M
