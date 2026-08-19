local M = {}

local plugins = require("distract.plugins")

function M.is_quiescent(entities)
  if plugins.consume_dirty() then
    return false
  end

  for _, entity in ipairs(entities) do
    if not entity.is_active then
      return false
    end
    if entity.action_timer then
      return false
    end
    if math.abs(entity.vx) > 0.001 or math.abs(entity.vy) > 0.001 then
      return false
    end

    local states = entity.manifest and entity.manifest.states
    local state_def = states and states[entity.current_state]
    if state_def then
      local anim = state_def.animation
      if anim and anim.frames and #anim.frames > 1 then
        return false
      end
      local transitions = state_def.transitions
      if transitions and transitions.timeout_ms then
        return false
      end
      local path_type = state_def.physics and state_def.physics.path_type
      if path_type and path_type ~= "linear" then
        return false
      end
    end
  end
  return true
end

return M
