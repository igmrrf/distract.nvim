--- Editor event plumbing.
---
--- Translates Neovim autocommands into engine events and routes them to
--- whichever backend is running.

local M = {}
local external = require("distract.external")
local engine = require("distract.engine")

local uv = vim.uv or vim.loop
local group = vim.api.nvim_create_augroup("DistractEvents", { clear = true })

-- The idle timer is created on demand and closed on teardown. Creating timers
-- at module load and only ever stopping them leaked a libuv handle per
-- setup/teardown cycle, which the test suite goes through repeatedly. The
-- debounce timer is gone entirely: throttling is a deadline per event name,
-- which needs no handle at all.
local idle_timer = nil

local config = {
  idle_timeout_ms = 5000,
  debounce_ms = 50,
}

-- Throttle state is per event name. A single shared flag was defeated exactly
-- when it mattered: in insert mode `TextChangedI` ("typing") and `CursorMovedI`
-- ("moving") both fire on every keystroke, so the name alternated every time,
-- the `current_event ~= event_name` branch short-circuited the throttle, and
-- every keystroke dispatched — flip-flopping the entity between walk_fast and
-- walk.
local throttled_until = {}

local function now_ms()
  return uv.hrtime() / 1e6
end

--- Routes an event to every running backend.
---
--- `reset_idle_timer` used to call `external.send_event` directly, so the
--- in-terminal backend — the default — never received an `idle` event and
--- `idle_timeout_ms` was dead config for it.
local function dispatch_event(event_name, context)
  if external.is_running() then
    external.send_event(event_name, context)
  end
  if engine.is_running() then
    engine.handle_editor_event(event_name, context)
  end
end

M.dispatch_event = dispatch_event

--- Where the cursor is, in screen cells.
---
--- This is the most informative signal the editor has, and it used to be
--- dropped at the boundary: the IPC `context` field was sent as an empty table
--- and destructured away on the engine side. Entities use it to orient toward
--- where the user is actually working.
local function cursor_context()
  local ok, col = pcall(vim.fn.screencol)
  if not ok then
    return nil
  end
  local ok_row, row = pcall(vim.fn.screenrow)
  return {
    cursor_col = col,
    cursor_row = ok_row and row or nil,
  }
end

function M.emit_debounced(event_name)
  M.reset_idle_timer()

  local until_ms = throttled_until[event_name]
  if until_ms and now_ms() < until_ms then
    return
  end

  throttled_until[event_name] = now_ms() + (config.debounce_ms or 0)
  dispatch_event(event_name, cursor_context())
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
  if not idle_timer then
    idle_timer = uv.new_timer()
  end
  idle_timer:stop()
  idle_timer:start(
    config.idle_timeout_ms,
    0,
    vim.schedule_wrap(function()
      dispatch_event("idle")
    end)
  )
end

local function close_timer(t)
  if t then
    t:stop()
    if not t:is_closing() then
      t:close()
    end
  end
end

function M.teardown()
  vim.api.nvim_clear_autocmds({ group = "DistractEvents" })
  close_timer(idle_timer)
  idle_timer = nil
  throttled_until = {}
end

--- Throttle state, for tests.
function M.throttle_state()
  return vim.deepcopy(throttled_until)
end

return M
