--- Plugin middleware and the lifecycle hook pipeline.
---
--- Plugins observe the simulation and ask it for changes; they never write to
--- it. The entity a hook receives is a read-only proxy and every mutation goes
--- through a world command, because the in-terminal backends simulate in Lua
--- while the overlay simulates in a separate Rust process: a hook that assigned
--- `entity.vx` would move the sprite on one backend and nothing on the other,
--- and one manifest plus one plugin has to behave the same way on both. Commands
--- are queued here and applied by whichever backend is running — locally by
--- `engine.lua`, over IPC by `external.lua`.
---
--- Failure policy: a hook that errors is reported once and its plugin is
--- disabled for the session. A plugin that throws every tick would otherwise
--- produce `fps` notifications per second.

local M = {}

---@alias DistractHookName
---| "on_init" | "on_tick" | "on_state_change" | "on_collision"
---| "on_editor_event" | "on_draw" | "on_teardown"

local HOOK_NAMES = {
  "on_init",
  "on_tick",
  "on_state_change",
  "on_collision",
  "on_editor_event",
  "on_draw",
  "on_teardown",
}

local IS_HOOK = {}
for _, name in ipairs(HOOK_NAMES) do
  IS_HOOK[name] = true
end

--- Registered plugins in registration order, which is dispatch order.
local ordered = {}
local by_name = {}

--- Queued world commands, drained by the running backend.
local commands = {}

--- Whether a plugin asked for a redraw the simulation would not have asked for.
local is_dirty = false

--- What the world handle reads entities from, and which backend is running.
local source = nil

local ENTITY_PROXY_MESSAGE = "distract: an entity passed to a hook is read-only; "
  .. "use world.request_state / world.apply_impulse / world.despawn"

--- A read-only view of one live entity.
---
--- Shallow by design: nested tables reached through it are the engine's own and
--- are documented as read-only rather than copied, because a hook runs per
--- entity per tick and a deep copy there is a per-frame allocation.
---@param entity table
---@return table
local function read_only(entity)
  return setmetatable({}, {
    __index = entity,
    __newindex = function()
      error(ENTITY_PROXY_MESSAGE, 2)
    end,
    __len = function()
      return 0
    end,
    __metatable = false,
  })
end

M.read_only = read_only

local function disable(plugin, hook_name, err)
  plugin.disabled = true
  vim.notify(
    string.format(
      "[Distract] Plugin '%s' disabled: %s raised an error.\n%s",
      plugin.name,
      hook_name,
      tostring(err)
    ),
    vim.log.levels.WARN
  )
end

local function invoke(plugin, hook_name, ...)
  local hook = plugin.spec[hook_name]
  if not hook or plugin.disabled then
    return
  end
  local ok, err = xpcall(hook, debug.traceback, ...)
  if not ok then
    disable(plugin, hook_name, err)
  end
end

local function dispatch(hook_name, ...)
  for _, plugin in ipairs(ordered) do
    invoke(plugin, hook_name, ...)
  end
end

--- Whether any registered, enabled plugin subscribes to a hook.
---
--- The overlay backend asks this before subscribing to world snapshots: nothing
--- goes on the wire per frame unless a plugin is actually listening.
---@param hook_name DistractHookName
---@return boolean
function M.has_subscriber(hook_name)
  for _, plugin in ipairs(ordered) do
    if not plugin.disabled and plugin.spec[hook_name] then
      return true
    end
  end
  return false
end

--- Registers a plugin.
---@param name string unique plugin name
---@param spec table<DistractHookName, function>
function M.register(name, spec)
  if type(name) ~= "string" or name == "" then
    error("distract.register_plugin: name must be a non-empty string")
  end
  if type(spec) ~= "table" then
    error("distract.register_plugin: spec must be a table of hooks")
  end
  if by_name[name] then
    error(string.format("distract.register_plugin: '%s' is already registered", name))
  end

  local declared = 0
  for key, value in pairs(spec) do
    if not IS_HOOK[key] then
      error(
        string.format(
          "distract.register_plugin: '%s' declares unknown hook '%s'; known hooks are %s",
          name,
          tostring(key),
          table.concat(HOOK_NAMES, ", ")
        )
      )
    end
    if type(value) ~= "function" then
      error(string.format("distract.register_plugin: '%s' hook '%s' must be a function", name, key))
    end
    declared = declared + 1
  end

  if declared == 0 then
    error(string.format("distract.register_plugin: '%s' declares no hooks", name))
  end

  local plugin = { name = name, spec = spec, disabled = false }
  by_name[name] = plugin
  table.insert(ordered, plugin)

  if source then
    invoke(plugin, "on_init", M.world())
  end
