local M = {}
local uv = vim.uv or vim.loop
local renderer = require("distract.renderer")

local timer = nil
local entities = {}
local entity_counter = 0
local is_running = false
local config = {
  fps = 30,
  backend = "halfblock", -- "halfblock", "float" (in-terminal); "overlay" runs in distract.external
  assets = {},
}

local last_tick_time = nil

-- A render fault repeats every tick, so an unguarded error becomes an error
-- storm at `fps` messages per second that makes the editor unusable. Tolerate a
-- short burst (transient state during a resize, say), then shut down and report
-- once.
local MAX_CONSECUTIVE_RENDER_FAILURES = 5
local consecutive_render_failures = 0

function M.setup(opts)
  if opts then
    config = vim.tbl_deep_extend("force", config, opts)
  end
end

function M.is_running()
  return is_running
end

function M.start()
  if is_running then return end
  is_running = true
  last_tick_time = uv.hrtime()
  consecutive_render_failures = 0

  local tick_rate = math.floor(1000 / (config.fps or 30))
  timer = uv.new_timer()
  timer:start(0, tick_rate, vim.schedule_wrap(function()
    M.tick()
  end))
end

function M.stop()
  if timer then
    timer:stop()
    timer:close()
    timer = nil
  end
  is_running = false
  renderer.clear_all()
  entities = {}
end

function M.spawn(asset_name, opts)
  opts = opts or {}
  asset_name = asset_name or "cat"

  local manifest = config.assets and config.assets[asset_name]
  if not manifest then
    local ok, loaded = pcall(require, "distract.manifests." .. asset_name)
    if ok then
      manifest = loaded
    else
      manifest = require("distract.manifests.cat")
    end
  end

  entity_counter = entity_counter + 1
  local id = entity_counter
  local initial_state = manifest.initial_state or "idle"
  local z_index = manifest.z_index or 10

  local start_x = opts.x or math.floor(vim.o.columns / 2)
  local start_y = opts.y or math.floor(vim.o.lines / 2)
  local flip_x = opts.flip_x or false
  local heading_x = flip_x and -1 or 1

  local entity = {
    id = id,
    asset_name = asset_name,
    manifest = manifest,
    x = start_x,
    y = start_y,
    vx = 0,
    vy = 0,
    target_vx = 0,
    target_vy = 0,
    heading_x = heading_x,
    flip_x = flip_x,
    current_state = initial_state,
    state_time = 0,
    frame_idx = 1,
    frame_timer = 0,
    animation_finished = false,
    is_active = true,
    base_y = start_y,
    ground_y = start_y,
    path_phase = 0,
    action_timer = nil,
    action_duration = nil,
    return_state = nil,
    is_locked = false,
    z_index = z_index,
  }

  -- Apply initial state physics
  local state_def = manifest.states and manifest.states[initial_state]
  if state_def and state_def.physics then
    local p = state_def.physics
    entity.target_vx = (p.target_vx or 0) * heading_x
    entity.target_vy = p.target_vy or 0
    entity.vx = entity.target_vx
    entity.vy = entity.target_vy
    entity.is_locked = state_def.is_locked or false
    if p.ground_y then entity.ground_y = p.ground_y end
  end

  table.insert(entities, entity)

  if not is_running then
    M.start()
  end

  vim.notify(string.format("[Distract] Spawned %s (#%d) [%s] (in-terminal mode)", asset_name, id, initial_state), vim.log.levels.INFO)
  return id
end

function M.set_entity_state(entity, new_state)
  if entity.current_state ~= new_state then
    entity.current_state = new_state
    entity.state_time = 0
    entity.frame_idx = 1
    entity.frame_timer = 0
    entity.animation_finished = false
    entity.base_y = entity.y
    entity.path_phase = 0

    local state_def = entity.manifest.states and entity.manifest.states[new_state]
    if state_def then
      entity.is_locked = state_def.is_locked or false
    end
  end
end

function M.trigger_action(action_name, target)
  local triggered_count = 0

  for _, entity in ipairs(entities) do
    local match = false
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
        local duration_s = action_def.duration_ms and (action_def.duration_ms / 1000) or nil
        local return_state = action_def.return_state
        local is_locked = (action_def.is_locked ~= false)

        entity.ground_y = entity.y
        M.set_entity_state(entity, target_state)
        entity.action_timer = 0
        entity.action_duration = duration_s
        entity.return_state = return_state
        entity.is_locked = is_locked

        -- Apply jump impulse if defined
        local state_def = entity.manifest.states and entity.manifest.states[target_state]
        if state_def and state_def.physics and state_def.physics.jump_impulse_y then
          entity.vy = state_def.physics.jump_impulse_y
        end

        triggered_count = triggered_count + 1
        vim.notify(string.format("[Distract] %s (#%d) -> %s", entity.asset_name, entity.id, action_name), vim.log.levels.INFO)
      end
    end
  end

  if triggered_count == 0 then
    vim.notify(string.format("[Distract] Action '%s' not found or matched no active entities", action_name), vim.log.levels.WARN)
  end
end

function M.handle_editor_event(event_name)
  for _, entity in ipairs(entities) do
    if not entity.is_locked and entity.manifest.states then
      local state_def = entity.manifest.states[entity.current_state]
      if state_def and state_def.transitions and state_def.transitions.on_event then
        local next_state = state_def.transitions.on_event[event_name]
        if next_state then
          M.set_entity_state(entity, next_state)
        end
      end
    end
  end
end

