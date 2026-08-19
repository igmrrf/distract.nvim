--- One entity, one simulated frame.
---
--- Extracted from `engine.lua`'s `M.step`, which had grown past 200 lines and was
--- the reason that module sat over a 400-line cap. Verbatim: the same arithmetic in
--- the same order, and the physics-parity fixtures are the characterization tests
--- that say so -- a golden that moved would mean the move changed behaviour.
---
--- Everything the frame needs and this module cannot see is passed in rather than
--- reached for: the bounds, the obstacle list, the collision sink, and the two
--- engine-owned callbacks -- `set_state`, which has to run the engine's own
--- transition bookkeeping, and `sprite_cell_size`, which resolves an asset's
--- footprint through the sprite cache.
---
--- `M.advance` is one long function on purpose. Its five numbered steps look like
--- five functions and are not separable: they share a dozen locals -- the
--- footprint, the resolved physics, the per-axis cell scales -- and each step reads
--- what the previous one wrote, so splitting them means threading that state
--- through five signatures or rebuilding it five times. The numbered comments are
--- the structure. `../../CLAUDE.md` §5 covers this case: a cap is a signal to
--- decompose, not a reason to fragment a unit that has to be read as one.
---
--- Units are terminal cells, as everywhere else in this engine.

local M = {}

local kinematics = require("distract.kinematics")
local locomotion = require("distract.locomotion")
local obstacles = require("distract.obstacles")

local CELLS_PER_SPRITE_PX_X = kinematics.CELLS_PER_SPRITE_PX_X
local CELLS_PER_SPRITE_PX_Y = kinematics.CELLS_PER_SPRITE_PX_Y
local BALLISTIC = locomotion.BALLISTIC

---@class DistractFrame
---@field dt number seconds this frame covers
---@field step number `dt` in frames at the reference 60 FPS
---@field min_col number left edge of the bounds, in cells
---@field min_row number top edge
---@field max_columns number right edge
---@field max_lines number bottom edge
---@field obstacle_rects table[] platforms and hazards, in cells
---@field collisions table[] appended to, drained by the caller
---@field set_state function `(entity, state)`
---@field sprite_cell_size function `(asset_name) -> width, height`

