local M = {}
local external = require("distract.external")

local group = vim.api.nvim_create_augroup("DistractEvents", { clear = true })
local idle_timer = (vim.uv or vim.loop).new_timer()
local debounce_timer = (vim.uv or vim.loop).new_timer()

local config = {
  idle_timeout_ms = 5000,
  debounce_ms = 50,
}

local current_event = nil
local is_throttled = false

function M.emit_debounced(event_name)
  M.reset_idle_timer()

  if current_event ~= event_name or not is_throttled then
    current_event = event_name
    is_throttled = true
    external.send_event(event_name)

    debounce_timer:stop()
    debounce_timer:start(config.debounce_ms, 0, vim.schedule_wrap(function()
      is_throttled = false
    end))
  end
end


function M.setup(opts)
  if opts then
    config.idle_timeout_ms = opts.idle_timeout_ms or config.idle_timeout_ms
    config.debounce_ms = opts.debounce_ms or config.debounce_ms
  end

  -- Detect typing
  vim.api.nvim_create_autocmd({ "TextChanged", "TextChangedI" }, {
    group = group,
    callback = function()
      M.emit_debounced("typing")
    end,
  })

  -- Detect scrolling
  vim.api.nvim_create_autocmd("WinScrolled", {
    group = group,
    callback = function()
      M.emit_debounced("scrolling")
    end,
  })

  -- Detect cursor movement
  vim.api.nvim_create_autocmd({ "CursorMoved", "CursorMovedI" }, {
    group = group,
    callback = function()
      M.emit_debounced("moving")
    end,
  })

  -- Detect terminal resize
  vim.api.nvim_create_autocmd("VimResized", {
    group = group,
    callback = function()
      external.update_grid()
    end,
  })

  M.reset_idle_timer()
end

function M.reset_idle_timer()
  idle_timer:stop()
  idle_timer:start(config.idle_timeout_ms, 0, vim.schedule_wrap(function()
    external.send_event("idle")
  end))
end

function M.teardown()
  vim.api.nvim_clear_autocmds({ group = "DistractEvents" })
  idle_timer:stop()
  debounce_timer:stop()
  current_event = nil
  is_throttled = false
end

return M