function M.tick()
  local now = uv.hrtime()
  local dt = last_tick_time and ((now - last_tick_time) / 1e9) or 0.033
  last_tick_time = now
  if dt > 0.1 then dt = 0.1 end

  local max_columns = vim.o.columns
  local max_lines = vim.o.lines

  for _, entity in ipairs(entities) do
    entity.state_time = entity.state_time + dt

    -- 1. Action duration timer
    if entity.action_timer and entity.action_duration then
      entity.action_timer = entity.action_timer + dt
      if entity.action_timer >= entity.action_duration then
        entity.action_timer = nil
        entity.action_duration = nil
        entity.is_locked = false
        local next_state = entity.return_state or "idle"
        entity.return_state = nil
        M.set_entity_state(entity, next_state)
      end
    end

    local state_def = entity.manifest.states and entity.manifest.states[entity.current_state]
    if state_def then
      -- 2. State Timeout
      if state_def.transitions and state_def.transitions.timeout_ms and state_def.transitions.on_timeout then
        if entity.state_time * 1000 >= state_def.transitions.timeout_ms then
          M.set_entity_state(entity, state_def.transitions.on_timeout)
        end
      end

      -- 3. Animation frames
      local anim = state_def.animation or { frames = { 0 }, fps = 6, loop_anim = true }
      local frame_count = #(anim.frames or { 0 })
      if frame_count > 0 then
        local frame_duration = (anim.fps and anim.fps > 0) and (1 / anim.fps) or 0.1
        entity.frame_timer = entity.frame_timer + dt

        if entity.frame_timer >= frame_duration then
          entity.frame_timer = entity.frame_timer - frame_duration
          if entity.frame_idx < frame_count then
            entity.frame_idx = entity.frame_idx + 1
          elseif anim.loop_anim ~= false then
            entity.frame_idx = 1
          else
            entity.animation_finished = true
            if state_def.transitions and state_def.transitions.on_finish then
              M.set_entity_state(entity, state_def.transitions.on_finish)
            end
          end
        end
      end

      -- 4. Physics
      local phys = state_def.physics or {}
      local speed_x = math.abs(phys.target_vx or 0)
      entity.target_vx = speed_x * entity.heading_x
      entity.target_vy = phys.target_vy or 0
      entity.flip_x = (entity.heading_x < 0)

      local friction = phys.friction or 0.1
      local lerp_factor = math.min(1.0, math.max(0.05, 1.0 - math.exp(-friction * dt * 30)))
      entity.vx = entity.vx + (entity.target_vx - entity.vx) * lerp_factor

      if (phys.gravity or 0) > 0 then
        entity.vy = entity.vy + (phys.gravity * dt * 30)
        entity.y = entity.y + (entity.vy * dt * 15)

        if entity.y >= entity.ground_y then
          entity.y = entity.ground_y
          entity.vy = 0
        end
      else
        entity.vy = entity.vy + (entity.target_vy - entity.vy) * lerp_factor
        if phys.path_type == "sine" then
          local amp = phys.path_amplitude or 2.0
          local freq = phys.path_frequency or 2.0
          entity.path_phase = entity.path_phase + (dt * freq)
          entity.y = entity.base_y + math.sin(entity.path_phase) * amp
        else
          entity.y = entity.y + (entity.vy * dt * 15)
        end
      end

      entity.x = entity.x + (entity.vx * dt * 15)

      -- 5. Screen boundary modes
      local wrap_mode = phys.wrap_mode or "wrap"
      local sprite_w = 16
      if wrap_mode == "wrap" then
        if entity.vx > 0 and entity.x > max_columns then
          entity.x = -sprite_w
        elseif entity.vx < 0 and entity.x < -sprite_w then
          entity.x = max_columns
        end
      elseif wrap_mode == "bounce" then
        if entity.x <= 0 then
          entity.x = 0
          entity.heading_x = 1
          entity.vx = math.max(0.5, math.abs(entity.vx))
        elseif entity.x + sprite_w >= max_columns then
          entity.x = math.max(0, max_columns - sprite_w)
          entity.heading_x = -1
          entity.vx = -math.max(0.5, math.abs(entity.vx))
        end
      elseif wrap_mode == "clamp" then
        entity.x = math.max(0, math.min(entity.x, max_columns - sprite_w))
        entity.y = math.max(0, math.min(entity.y, max_lines - 4))
      end
    end
  end

  local ok, err = pcall(renderer.draw, entities, config.backend)
  if ok then
    consecutive_render_failures = 0
  else
    consecutive_render_failures = consecutive_render_failures + 1
    -- `==` not `>=`: the counter only crosses the limit once, so the user is
    -- told once even if something keeps calling tick after the shutdown.
    if consecutive_render_failures == MAX_CONSECUTIVE_RENDER_FAILURES then
      M.stop()
      vim.notify(
        "[Distract] Rendering failed repeatedly; engine stopped.\n" .. tostring(err),
        vim.log.levels.WARN
      )
    end
  end
end

function M.get_status()
  if #entities == 0 then
    vim.notify("[Distract] No active entities (in-terminal mode).", vim.log.levels.INFO)
  else
    local lines = { string.format("[Distract] %d active entities (in-terminal mode, backend: %s):", #entities, config.backend) }
    for _, ent in ipairs(entities) do
      table.insert(lines, string.format("  • #%d %s (state: %s, pos: %.0f, %.0f)", ent.id, ent.asset_name, ent.current_state, ent.x, ent.y))
    end
    vim.notify(table.concat(lines, "\n"), vim.log.levels.INFO)
  end
end

function M.despawn(id)
  local initial_len = #entities
  local new_entities = {}
  for _, e in ipairs(entities) do
    if e.id == id then
      renderer.close_window(id)
    else
      table.insert(new_entities, e)
    end
  end
  entities = new_entities
  if #entities < initial_len then
    vim.notify(string.format("[Distract] Despawned entity #%d", id), vim.log.levels.INFO)
  end
end

function M.clear()
  M.stop()
  vim.notify("[Distract] All entities cleared", vim.log.levels.INFO)
end

return M
