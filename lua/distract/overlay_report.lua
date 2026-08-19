--- Turning an engine response into a message for the user.
---
--- Only the responses a person should see are handled here. The ones the plugin
--- pipeline consumes — snapshots and world events — are routed by `external.lua`
--- and are deliberately silent: they arrive on a cadence and would bury every
--- other notification.

local M = {}

local function report_status(msg)
  local count = msg.count or 0
  if count == 0 then
    vim.notify("[Distract] No active entities.", vim.log.levels.INFO)
    return
  end

  local lines = { string.format("[Distract] %d active entities:", count) }
  for _, entity in ipairs(msg.entities or {}) do
    table.insert(
      lines,
      string.format(
        "  • #%d %s (state: %s, pos: %.0f, %.0f)",
        entity.id,
        entity.asset_name,
        entity.state,
        entity.x,
        entity.y
      )
    )
  end
  vim.notify(table.concat(lines, "\n"), vim.log.levels.INFO)
end

local REPORTERS = {
  ready = function(msg)
    vim.notify("[Distract] Engine v" .. tostring(msg.version) .. " active", vim.log.levels.INFO)
  end,
  spawned = function(msg)
    vim.notify(
      string.format("[Distract] Spawned %s (#%d) [%s]", msg.asset_name, msg.id, msg.state),
      vim.log.levels.INFO
    )
  end,
  action_triggered = function(msg)
    vim.notify(
      string.format("[Distract] %s (#%d) -> %s", msg.asset_name, msg.id, msg.action),
      vim.log.levels.INFO
    )
  end,
  despawned = function(msg)
    vim.notify(string.format("[Distract] Despawned entity #%d", msg.id), vim.log.levels.INFO)
  end,
  cleared = function()
    vim.notify("[Distract] All entities cleared", vim.log.levels.INFO)
  end,
  status_report = report_status,
  warning = function(msg)
    vim.notify("[Distract] " .. tostring(msg.message), vim.log.levels.WARN)
  end,
  error = function(msg)
    vim.notify("[Distract Error] " .. tostring(msg.message), vim.log.levels.ERROR)
  end,
}

--- Reports a response, if it is one the user should see.
---@param msg table a decoded engine response
---@return boolean whether it was reported
function M.notify(msg)
  local reporter = REPORTERS[msg.status]
  if not reporter then
    return false
  end
  reporter(msg)
  return true
end

return M
