--- In-terminal simulation.
---
--- Runs the same state machine and the same physics as the Rust overlay engine,
--- so one manifest describes one behaviour on both backends.
---
--- Units. Manifest positions and velocities are in *sprite pixels*, and
--- velocities are per frame at 60 FPS. One sprite pixel is one terminal cell
--- wide and half a terminal cell tall, which is what the half-block renderer
--- draws. This module therefore converts sprite pixels to cells on the way out;
--- the overlay engine multiplies by its own pixels-per-sprite-pixel scale. The
--- two used to apply unrelated ad-hoc factors (`dt * 60` against `dt * 15` and
--- `dt * 30`), so the same manifest moved at different speeds on each backend.

local M = {}
local uv = vim.uv or vim.loop
local renderer = require("distract.renderer")
local sprites = require("distract.terminal_sprites")

local entity_spawn = require("distract.entity_spawn")
local engine_world = require("distract.engine_world")
local obstacles = require("distract.obstacles")
local plugins = require("distract.plugins")
local viewport = require("distract.viewport")
local visibility = require("distract.visibility")

local timer = nil
local entities = {}
local entity_counter = 0
local is_running = false
local config = {
  fps = 30,
  backend = "halfblock",
  assets = {},
}

local last_tick_time = nil

-- A render fault repeats every tick, so an unguarded error becomes an error
-- storm at `fps` messages per second that makes the editor unusable. Tolerate a
-- short burst (transient state during a resize, say), then shut down and report
-- once.
local MAX_CONSECUTIVE_RENDER_FAILURES = 5
local consecutive_render_failures = 0

--- Whether the settled pose has already been painted.
---
--- Quiescence must not skip the frame that *reaches* the resting pose, only
--- the identical frames after it, so the first quiescent tick still draws.
local quiescent_drawn = false

--- The capability gate, shared with `external.lua` so the same manifest is
--- refused with the same words on either backend. Placement now travels with
--- entity construction, in `entity_spawn.lua`.
local locomotion = require("distract.locomotion")

--- Re-exported for tests and for anyone reading a manifest by hand.
M.effective_locomotion = locomotion.effective_locomotion
M.validate_capabilities = locomotion.validate

function M.setup(opts)
  if opts then
    config = vim.tbl_deep_extend("force", config, opts)
  end
  sprites.configure(config)
end

function M.is_running()
  return is_running
end

function M.start()
  if is_running then
    return
  end
  is_running = true
  last_tick_time = uv.hrtime()
  consecutive_render_failures = 0
  plugins.bind_world({ backend = config.backend, entities = M.get_entities })

  local tick_rate = math.floor(1000 / (config.fps or 30))
  timer = uv.new_timer()
  timer:start(
    0,
    tick_rate,
    vim.schedule_wrap(function()
      M.tick()
    end)
  )
end

function M.stop()
  if timer then
    timer:stop()
    timer:close()
    timer = nil
  end
  is_running = false
  plugins.dispatch_teardown()
  plugins.unbind_world()
  renderer.clear_all()
  entities = {}
  quiescent_drawn = false
end

--- How close two floors must be to count as the same one, in cells.
local FLOOR_MATCH_EPSILON_CELLS = 1e-6

--- The floor entities were last placed against, in cells.
---
--- Held so a screen that changes shape can re-seat what was standing on the old
--- floor. Nil until the first spawn or the first `set_ground_row`.
local floor_row = nil

function M.spawn(asset_name, opts)
  opts = opts or {}
  asset_name = asset_name or "cat"

  local manifest = entity_spawn.resolve_manifest(asset_name, config.assets)

  -- Checked here rather than per frame, and before anything is allocated: a
  -- manifest that cannot work is worth one message when it arrives, not thirty
  -- a second forever.
  local violation = M.validate_capabilities(manifest)
  if violation then
    vim.notify(
      string.format("[Distract] Cannot spawn '%s': %s.", asset_name, violation),
      vim.log.levels.ERROR
    )
    return nil
  end

  -- Art follows the manifest here as it does on the overlay: an asset pointing
  -- at a GIF draws that GIF. Bound before the placement is resolved, because
  -- the sprite's own size is what the anchors and the floor measure against.
  sprites.bind_manifest(asset_name, manifest)

  entity_counter = entity_counter + 1
  local initial_state = manifest.initial_state or "idle"
  local initial_def = manifest.states and manifest.states[initial_state]

  local entity = entity_spawn.build({
    id = entity_counter,
    asset_name = asset_name,
    manifest = manifest,
    flip_x = opts.flip_x or false,
    placement = entity_spawn.placement({
      asset_name = asset_name,
      manifest = manifest,
      initial_def = initial_def,
      opts = opts,
      config = config,
      floor_row = floor_row,
    }),
  })

  table.insert(entities, entity)
  -- A new entity may itself be perfectly still. Without this, an idle screen
  -- that had already been marked drawn would skip its very first frame.
  quiescent_drawn = false

  if not is_running then
    M.start()
  end

  vim.notify(
    string.format(
      "[Distract] Spawned %s (#%d) [%s] (in-terminal mode)",
      asset_name,
      entity.id,
      initial_state
    ),
    vim.log.levels.INFO
  )
  return entity.id
end

