--- The plugin pipeline's view of the overlay's world.
---
--- The in-terminal engines simulate in Lua, so a hook can be dispatched from the
--- step itself. The overlay simulates in its own process, so the same hooks are
--- driven from what it reports: snapshots on a bounded cadence for `on_tick`,
--- and world events for `on_state_change` and `on_collision`. Nothing is
--- requested unless a registered plugin actually subscribes.
---
--- Everything a hook sees is in **terminal cells**, on every backend. The
--- overlay reports physical pixels, so they are converted here rather than left
--- for each plugin to divide by a cell size it would have to go looking for.

local M = {}

local plugins = require("distract.plugins")
local sprites = require("distract.terminal_sprites")

--- How often the overlay is asked for a snapshot, in milliseconds.
---
--- Ten a second: enough for a plugin reacting to where a sprite is, far below
--- the 60 FPS the simulation actually runs at. The engine clamps it too.
local SNAPSHOT_INTERVAL_MS = 100

--- The hooks that need to be told what the world is doing frame by frame.
local SNAPSHOT_HOOKS = { "on_tick", "on_draw" }

--- The latest reported entities, in cells, newest snapshot wins.
local reported = {}

--- The interval to subscribe at, or nil when nothing is listening.
---@return integer|nil
function M.desired_snapshot_ms()
  for _, hook in ipairs(SNAPSHOT_HOOKS) do
    if plugins.has_subscriber(hook) then
      return SNAPSHOT_INTERVAL_MS
    end
  end
  return nil
end

--- Whether any hook needs the engine's journal running.
---
--- State changes and collisions are events, not frames, so a plugin that only
--- reacts to them costs one message per event and no per-frame traffic — but the
--- engine still has to be subscribed for its journal to record anything.
---@return boolean
function M.wants_world_events()
  return plugins.has_subscriber("on_state_change") or plugins.has_subscriber("on_collision")
end

--- The entities as last reported, for the world handle.
function M.entities()
  return reported
end

function M.reset()
  reported = {}
end

local function find(id)
  for _, entity in ipairs(reported) do
    if entity.id == id then
      return entity
    end
  end
  return nil
end

--- Records a snapshot and dispatches `on_tick` and `on_draw` from it.
---@param payload table the `snapshot` response
---@param cell { width: number, height: number } overlay pixels per terminal cell
function M.on_snapshot(payload, cell)
  local cell_width = math.max(1, cell.width or 1)
  local cell_height = math.max(1, cell.height or 1)

  reported = {}
  for _, summary in ipairs(payload.entities or {}) do
    table.insert(reported, {
      id = summary.id,
      asset_name = summary.asset_name,
      current_state = summary.state,
      x = summary.x / cell_width,
      y = summary.y / cell_height,
      vx = summary.vx / cell_width,
      vy = summary.vy / cell_height,
    })
  end

  local dt = payload.dt or (SNAPSHOT_INTERVAL_MS / 1000)
  for _, entity in ipairs(reported) do
    plugins.dispatch_tick(entity, dt)
  end
  plugins.dispatch_draw(M.layers())
end

--- Where each reported entity sits, in cells, with the footprint it occupies.
---@return table[]
function M.layers()
  local layers = {}
  for _, entity in ipairs(reported) do
    local ok, width, height = pcall(sprites.get_dimensions, entity.asset_name)
    table.insert(layers, {
      id = entity.id,
      asset_name = entity.asset_name,
      row = math.floor(entity.y),
      col = math.floor(entity.x),
      width = ok and width or nil,
      height = ok and height or nil,
    })
  end
  return layers
end

--- Dispatches the engine's journal to the hooks that asked for it.
---
--- An event about an entity no snapshot has described yet is dropped: a hook
--- receiving an entity with no position would be worse than receiving nothing.
---@param payload table the `world_events` response
function M.on_world_events(payload)
  if (payload.dropped or 0) > 0 then
    vim.notify(
      string.format(
        "[Distract] %d world event(s) dropped before Neovim read them; "
          .. "a plugin hook may be slower than the engine.",
        payload.dropped
      ),
      vim.log.levels.WARN
    )
  end

  for _, event in ipairs(payload.events or {}) do
    local entity = find(event.id)
    if entity then
      if event.event == "state_change" then
        entity.current_state = event.to
        plugins.dispatch_state_change(entity, event.from, event.to)
      elseif event.event == "collision" then
        plugins.dispatch_collision(entity, { edge = event.edge, target = nil })
      end
    end
  end
end

--- Sends what the hooks asked the world for.
---
--- The same queue `engine.lua` applies locally, forwarded as IPC commands so one
--- plugin produces one behaviour on either backend.
---@param send function called with each command table
function M.flush_commands(send)
  for _, command in ipairs(plugins.drain_commands()) do
    if command.kind == "state" then
      send({ command = "SetState", id = command.id, state = command.state })
    elseif command.kind == "impulse" then
      send({ command = "Impulse", id = command.id, vx = command.vx, vy = command.vy })
    elseif command.kind == "despawn" then
      send({ command = "Despawn", id = command.id })
    end
  end
end

return M
