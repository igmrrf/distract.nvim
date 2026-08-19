local M = {}

local placement = require("distract.placement")
local sprites = require("distract.terminal_sprites")
local viewport = require("distract.viewport")

local HALFBLOCK_CAPABILITY = { native_resolution = false }

function M.wraps_at_the_edge(entity)
  local states = entity.manifest and entity.manifest.states
  local state_def = states and states[entity.current_state]
  local physics = state_def and state_def.physics
  return physics == nil or (physics.wrap_mode or "wrap") == "wrap"
end

function M.is_occluding(entity, surface, bounds, blocked)
  if #blocked == 0 then
    return false
  end
  local geom = placement.resolve({
    x = entity.x,
    y = entity.y,
    width = surface.width,
    height = surface.height,
    bounds = bounds,
    wrap = M.wraps_at_the_edge(entity),
  })
  for _, slice in ipairs(geom.slices) do
    for _, rect in ipairs(blocked) do
      if viewport.overlaps(slice, rect) then
        return true
      end
    end
  end
  return false
end

function M.resolve_pixel_frame(entity, frame_count)
  if not frame_count or frame_count < 1 then
    return 1
  end

  local manifest = entity.manifest
  local state_def = manifest and manifest.states and manifest.states[entity.current_state]
  local frames = state_def and state_def.animation and state_def.animation.frames

  if not frames or #frames == 0 then
    return 1
  end

  local position = ((math.max(1, entity.frame_idx or 1) - 1) % #frames) + 1
  local sheet_idx = frames[position] or 0

  return (sheet_idx % frame_count) + 1
end

function M.resolve_flip(entity)
  local manifest = entity.manifest
  local state_def = manifest and manifest.states and manifest.states[entity.current_state]
  local anim_flip = state_def and state_def.animation and state_def.animation.flip_x or false
  local entity_flip = entity.flip_x or false
  return entity_flip ~= anim_flip
end

function M.halfblock_surface(entity)
  local frame_count = #sprites.get_pixel_frames(entity.asset_name, HALFBLOCK_CAPABILITY)
  local frame_idx = M.resolve_pixel_frame(entity, frame_count)
  local flip_x = M.resolve_flip(entity)

  local frame_buf, sprite_w, sprite_h =
    sprites.get_frame_buffer(entity.asset_name, frame_idx, flip_x)

  if not frame_buf or sprite_w < 1 or sprite_h < 1 then
    return nil
  end

  return {
    key = frame_buf,
    buf = frame_buf,
    width = sprite_w,
    height = sprite_h,
    runs = function()
      return sprites.get_frame_runs(entity.asset_name, frame_idx, flip_x)
    end,
  }
end

return M