end

function M.unregister(name)
  local plugin = by_name[name]
  if not plugin then
    return false
  end
  invoke(plugin, "on_teardown")
  by_name[name] = nil
  for index, candidate in ipairs(ordered) do
    if candidate == plugin then
      table.remove(ordered, index)
      break
    end
  end
  return true
end

--- Clears every registration. For tests, and for a full plugin reload.
function M.reset()
  ordered = {}
  by_name = {}
  commands = {}
  is_dirty = false
  source = nil
end

function M.names()
  local names = {}
  for _, plugin in ipairs(ordered) do
    table.insert(names, plugin.name)
  end
  return names
end

function M.is_disabled(name)
  local plugin = by_name[name]
  return plugin ~= nil and plugin.disabled
end

--- Marks the world worth redrawing for one frame.
---
--- `is_quiescent()` suppresses the redraw of an unchanged picture, which would
--- otherwise also suppress a layer a plugin drew.
function M.mark_dirty()
  is_dirty = true
end

function M.consume_dirty()
  local was_dirty = is_dirty
  is_dirty = false
  return was_dirty
end

local function enqueue(command)
  table.insert(commands, command)
  is_dirty = true
end

--- Takes the queued commands, leaving the queue empty.
---@return table[] commands `{ kind = "state"|"impulse"|"despawn", id = integer, ... }`
function M.drain_commands()
  if #commands == 0 then
    return {}
  end
  local drained = commands
  commands = {}
  return drained
end

--- Binds the world handle to the running backend.
---@param spec { backend: string, entities: function }
function M.bind_world(spec)
  source = spec
  for _, plugin in ipairs(ordered) do
    invoke(plugin, "on_init", M.world())
  end
end

function M.unbind_world()
  source = nil
end

--- The handle hooks receive: read access, and commands for everything else.
---@return table|nil
function M.world()
  if not source then
    return nil
  end
  return {
    backend = source.backend,
    entities = function()
      local views = {}
      for _, entity in ipairs(source.entities()) do
        table.insert(views, read_only(entity))
      end
      return views
    end,
    request_state = function(id, state)
      if type(id) ~= "number" or type(state) ~= "string" or state == "" then
        error("world.request_state(id, state): id must be a number and state a non-empty string")
      end
      enqueue({ kind = "state", id = id, state = state })
    end,
    apply_impulse = function(id, vx, vy)
      if type(id) ~= "number" then
        error("world.apply_impulse(id, vx, vy): id must be a number")
      end
      enqueue({ kind = "impulse", id = id, vx = vx or 0, vy = vy or 0 })
    end,
    despawn = function(id)
      if type(id) ~= "number" then
        error("world.despawn(id): id must be a number")
      end
      enqueue({ kind = "despawn", id = id })
    end,
    mark_dirty = M.mark_dirty,
  }
end

function M.dispatch_tick(entity, dt)
  dispatch("on_tick", read_only(entity), dt)
end

function M.dispatch_state_change(entity, from_state, to_state)
  dispatch("on_state_change", read_only(entity), from_state, to_state)
end

---@param collision { edge: string, target: table|nil }
function M.dispatch_collision(entity, collision)
  dispatch("on_collision", read_only(entity), collision)
end

function M.dispatch_editor_event(event_name, context)
  dispatch("on_editor_event", event_name, context)
end

--- Layers are reported in terminal cells on every backend, so a plugin that
--- draws next to a sprite does not need to know which renderer is running.
---@param layers table[] `{ id, asset_name, row, col, width, height }`
function M.dispatch_draw(layers)
  dispatch("on_draw", layers)
end

function M.dispatch_teardown()
  dispatch("on_teardown")
end

return M
