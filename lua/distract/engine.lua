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

local entity_step = require("distract.entity_step")
local kinematics = require("distract.kinematics")
local obstacles = require("distract.obstacles")
local plugins = require("distract.plugins")
local viewport = require("distract.viewport")
local visibility = require("distract.visibility")

--- Sprite pixels per terminal cell.
local CELLS_PER_SPRITE_PX_X = kinematics.CELLS_PER_SPRITE_PX_X
local CELLS_PER_SPRITE_PX_Y = kinematics.CELLS_PER_SPRITE_PX_Y
--- Reference frame rate the manifest velocities are expressed against.
local REFERENCE_FPS = kinematics.REFERENCE_FPS

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

--- Locomotion classes and the capability gate, shared with `external.lua` so
--- the same manifest is refused with the same words on either backend.
local locomotion = require("distract.locomotion")
local BALLISTIC = locomotion.BALLISTIC

--- Placement, floors and parallax, shared with `external.lua` for the same
--- reason: one manifest and one `position` config place an entity the same way
--- on either backend.
local position = require("distract.position")

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

--- Size of an asset's sprite in terminal cells.
local function sprite_cell_size(asset_name)
  local ok, w, h = pcall(sprites.get_dimensions, asset_name)
  if not ok or not w then
    return 16, 8
  end
  return w * CELLS_PER_SPRITE_PX_X, h * CELLS_PER_SPRITE_PX_Y
end

--- Where one spawn lands, how deep it is, and what it stands on.
---
--- The floor is whatever was last pushed in, exactly as it is on the overlay:
--- only the editor can see `cmdheight`, the statusline and where the text ends,
--- so only the editor measures, and both engines are told. A spawn naming its
--- own `ground` is the one case that measures here, because it is asking about
--- a surface the pushed floor does not describe.
local function resolve_placement(asset_name, manifest, initial_def, opts)
  local settings = position.settings(config.position, opts)
  local spawn_floor_row = opts.ground and position.floor_row(settings.ground) or floor_row

  local _, sprite_h = sprite_cell_size(asset_name)
  return position.placement({
    settings = settings,
    backend = config.backend,
    locomotion = locomotion.locomotion_for(manifest, initial_def),
    declared_anchor = position.manifest_anchor(manifest),
    floor_row = spawn_floor_row,
    sprite_h = sprite_h,
    bounds = viewport.bounds(),
    opts = opts,
  })
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
      -- Reported rather than silently substituted: spawning a typo used to
      -- produce a working-looking cat under the name you asked for.
      vim.notify(
        string.format(
          "[Distract] No manifest for asset '%s'; using the cat's behaviour. "
            .. "Define it in setup({ assets = { %s = ... } }).",
          asset_name,
          asset_name
        ),
        vim.log.levels.WARN
      )
      manifest = require("distract.manifests.cat")
    end
  end

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
  local id = entity_counter
  local initial_state = manifest.initial_state or "idle"

  local initial_def = manifest.states and manifest.states[initial_state]
  local placement = resolve_placement(asset_name, manifest, initial_def, opts)

  -- `z` is draw order as well as depth, and it wins over the manifest's
  -- `z_index` when a spawn asks for one.
  local z_index = placement.z and math.floor(placement.z + 0.5) or manifest.z_index or 10

  local start_x = placement.x
  local start_y = placement.y
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
    -- Where a path primitive anchors its x axis, re-taken on every state
    -- change. `base_y` has always existed for `sine`; the paths that write x
    -- need the other half of the same idea.
    base_x = start_x,
    base_y = start_y,
    ground_y = placement.ground_y or start_y,
    path_phase = 0,
    action_timer = nil,
    action_duration = nil,
    return_state = nil,
    is_locked = false,
    z_index = z_index,
    z = placement.z or 0,
    parallax = placement.parallax,
  }

  -- Apply initial state physics
  local state_def = initial_def
  if state_def and state_def.physics then
    local p = state_def.physics
    entity.target_vx = (p.target_vx or 0) * heading_x
    entity.target_vy = p.target_vy or 0
    entity.vx = entity.target_vx
    entity.vy = entity.target_vy
    entity.is_locked = state_def.is_locked or false
    if p.ground_y then
      entity.ground_y = p.ground_y
    end
  end

  -- Desynchronise from anything already alive. Two cats spawned together
  -- otherwise share a frame index, a frame timer and a path phase for the rest
  -- of their lives, which reads as a chorus line rather than as two animals.
  local anim = state_def and state_def.animation
  local frame_count = (anim and anim.frames and #anim.frames) or 1
  entity.frame_idx = math.random(1, math.max(1, frame_count))
  entity.frame_timer = math.random() * 0.1
  entity.path_phase = math.random() * 2 * math.pi

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
      id,
      initial_state
    ),
    vim.log.levels.INFO
  )
  return id
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

