--- Entity construction for the in-terminal engine.
---
--- `engine.spawn` coordinates the steps; this module decides what a new entity
--- *is*. Split out because the manifest lookup, the placement, the physics seed
--- and the desynchronisation all shared one function's locals, which is what
--- kept `engine.lua` over the module cap and left the most consequential step in
--- the engine reachable only through a spawn.
---
--- Mirrors `Entity::new` and `spawn.rs` on the overlay: the same fields, seeded
--- the same way, so one manifest describes one entity on both backends.

local M = {}

local locomotion = require("distract.locomotion")
local position = require("distract.position")
local sprites = require("distract.terminal_sprites")
local viewport = require("distract.viewport")
local kinematics = require("distract.kinematics")

--- Footprint assumed when an asset's art cannot be measured, in terminal cells.
local FALLBACK_CELL_WIDTH = 16
local FALLBACK_CELL_HEIGHT = 8
--- Draw order for a manifest that declares none.
local DEFAULT_Z_INDEX = 10
--- Widest desynchronising offset into a frame's dwell, in seconds.
local FRAME_TIMER_JITTER_SECONDS = 0.1

--- Size of an asset's sprite in terminal cells.
---@param asset_name string
---@return number width_cells
---@return number height_cells
function M.sprite_cell_size(asset_name)
  local ok, sprite_w, sprite_h = pcall(sprites.get_dimensions, asset_name)
  if not ok or not sprite_w then
    return FALLBACK_CELL_WIDTH, FALLBACK_CELL_HEIGHT
  end
  return sprite_w * kinematics.CELLS_PER_SPRITE_PX_X, sprite_h * kinematics.CELLS_PER_SPRITE_PX_Y
end

--- Finds the manifest a spawn should use, reporting a fallback rather than
--- substituting one silently.
---
--- Spawning a typo used to produce a working-looking cat under the name asked
--- for, which is indistinguishable from a manifest that loaded and misbehaved.
---@param asset_name string
---@param assets table<string, table> the configured manifest registry
---@return table manifest
function M.resolve_manifest(asset_name, assets)
  local registered = assets and assets[asset_name]
  if registered then
    return registered
  end

  local ok, loaded = pcall(require, "distract.manifests." .. asset_name)
  if ok then
    return loaded
  end

  vim.notify(
    string.format(
      "[Distract] No manifest for asset '%s'; using the cat's behaviour. "
        .. "Define it in setup({ assets = { %s = ... } }).",
      asset_name,
      asset_name
    ),
    vim.log.levels.WARN
  )
  return require("distract.manifests.cat")
end

--- Where one spawn lands, how deep it is, and what it stands on.
---
--- The floor is whatever was last pushed in, exactly as it is on the overlay:
--- only the editor can see `cmdheight`, the statusline and where the text ends,
--- so only the editor measures and both engines are told. A spawn naming its own
--- `ground` is the one case that measures here, because it is asking about a
--- surface the pushed floor does not describe.
---@param request table `{ asset_name, manifest, initial_def, opts, config, floor_row }`
---@return table placement
function M.placement(request)
  local opts = request.opts
  local settings = position.settings(request.config.position, opts)
  local floor_row = opts.ground and position.floor_row(settings.ground) or request.floor_row

  local _, sprite_h = M.sprite_cell_size(request.asset_name)
  return position.placement({
    settings = settings,
    backend = request.config.backend,
    locomotion = locomotion.locomotion_for(request.manifest, request.initial_def),
    declared_anchor = position.manifest_anchor(request.manifest),
    floor_row = floor_row,
    sprite_h = sprite_h,
    bounds = viewport.bounds(),
    opts = opts,
  })
end

--- Seeds velocity, lock and floor from the initial state's physics.
---
--- Velocity is signed by the heading, so a pet spawned facing left walks left
--- without the manifest needing a mirrored copy of every state.
local function apply_initial_physics(entity, state_def)
  if not (state_def and state_def.physics) then
    return
  end
  local physics = state_def.physics

  entity.target_vx = (physics.target_vx or 0) * entity.heading_x
  entity.target_vy = physics.target_vy or 0
  entity.vx = entity.target_vx
  entity.vy = entity.target_vy
  entity.is_locked = state_def.is_locked or false
  if physics.ground_y then
    entity.ground_y = physics.ground_y
  end
end

--- Offsets the animation and the path from anything already alive.
---
--- Two entities spawned together otherwise share a frame index, a frame timer
--- and a path phase for the rest of their lives, which reads as a chorus line
--- rather than as two animals.
local function desynchronise(entity, state_def)
  local animation = state_def and state_def.animation
  local frame_count = (animation and animation.frames and #animation.frames) or 1

  entity.frame_idx = math.random(1, math.max(1, frame_count))
  entity.frame_timer = math.random() * FRAME_TIMER_JITTER_SECONDS
  entity.path_phase = math.random() * 2 * math.pi
end

--- Builds one entity, ready to be inserted into the world.
---@param request table `{ id, asset_name, manifest, placement, flip_x }`
---@return table entity
function M.build(request)
  local manifest = request.manifest
  local placement = request.placement
  local initial_state = manifest.initial_state or "idle"
  local state_def = manifest.states and manifest.states[initial_state]

  local flip_x = request.flip_x or false
  local entity = {
    id = request.id,
    asset_name = request.asset_name,
    manifest = manifest,
    x = placement.x,
    y = placement.y,
    vx = 0,
    vy = 0,
    target_vx = 0,
    target_vy = 0,
    heading_x = flip_x and -1 or 1,
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
    base_x = placement.x,
    base_y = placement.y,
    ground_y = placement.ground_y or placement.y,
    path_phase = 0,
    action_timer = nil,
    action_duration = nil,
    return_state = nil,
    is_locked = false,
    -- `z` is draw order as well as depth, and a spawned one wins over the
    -- manifest's `z_index`.
    z_index = placement.z and math.floor(placement.z + 0.5) or manifest.z_index or DEFAULT_Z_INDEX,
    z = placement.z or 0,
    parallax = placement.parallax,
  }

  apply_initial_physics(entity, state_def)
  desynchronise(entity, state_def)

  return entity
end

return M
