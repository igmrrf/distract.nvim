--- Editor event plumbing.
---
--- Translates Neovim autocommands into engine events and routes them to
--- whichever backend is running.

local M = {}
local external = require("distract.external")
local engine = require("distract.engine")
local position = require("distract.position")
local obstacles = require("distract.obstacles")
local visibility = require("distract.visibility")

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
  position = nil,
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
  require("distract.plugins").dispatch_editor_event(event_name, context or {})
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

--- Measures the floor and pushes it to both engines.
---
--- Neither engine measures for itself. Only the editor can see `cmdheight`,
--- the statusline and where the buffer text ends, so the measurement happens
--- once here and the same number reaches the terminal renderer and the overlay
--- process. Cheap when nothing moved: the overlay only sends a message when the
--- value changes.
---@param position_config table|nil the `position` block from `setup`
function M.sync_floor(position_config)
  if position_config then
    config.position = position_config
  end
  local row = position.floor_row((config.position or {}).ground)
  engine.set_ground_row(row)
  external.set_ground_row(row)
end

--- Collects the registered obstacles and pushes them to both engines.
---
--- Debounced by its callers, never called per tick: a provider may run a
--- Tree-sitter query, and doing that per frame per entity is the performance trap
--- the provider contract exists to avoid. Cheap when nothing registered one --
--- `collect` returns immediately with no providers.
function M.sync_obstacles()
  if obstacles.provider_count() == 0 then
    return
  end
  local rects = obstacles.collect()
  engine.set_obstacles(rects)
  external.set_obstacles(rects)
end

--- Whether the floor follows the buffer text, and so moves as the text does.
local function is_text_grounded()
  return (config.position or {}).ground == position.TEXT
end

function M.setup(opts)
  if opts then
    config.idle_timeout_ms = opts.idle_timeout_ms or config.idle_timeout_ms
    config.debounce_ms = opts.debounce_ms or config.debounce_ms
    config.position = opts.position or config.position
    visibility.configure(opts)
  end

  -- Detect typing
  vim.api.nvim_create_autocmd({ "TextChanged", "TextChangedI" }, {
    group = group,
    callback = function()
      M.emit_debounced("typing")
      -- Text arriving or leaving moves a text floor and nothing else, so the
      -- measurement is skipped entirely for a screen floor.
      if is_text_grounded() then
        M.sync_floor()
      end
      -- Editing moves every function header in the file, so what a provider
      -- reported is stale. Throttled by the same deadline the events use.
      M.sync_obstacles()
    end,
  })

  -- Detect scrolling
  vim.api.nvim_create_autocmd("WinScrolled", {
    group = group,
    callback = function()
      M.emit_debounced("scrolling")
      if is_text_grounded() then
        M.sync_floor()
      end
      external.sync_viewport_scope()
      M.sync_obstacles()
    end,
  })

  -- The scoped rectangle follows the window the user is working in, so it is
  -- re-measured when that changes rather than per tick: resolving a window rect
  -- is several API calls and a `getwininfo`.
  vim.api.nvim_create_autocmd({ "WinEnter", "WinResized", "BufWinEnter", "WinClosed" }, {
    group = group,
    callback = function()
      external.sync_viewport_scope()
      M.sync_obstacles()
    end,
  })

  -- Detect cursor movement
  vim.api.nvim_create_autocmd({ "CursorMoved", "CursorMovedI" }, {
    group = group,
    callback = function()
      M.emit_debounced("moving")
    end,
  })

  -- Focus. A companion belongs to the instance it was spawned from, so an
  -- unfocused instance stops drawing and keeps simulating.
  vim.api.nvim_create_autocmd({ "FocusGained", "FocusLost" }, {
    group = group,
    callback = function(event)
      M.set_focus(event.event == "FocusGained")
    end,
  })

  -- Detect terminal resize
  vim.api.nvim_create_autocmd("VimResized", {
    group = group,
    callback = function()
      external.update_grid()
      M.sync_floor()
      external.sync_viewport_scope()
    end,
  })

  -- The screen floor is the screen height less the editor's own chrome, so it
  -- moves when that chrome does.
  vim.api.nvim_create_autocmd("OptionSet", {
    group = group,
    pattern = { "cmdheight", "laststatus" },
    callback = function()
      M.sync_floor()
    end,
  })

  M.sync_floor()
  M.sync_obstacles()
  M.reset_idle_timer()
end

--- Records a focus change and tells the backends to show or hide.
---
--- Exposed rather than inlined into the autocommand because `FocusGained` and
--- `FocusLost` never fire headless, and this is the seam the specs drive.
---@param gained boolean
function M.set_focus(gained)
  if not visibility.set_focus(gained) then
    return
  end
  local is_visible = visibility.is_visible()
  engine.set_visible(is_visible)
  external.set_visible(is_visible)
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
