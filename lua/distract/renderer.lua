local M = {}
local api = vim.api

-- Map of entity id -> {buf, win}
local active_windows = {}

function M.draw(entities)
  local max_columns = vim.o.columns
  local max_lines = vim.o.lines

  for _, entity in ipairs(entities) do
    local state = entity:get_render_state()
    local x = math.floor(state.x)
    local y = math.floor(state.y)
    local sprite = state.sprite

    if not active_windows[entity.id] then
      -- Create unlisted, scratch buffer
      local buf = api.nvim_create_buf(false, true)
      
      -- Create floating window
      local width = math.max(1, vim.fn.strdisplaywidth(sprite))
      local win = api.nvim_open_win(buf, false, {
        relative = 'editor',
        width = width,
        height = 1,
        row = y,
        col = x,
        style = 'minimal',
        border = 'none',
        focusable = false,
        noautocmd = true,
      })
      
      -- Ensure floating window text is visible (winblend = 0)
      api.nvim_set_option_value('winblend', 0, {win = win})
      api.nvim_set_option_value('winhighlight', 'Normal:NormalFloat,NormalNC:NormalFloat', {win = win})
      
      active_windows[entity.id] = { buf = buf, win = win }

    end
    
    local w = active_windows[entity.id]
    if api.nvim_win_is_valid(w.win) and api.nvim_buf_is_valid(w.buf) then
      -- Safety bounds
      local width = math.max(1, vim.fn.strdisplaywidth(sprite))
      local safe_x = math.max(0, math.min(x, max_columns - width))
      local safe_y = math.max(0, math.min(y, max_lines - 2)) -- -2 to keep above cmdline
      
      -- Update window position and size
      api.nvim_win_set_config(w.win, {
        relative = 'editor',
        row = safe_y,
        col = safe_x,
        width = width,
        height = 1
      })
      
      -- Update sprite content
      api.nvim_buf_set_lines(w.buf, 0, -1, false, { sprite })
    end
  end
end

function M.clear_all()
  for _, w in pairs(active_windows) do
    if api.nvim_win_is_valid(w.win) then
      api.nvim_win_close(w.win, true)
    end
    if api.nvim_buf_is_valid(w.buf) then
      api.nvim_buf_delete(w.buf, {force = true})
    end
  end
  active_windows = {}
end

return M