--- Moves the floor, re-seating whatever was standing on the old one.
---
--- Mirrors `World::set_ground_row`. Only entities whose floor *is* the previous
--- world floor move: a manifest floor and the anchor a jump takes are their
--- own, and a screen that changed shape has nothing to say about either. An
--- entity already resting is carried down with the floor rather than left
--- hanging in the air until gravity notices.
---@param row number|nil the new floor in terminal cells, or nil for none
--- Replaces the obstacles entities interact with.
---
--- Held in `distract.obstacles`, which both this engine and the collection side
--- read, so there is one list rather than a copy per backend. Repainting is
--- forced because a platform that moved changes where a resting entity stands,
--- and a resting entity is exactly what quiescence suppresses.
---@param rects table[] rectangles in terminal cells
function M.set_obstacles(rects)
  obstacles.set_rects(rects)
  quiescent_drawn = false
end

--- Shows or hides what is already on screen.
---
--- Hiding closes the surfaces rather than leaving them in place, because an
--- in-terminal float is drawn by the terminal emulator and stays visible over
--- whatever the user switched to. Showing forces the next tick to repaint, since
--- the resting pose it had already painted is now gone.
---@param is_visible boolean
function M.set_visible(is_visible)
  if is_visible then
    quiescent_drawn = false
    return
  end
  renderer.clear_all()
end

function M.set_ground_row(row)
  if row ~= nil and type(row) ~= "number" then
    return
  end
  local previous = floor_row
  floor_row = row
  engine_world.update_floor(entities, previous, row, entity_spawn.sprite_cell_size)
  quiescent_drawn = false
end

--- The floor entities are placed against, in cells, or nil before the first
--- spawn. For tests and diagnostics.
function M.get_ground_row()
  return floor_row
end

function M.set_entity_state(entity, new_state)
  if entity.current_state ~= new_state then
    local previous_state = entity.current_state
    -- A new state brings new art even when the entity does not move, so the
    -- settled pose that was painted no longer reflects the world.
    quiescent_drawn = false
    entity.current_state = new_state
    entity.state_time = 0
    entity.frame_idx = 1
    entity.frame_timer = 0
    entity.animation_finished = false
    entity.base_x = entity.x
    entity.base_y = entity.y
    entity.path_phase = 0

    local state_def = entity.manifest.states and entity.manifest.states[new_state]
    if state_def then
      entity.is_locked = state_def.is_locked or false
    end

    plugins.dispatch_state_change(entity, previous_state, new_state)
  end
end

local engine_actions = require("distract.engine_actions")
local engine_quiescence = require("distract.engine_quiescence")

function M.trigger_action(action_name, target)
  return engine_actions.trigger_action(entities, M.set_entity_state, action_name, target)
end

--- @param event_name string
--- @param context table|nil optional `{ cursor_col = n, cursor_row = n }`
function M.handle_editor_event(event_name, context)
  return engine_actions.handle_editor_event(entities, M.set_entity_state, event_name, context)
end

--- Advances the simulation by an explicit `dt` against explicit screen bounds.
---
--- Split out of `tick`, which reads the clock and `vim.o` inline and so left
--- tests able to assert only on direction, never on distance. Everything the
--- overlay's `World::update` does happens here and nothing else does, which is
--- what lets the two engines be compared against one set of trajectories.
---
function M.step(dt, bounds)
  entities =
    engine_world.step(entities, dt, bounds, M.set_entity_state, entity_spawn.sprite_cell_size)
end

--- Wall-clock driver: measures `dt`, supplies the current screen size, and
--- renders what `step` produced.
function M.tick()
  local now = uv.hrtime()
  local dt = last_tick_time and ((now - last_tick_time) / 1e9) or 0.033
  last_tick_time = now
  -- A long pause -- a resumed session, a blocking command -- would otherwise
  -- teleport every entity clear across the screen in a single frame.
  if dt > 0.1 then
    dt = 0.1
  end

  if #entities == 0 then
    return
  end

  M.step(dt, viewport.bounds())

  -- Stepped, deliberately, before this returns: hiding is a drawing decision.
  if not visibility.is_visible() then
    return
  end

  -- Quiescence gates the *redraw*, never the step, matching how `ecs.rs` uses
  -- it: `World::update` always runs there. Stepping is arithmetic; drawing is
  -- the ~92 API calls per entity that this exists to avoid. Gating the step as
  -- well meant an entity that needed a boundary wrap never got one.
  local settled = M.is_quiescent()
  if settled and quiescent_drawn then
    return
  end

  local ok, err = pcall(renderer.draw, entities, config.backend)
  if ok then
    consecutive_render_failures = 0
    plugins.dispatch_draw(renderer.placements())
    -- A failed draw must not count as having painted the resting pose.
    quiescent_drawn = settled
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
  local lines = engine_world.format_status(entities, config.backend)
  vim.notify(table.concat(lines, "\n"), vim.log.levels.INFO)
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
  quiescent_drawn = false
  if #entities < initial_len then
    vim.notify(string.format("[Distract] Despawned entity #%d", id), vim.log.levels.INFO)
  end
end

--- Removes every entity but leaves the engine running.
---
--- This matches the overlay backend's `ClearAll`. `:DistractClear` used to mean
--- "clear and stop" here and "clear" there, so the same command left the plugin
--- in a different state depending on the backend. `tick` returns immediately
--- while nothing is alive, so an idle engine costs nothing.
function M.clear()
  renderer.clear_all()
  entities = {}
  quiescent_drawn = false
  vim.notify("[Distract] All entities cleared", vim.log.levels.INFO)
end

function M.is_quiescent()
  return engine_quiescence.is_quiescent(entities)
end

--- Live entities, for tests and diagnostics.
function M.get_entities()
  return entities
end

return M
