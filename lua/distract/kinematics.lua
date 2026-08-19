--- Path primitives, unit conversion and animation frame timing.
---
--- Extracted from `engine.lua` verbatim so that module has room to grow under
--- the size cap. The physics parity fixtures pin every function here against
--- the Rust port, which is what makes the move safe to read as structural.
---
--- Units. Manifest positions and velocities are in *sprite pixels*, velocities
--- per frame at 60 FPS. One sprite pixel is one terminal cell wide and half a
--- cell tall, which is what the half-block renderer draws.

local M = {}

local sprites = require("distract.terminal_sprites")

--- Sprite pixels per terminal cell.
M.CELLS_PER_SPRITE_PX_X = 1.0
M.CELLS_PER_SPRITE_PX_Y = 0.5
--- Reference frame rate the manifest velocities are expressed against.
M.REFERENCE_FPS = 60

local CELLS_PER_SPRITE_PX_X = M.CELLS_PER_SPRITE_PX_X
local CELLS_PER_SPRITE_PX_Y = M.CELLS_PER_SPRITE_PX_Y

--- Path parameters with the legacy aliases and the defaults filled in.
---
--- Mirrors `PhysicsConfig::resolved_path` in `manifest.rs`. `path_amplitude`
--- and `path_frequency` predate `path_params` and are exactly `amp_y` and
--- `freq_y` under older names -- the sun's manifest still uses them.
function M.resolved_path(phys)
  local p = phys.path_params or {}
  local amp_y = p.amp_y or phys.path_amplitude or 4.0
  local freq_y = p.freq_y or phys.path_frequency or 2.0
  return {
    freq = p.freq or 1.0,
    -- Defaulting the x axis to the y axis makes an `orbital` path with no
    -- parameters a circle rather than a flat line.
    freq_x = p.freq_x or freq_y,
    freq_y = freq_y,
    amp_x = p.amp_x or amp_y,
    amp_y = amp_y,
    phase_delta = p.phase_delta or 0.0,
  }
end

--- A cubic Bezier evaluated at `t`, in sprite pixels relative to the anchor.
local function cubic_bezier(points, t)
  local u = 1 - t
  local a, b, c, d = u * u * u, 3 * u * u * t, 3 * u * t * t, t * t * t
  return a * points[1][1] + b * points[2][1] + c * points[3][1] + d * points[4][1],
    a * points[1][2] + b * points[2][2] + c * points[3][2] + d * points[4][2]
end

--- Applies a path primitive's positional override in place.
---
--- Mirrors `apply_path` in `ecs.rs`. The phase advances at a base rate and
--- per-axis frequency multiplies *inside* the trigonometric term; folding
--- frequency into the advance instead would double-apply it on `lissajous`,
--- where the two axes must run at different rates against one shared phase.
--- With `freq` defaulting to 1 and the `path_frequency` alias, `sine` evaluates
--- exactly what it always did.
function M.apply_path(entity, phys, dt)
  local path_type = phys.path_type
  -- `linear` is pure velocity integration, which already happened.
  if not path_type or path_type == "linear" then
    return
  end

  local p = M.resolved_path(phys)
  entity.path_phase = entity.path_phase + (dt * p.freq)
  local phase = entity.path_phase

  if path_type == "sine" then
    entity.y = entity.base_y + math.sin(p.freq_y * phase) * p.amp_y * CELLS_PER_SPRITE_PX_Y
  elseif path_type == "orbital" then
    entity.x = entity.base_x + math.cos(p.freq_x * phase) * p.amp_x * CELLS_PER_SPRITE_PX_X
    entity.y = entity.base_y + math.sin(p.freq_y * phase) * p.amp_y * CELLS_PER_SPRITE_PX_Y
  elseif path_type == "lissajous" then
    entity.x = entity.base_x
      + math.sin(p.freq_x * phase + p.phase_delta) * p.amp_x * CELLS_PER_SPRITE_PX_X
    entity.y = entity.base_y + math.sin(p.freq_y * phase) * p.amp_y * CELLS_PER_SPRITE_PX_Y
  elseif path_type == "bezier" then
    local points = phys.path_params and phys.path_params.points
    if not points or #points < 4 then
      return
    end
    -- Wrapped rather than clamped, so the curve loops instead of running off
    -- its last control point and staying there.
    local ox, oy = cubic_bezier(points, phase % 1.0)
    entity.x = entity.base_x + ox * CELLS_PER_SPRITE_PX_X
    entity.y = entity.base_y + oy * CELLS_PER_SPRITE_PX_Y
  end
  -- An unrecognised path is velocity integration, same as `linear`.
end

--- What one animation frame is shown for when nothing declares a rate.
local FALLBACK_FRAME_SECONDS = 0.1

local MS_PER_SECOND = 1000

--- How long the entity's current animation frame is shown for, in seconds.
---
--- A manifest `fps` wins. Imported art whose state declares none is timed by
--- the delays stored in the file, which is the only rate an animation authored
--- elsewhere carries; `engine/src/ecs.rs` applies the same precedence, so a GIF
--- asset runs at one speed on both backends.
function M.frame_duration_seconds(entity, anim)
  if anim.fps and anim.fps > 0 then
    return 1 / anim.fps
  end

  local sheet_index = anim.frames and anim.frames[entity.frame_idx]
  if sheet_index then
    local delay_ms = sprites.frame_delay_ms(entity.asset_name, sheet_index + 1)
    if delay_ms and delay_ms > 0 then
      return delay_ms / MS_PER_SECOND
    end
  end

  return FALLBACK_FRAME_SECONDS
end

return M
