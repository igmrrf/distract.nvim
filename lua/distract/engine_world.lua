local M = {}

local entity_step = require("distract.entity_step")
local kinematics = require("distract.kinematics")
local obstacles = require("distract.obstacles")
local plugins = require("distract.plugins")
local renderer = require("distract.renderer")

local FLOOR_MATCH_EPSILON_CELLS = 1e-6

function M.update_floor(entities, previous, row, sprite_cell_size)
  if not previous or not row or previous == row then
    return
  end
  for _, entity in ipairs(entities) do
    local _, sprite_h = sprite_cell_size(entity.asset_name)
    sprite_h = sprite_h * (entity.parallax or 1.0)
    local was = previous - sprite_h
    if math.abs(entity.ground_y - was) < FLOOR_MATCH_EPSILON_CELLS then
      local is_resting = entity.y >= was - FLOOR_MATCH_EPSILON_CELLS
      entity.ground_y = row - sprite_h
      if is_resting then
        entity.y = entity.ground_y
      end
    end
  end
end

local function find_entity(entities, id)
  for _, candidate in ipairs(entities) do
    if candidate.id == id then
      return candidate
    end
  end
  return nil
end

function M.apply_plugin_commands(entities, set_entity_state)
  local deactivated = false
  for _, command in ipairs(plugins.drain_commands()) do
    local entity = find_entity(entities, command.id)
    if entity then
      if command.kind == "state" then
        set_entity_state(entity, command.state)
      elseif command.kind == "impulse" then
        entity.vx = entity.vx + command.vx
        entity.vy = entity.vy + command.vy
      elseif command.kind == "despawn" then
        entity.is_active = false
        deactivated = true
      end
    end
  end
  return deactivated
end

function M.step(entities, dt, bounds, set_entity_state, sprite_cell_size)
  if #entities == 0 then
    return entities
  end

  local requested_despawn = M.apply_plugin_commands(entities, set_entity_state)

  local min_col = bounds.col or 0
  local min_row = bounds.row or 0
  local max_columns = min_col + bounds.columns
  local max_lines = min_row + bounds.lines
  local step = dt * kinematics.REFERENCE_FPS

  local despawned = requested_despawn
  local collisions = {}
  local obstacle_rects = obstacles.rects()

  for _, entity in ipairs(entities) do
    if
      entity_step.advance(entity, {
        dt = dt,
        step = step,
        min_col = min_col,
        min_row = min_row,
        max_columns = max_columns,
        max_lines = max_lines,
        obstacle_rects = obstacle_rects,
        collisions = collisions,
        set_state = set_entity_state,
        sprite_cell_size = sprite_cell_size,
      })
    then
      despawned = true
    end
  end

  for _, entity in ipairs(entities) do
    plugins.dispatch_tick(entity, dt)
  end
  for _, collision in ipairs(collisions) do
    plugins.dispatch_collision(collision.entity, { edge = collision.edge, target = nil })
  end

  if despawned then
    local kept = {}
    for _, e in ipairs(entities) do
      if e.is_active then
        table.insert(kept, e)
      else
        renderer.close_window(e.id)
        vim.notify(
          string.format("[Distract] Despawned entity #%d (left the screen)", e.id),
          vim.log.levels.INFO
        )
      end
    end
    return kept
  end

  return entities
end

function M.format_status(entities, backend)
  if #entities == 0 then
    return { "[Distract] No active entities (in-terminal mode)." }
  end
  local lines = {
    string.format(
      "[Distract] %d active entities (in-terminal mode, backend: %s):",
      #entities,
      backend
    ),
  }
  for _, ent in ipairs(entities) do
    table.insert(
      lines,
      string.format(
        "  • #%d %s (state: %s, pos: %.0f, %.0f)",
        ent.id,
        ent.asset_name,
        ent.current_state,
        ent.x,
        ent.y
      )
    )
  end
  return lines
end

return M