--- Advances one entity, reporting whether it left the world.
---@param entity table
---@param frame DistractFrame
---@return boolean whether the entity deactivated itself and needs collecting
function M.advance(entity, frame)
  local dt, step = frame.dt, frame.step
  local min_col, min_row = frame.min_col, frame.min_row
  local max_columns, max_lines = frame.max_columns, frame.max_lines
  local obstacle_rects = frame.obstacle_rects
  local collisions = frame.collisions
  local set_state = frame.set_state
  local sprite_cell_size = frame.sprite_cell_size
  local despawned = false

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
      set_state(entity, next_state)
    end
  end

  local state_def = entity.manifest.states and entity.manifest.states[entity.current_state]
  if state_def then
    -- 2. State Timeout
    if
      state_def.transitions
      and state_def.transitions.timeout_ms
      and state_def.transitions.on_timeout
    then
      if entity.state_time * 1000 >= state_def.transitions.timeout_ms then
        set_state(entity, state_def.transitions.on_timeout)
      end
    end

    -- 3. Animation frames
    local anim = state_def.animation or { frames = { 0 }, fps = 6, loop_anim = true }
    local frame_count = #(anim.frames or { 0 })
    if frame_count > 0 then
      local frame_duration = kinematics.frame_duration_seconds(entity, anim)
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
            set_state(entity, state_def.transitions.on_finish)
          end
        end
      end
    end

    -- 4. Physics, in the shared manifest unit (sprite pixels per 60 FPS frame)
    --
    -- Parallax damps the displacement rather than the velocity itself:
    -- damping the stored velocity every frame would decay it to zero instead
    -- of moving a distant thing slower at a steady speed.
    local parallax = entity.parallax or 1.0
    local cells_x = step * CELLS_PER_SPRITE_PX_X * parallax
    local cells_y = step * CELLS_PER_SPRITE_PX_Y * parallax
    -- The footprint every surface and boundary is measured against. Sizes come
    -- from the asset rather than a constant: the built-in sprites are 24 cells
    -- wide (cat, crab) and 16 (sun), so a hardcoded 16 measured in the wrong
    -- place. Parallax shrinks the drawn art, so the footprint shrinks with it.
    local sprite_w, sprite_h = sprite_cell_size(entity.asset_name)
    sprite_w, sprite_h = sprite_w * parallax, sprite_h * parallax
    local phys = state_def.physics or {}
    local speed_x = math.abs(phys.target_vx or 0)
    entity.target_vx = speed_x * entity.heading_x
    entity.target_vy = phys.target_vy or 0
    entity.flip_x = (entity.heading_x < 0)

    local friction = phys.friction or 0.05
    local lerp_factor = math.min(1.0, math.max(0.01, 1.0 - math.exp(-friction * step)))
    entity.vx = entity.vx + (entity.target_vx - entity.vx) * lerp_factor
    -- Constant acceleration, on top of the pull toward `target_vx`. `gravity`
    -- is the same thing on the y axis under a name that also brings a floor
    -- with it; `accel_y` is the floorless version.
    entity.vx = entity.vx + ((phys.accel_x or 0) * step)

    if (phys.gravity or 0) > 0 then
      -- Read before the integration: an entity already resting on the floor
      -- is re-accelerated by gravity and caught by the clamp on every single
      -- tick, so "the clamp ran" is not a landing. Crossing the floor from
      -- above is.
      local feet_before = entity.y + sprite_h
      entity.vy = entity.vy + (phys.gravity * step)
      entity.y = entity.y + (entity.vy * cells_y)

      -- A registered platform is a floor the entity reaches earlier, so the
      -- surface for this frame is whichever is higher. With no obstacles this is
      -- the floor exactly, and the arithmetic is the ground clamp it replaces.
      local floor_feet = entity.ground_y + sprite_h
      local surface = floor_feet
      local platform_top = obstacles.crossed_platform(obstacle_rects, {
        left = entity.x,
        top = entity.y,
        width = sprite_w,
        height = sprite_h,
      }, feet_before)
      if platform_top and platform_top < surface then
        surface = platform_top
      end
      local was_airborne = feet_before < surface

      if entity.y + sprite_h >= surface then
        entity.y = surface - sprite_h
        local landed = was_airborne and entity.vy > 0
        entity.vy = 0
        if landed then
          table.insert(collisions, { entity = entity, edge = "bottom" })
        end
        if landed and locomotion.effective_locomotion(phys) == BALLISTIC then
          local on_land = state_def.transitions and state_def.transitions.on_land
          if on_land then
            -- Landing ends the action that launched the entity. Leaving its
            -- timer running would drag the entity out of the state it just
            -- reached as soon as the clock caught up, so a jump that lands
            -- early would still be locked until its declared duration.
            entity.action_timer = nil
            entity.action_duration = nil
            entity.return_state = nil
            entity.is_locked = false
            set_state(entity, on_land)
          end
        end
      end
    else
      entity.vy = entity.vy + (entity.target_vy - entity.vy) * lerp_factor
      entity.vy = entity.vy + ((phys.accel_y or 0) * step)
      entity.y = entity.y + (entity.vy * cells_y)
    end

    entity.x = entity.x + (entity.vx * cells_x)

    -- A path is a positional *override*, applied after integration so it
    -- replaces the velocity result on the axes it owns and leaves the others
    -- alone. Gravity is excluded: a path that writes y fights the floor,
    -- which is what the locomotion classes exist to keep apart.
    if (phys.gravity or 0) <= 0 then
      kinematics.apply_path(entity, phys, dt)
    end

    -- A grounded state has no gravity to fall under, so which surface it stands
    -- on is resolved rather than integrated. Only reached while obstacles exist:
    -- without them the answer is the floor it was seated on.
    if
      #obstacle_rects > 0
      and (phys.gravity or 0) <= 0
      and entity.ground_y
      and locomotion.locomotion_for(entity.manifest, state_def) == locomotion.GROUNDED
    then
      entity.y = obstacles.standing_surface(obstacle_rects, {
        left = entity.x,
        top = entity.y,
        width = sprite_w,
        height = sprite_h,
      }, entity.ground_y + sprite_h) - sprite_h
    end

    local deflection = obstacles.deflection(obstacle_rects, {
      left = entity.x,
      top = entity.y,
      width = sprite_w,
      height = sprite_h,
    }, entity.heading_x)
    if deflection then
      entity.x = deflection.x
      entity.heading_x = deflection.heading_x
      entity.vx = math.abs(entity.vx) * entity.heading_x
      entity.flip_x = entity.heading_x < 0
      table.insert(collisions, { entity = entity, edge = "obstacle" })
    end

    -- 5. Screen boundary modes.
    local wrap_mode = phys.wrap_mode or "wrap"

    local edges = state_def.transitions or {}

    if wrap_mode == "wrap" then
      -- Gated on position, not on velocity: `vx` lerps toward its target, so
      -- a state whose target is zero decays it through zero and an entity
      -- that had already left the screen would never wrap back.
      if entity.x > max_columns then
        entity.x = min_col - sprite_w
      elseif entity.x < min_col - sprite_w then
        entity.x = max_columns
      end
      -- Vertical wrap too. The overlay has always wrapped both axes, so a
      -- manifest with vertical motion described one behaviour there and a
      -- different one here.
      if entity.y > max_lines then
        entity.y = min_row - sprite_h
      elseif entity.y < min_row - sprite_h then
        entity.y = max_lines
      end
    elseif wrap_mode == "bounce" then
      if entity.x <= min_col then
        entity.x = min_col
        entity.heading_x = 1
        entity.vx = math.max(0.5, math.abs(entity.vx))
        entity.flip_x = false
        table.insert(collisions, { entity = entity, edge = "left" })
        if edges.on_edge_left then
          set_state(entity, edges.on_edge_left)
        end
      elseif entity.x + sprite_w >= max_columns then
        entity.x = math.max(min_col, max_columns - sprite_w)
        entity.heading_x = -1
        entity.vx = -math.max(0.5, math.abs(entity.vx))
        entity.flip_x = true
        table.insert(collisions, { entity = entity, edge = "right" })
        if edges.on_edge_right then
          set_state(entity, edges.on_edge_right)
        end
      end

      if entity.vy ~= 0 then
        if entity.y <= min_row then
          entity.y = min_row
          entity.vy = math.abs(entity.vy)
          table.insert(collisions, { entity = entity, edge = "top" })
        elseif entity.y + sprite_h >= max_lines then
          entity.y = math.max(min_row, max_lines - sprite_h)
          entity.vy = -math.abs(entity.vy)
          table.insert(collisions, { entity = entity, edge = "bottom" })
        end
      end
    elseif wrap_mode == "clamp" then
      entity.x = math.max(min_col, math.min(entity.x, max_columns - sprite_w))
      -- No `- 1` on the ceiling: the overlay clamps at `viewport_h - frame_h`
      -- and `bounce` above clamps at `max_lines - sprite_h`, so the stray row
      -- made `clamp` disagree with both the other engine and its own
      -- neighbouring branch. Reserving space for the statusline is the floor
      -- system's job, where it is computed from `cmdheight` and `laststatus`
      -- rather than guessed at as a constant.
      entity.y = math.max(min_row, math.min(entity.y, max_lines - sprite_h))
    elseif wrap_mode == "despawn" then
      if
        entity.x < min_col - sprite_w
        or entity.x > max_columns
        or entity.y < min_row - sprite_h
        or entity.y > max_lines
      then
        entity.is_active = false
        despawned = true
      end
    end
    -- "none" deliberately applies no boundary handling.
  end

  return despawned
end

return M