--- Turns an entity to face a point, if it is not already facing it.
local function face_toward(entity, target_x)
  local dx = target_x - entity.x
  if math.abs(dx) < 1 then
    return
  end
  entity.heading_x = dx > 0 and 1 or -1
  entity.flip_x = entity.heading_x < 0
end

--- Where the user is working, in terminal cells, if known.
local focus_col = nil

function M.trigger_action(action_name, target)
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
        -- The Rust side gets this for free because serde requires the field.
        -- On this side a custom action missing `target_state` used to set
        -- `current_state = nil`, and the next tick failed the state lookup.
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
          M.set_entity_state(entity, target_state)
          entity.action_timer = 0
          entity.action_duration = duration_s
          entity.return_state = return_state
          entity.is_locked = is_locked

          -- Apply jump impulse if defined
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

--- @param event_name string
--- @param context table|nil optional `{ cursor_col = n, cursor_row = n }`
function M.handle_editor_event(event_name, context)
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
          M.set_entity_state(entity, next_state)

          -- Orient toward the cursor when picking up a new behaviour, so the
          -- entity looks like it noticed.
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

--- Advances the simulation by an explicit `dt` against explicit screen bounds.
---
--- Split out of `tick`, which reads the clock and `vim.o` inline and so left
--- tests able to assert only on direction, never on distance. Everything the
--- overlay's `World::update` does happens here and nothing else does, which is
--- what lets the two engines be compared against one set of trajectories.
---
---@param dt number seconds elapsed since the previous step
---@param bounds table `{ columns = number, lines = number }`, in terminal cells
--- The entity with this id, or nil.
local function find_entity(id)
  for _, candidate in ipairs(entities) do
    if candidate.id == id then
      return candidate
    end
  end
  return nil
end

--- Applies what plugin hooks asked for since the previous step.
---
--- Requests are applied here rather than where they were made so a hook cannot
--- observe an entity halfway through its own frame, and so the overlay backend
--- can forward the identical queue over IPC and reach the same result.
---@return boolean whether an entity was deactivated and needs collecting
local function apply_plugin_commands()
  local deactivated = false
  for _, command in ipairs(plugins.drain_commands()) do
    local entity = find_entity(command.id)
    if entity then
      if command.kind == "state" then
        M.set_entity_state(entity, command.state)
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

function M.step(dt, bounds)
  if #entities == 0 then
    return
  end

  local requested_despawn = apply_plugin_commands()

  -- The bounds carry an origin, because a scoped viewport is a rectangle
  -- somewhere inside the editor grid rather than the grid itself. Absent, they
  -- describe the whole screen and every existing trajectory is unchanged.
  local min_col = bounds.col or 0
  local min_row = bounds.row or 0
  local max_columns = min_col + bounds.columns
  local max_lines = min_row + bounds.lines
  local step = dt * REFERENCE_FPS

  local despawned = requested_despawn
  local collisions = {}
  -- Read once for the step rather than per entity: the list is replaced
  -- wholesale by the provider cadence, never mutated mid-step.
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
        set_state = M.set_entity_state,
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
    entities = kept
  end
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
  if #entities == 0 then
    vim.notify("[Distract] No active entities (in-terminal mode).", vim.log.levels.INFO)
  else
    local lines = {
      string.format(
        "[Distract] %d active entities (in-terminal mode, backend: %s):",
        #entities,
        config.backend
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

--- Whether anything in the world can still change without further input.
---
--- Mirrors `World::is_quiescent` in `ecs.rs` field for field, including the
--- two results that read oddly on their own: an *inactive* entity is not
--- quiescent, because it is still waiting to be despawned, and an empty world
--- is, because it has nothing to draw.
---
--- Without this, `tick` returned early only when no entity existed at all, so
--- a screen of sleeping cats woke the editor loop 30 times a second forever.
function M.is_quiescent()
  -- A plugin that drew a layer, or asked for a change, has produced a picture
  -- the simulation alone would not have: quiescence must not suppress it.
  if plugins.consume_dirty() then
    return false
  end

  for _, e in ipairs(entities) do
    if not e.is_active then
      return false
    end
    if e.action_timer then
      return false
    end
    if math.abs(e.vx) > 0.001 or math.abs(e.vy) > 0.001 then
      return false
    end

    local states = e.manifest and e.manifest.states
    local state_def = states and states[e.current_state]
    if state_def then
      -- A multi-frame animation, a pending timeout or a path all keep
      -- producing new pictures with no further input.
      local anim = state_def.animation
      if anim and anim.frames and #anim.frames > 1 then
        return false
      end
      local transitions = state_def.transitions
      if transitions and transitions.timeout_ms then
        return false
      end
      -- `linear` is the exception: it overrides no position, so it produces no
      -- picture that velocity alone would not.
      local path_type = state_def.physics and state_def.physics.path_type
      if path_type and path_type ~= "linear" then
        return false
      end
    end
  end
  return true
end

--- Live entities, for tests and diagnostics.
function M.get_entities()
  return entities
end

return M
