local M = {}

local focus_col = nil

local function face_toward(entity, target_x)
  local dx = target_x - entity.x
  if math.abs(dx) < 1 then
    return
  end
  entity.heading_x = dx > 0 and 1 or -1
  entity.flip_x = entity.heading_x < 0
end

M.face_toward = face_toward

function M.trigger_action(entities, set_entity_state, action_name, target)
  local triggered_count = 0

  for _, entity in ipairs(entities) do
    local match
    if type(target) == "number" then
      match = (entity.id == target)
    elseif type(target) == "string" and target ~= "" then
      match = (entity.asset_name == target)
    else
      match = true
    end

    if match and entity.manifest.custom_actions then
      local action_def = entity.manifest.custom_actions[action_name]
      if action_def then
        local target_state = action_def.target_state
        if type(target_state) ~= "string" or target_state == "" then
          vim.notify(
            string.format(
              "[Distract] Action '%s' on '%s' has no target_state; ignoring it.",
              action_name,
              entity.asset_name
            ),
            vim.log.levels.WARN
          )
        elseif not (entity.manifest.states and entity.manifest.states[target_state]) then
          vim.notify(
            string.format(
              "[Distract] Action '%s' on '%s' targets unknown state '%s'; ignoring it.",
              action_name,
              entity.asset_name,
              target_state
            ),
            vim.log.levels.WARN
          )
        else
          local duration_s = action_def.duration_ms and (action_def.duration_ms / 1000) or nil
          local return_state = action_def.return_state
          local is_locked = (action_def.is_locked ~= false)

          entity.ground_y = entity.y
          set_entity_state(entity, target_state)
          entity.action_timer = 0
          entity.action_duration = duration_s
          entity.return_state = return_state
          entity.is_locked = is_locked

          local state_def = entity.manifest.states[target_state]
          if state_def.physics and state_def.physics.jump_impulse_y then
            entity.vy = state_def.physics.jump_impulse_y
          end

          triggered_count = triggered_count + 1
          vim.notify(
            string.format("[Distract] %s (#%d) -> %s", entity.asset_name, entity.id, action_name),
            vim.log.levels.INFO
          )
        end
      end
    end
  end

  if triggered_count == 0 then
    vim.notify(
      string.format("[Distract] Action '%s' not found or matched no active entities", action_name),
      vim.log.levels.WARN
    )
  end
end

function M.handle_editor_event(entities, set_entity_state, event_name, context)
  if type(context) == "table" and type(context.cursor_col) == "number" then
    focus_col = context.cursor_col
  end

  for _, entity in ipairs(entities) do
    if not entity.is_locked and entity.manifest.states then
      local state_def = entity.manifest.states[entity.current_state]
      if state_def and state_def.transitions and state_def.transitions.on_event then
        local next_state = state_def.transitions.on_event[event_name]
        if next_state then
          local changed = entity.current_state ~= next_state
          set_entity_state(entity, next_state)

          if changed and focus_col then
            local next_def = entity.manifest.states[next_state]
            local moves = next_def
              and next_def.physics
              and math.abs(next_def.physics.target_vx or 0) > 0
            if moves then
              face_toward(entity, focus_col)
            end
          end
        end
      end
    end
  end
end

return M
