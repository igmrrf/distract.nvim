--- What the renderer draws, and how.
---
--- Mirrors `engine/src/render.rs`: one settings block, validated once here and
--- pushed to the overlay, so the terminal backends and the overlay read the same
--- numbers rather than each deciding for itself. That is the rule every measured
--- quantity in this plugin already follows — the floor, the viewport scope and the
--- obstacle list are all measured in Neovim and pushed to both engines.

local M = {}

local voxel = require("distract.voxel")

--- Draw one textured quad per entity, ordered by `z_index`. What every asset has
--- always been drawn as, and still the default.
M.FLAT = "2d"
--- Draw a voxel model extruded from the entity's current frame, depth-tested.
M.VOXEL = "3d"

local MODES = { [M.FLAT] = true, [M.VOXEL] = true }

---@class DistractLightConfig
---@field direction? number[] Direction the single directional light travels in [x, y, z] world axes (y is down)
---@field ambient? number Ambient floor brightness between 0.0 and 1.0 (default: 0.42)

---@class DistractRenderConfig
---@field mode? "2d"|"3d" Render mode: "2d" for flat sprites, "3d" for voxel models (default: "2d")
---@field fov_y_degrees? number Overlay only: vertical field of view in degrees (10 to 120, default: 45.0)
---@field depth_per_unit? number Depth of one unit of z as fraction of eye distance (0 to 0.5, default: 0.05)
---@field yaw_degrees? number Off-axis model rotation in degrees (default: 22.0)
---@field voxel_max_width? integer Widest voxel grid extruded before resampling (1 to 128, default: 48)
---@field voxel_depth? integer Slab thickness in voxels (1 to 64, default: 4)
---@field light? DistractLightConfig Directional and ambient lighting settings

---@type DistractRenderConfig
M.DEFAULTS = {
  mode = M.FLAT,
  fov_y_degrees = 45.0,
  --- Depth of one unit of `z`, as a fraction of the eye distance.
  depth_per_unit = 0.05,
  --- How far a model is turned off head-on, in degrees.
  ---
  --- Zero renders a model face-on, where it is indistinguishable from its own
  --- sprite: the depth is there and nothing reveals it.
  yaw_degrees = 22.0,
  voxel_max_width = voxel.DEFAULT_MAX_WIDTH,
  voxel_depth = voxel.DEFAULT_DEPTH,
  light = {
    --- Direction the light travels, in world axes. `y` is down, so a positive `y`
    --- is a light from above.
    direction = { -0.4, 0.8, -0.45 },
    --- Floor brightness for a face the light does not reach. A face in full
    --- shadow at 0 is pure black, which reads as a hole in the model.
    ambient = 0.42,
  },
}

--- Bounds every field is held to, because they come from a user's configuration.
M.MIN_FOV_Y_DEGREES = 10.0
M.MAX_FOV_Y_DEGREES = 120.0
M.MAX_VOXEL_MAX_WIDTH = 128
M.MAX_VOXEL_DEPTH = 64
M.MAX_DEPTH_PER_UNIT = 0.5

local function clamp(value, minimum, maximum)
  return math.max(minimum, math.min(maximum, value))
end

local function require_number(field, value)
  if type(value) ~= "number" then
    error(string.format("distract: render.%s must be a number", field))
  end
  return value
end

--- Validates and clamps a user's `render` block.
---
--- Refuses an unknown mode outright rather than falling back to a default: a
--- typo in `mode = "3D "` that silently rendered 2D is indistinguishable from a
--- renderer that does not work.
---@param config table|nil the user's `render` table
---@return table settings
function M.settings(config)
  local merged = vim.tbl_deep_extend("force", vim.deepcopy(M.DEFAULTS), config or {})

  if not MODES[merged.mode] then
    error(
      string.format(
        "distract: render.mode must be '%s' or '%s', got '%s'",
        M.FLAT,
        M.VOXEL,
        tostring(merged.mode)
      )
    )
  end

  merged.fov_y_degrees = clamp(
    require_number("fov_y_degrees", merged.fov_y_degrees),
    M.MIN_FOV_Y_DEGREES,
    M.MAX_FOV_Y_DEGREES
  )
  merged.depth_per_unit =
    clamp(require_number("depth_per_unit", merged.depth_per_unit), 0, M.MAX_DEPTH_PER_UNIT)
  merged.yaw_degrees = require_number("yaw_degrees", merged.yaw_degrees) % 360
  merged.voxel_max_width = math.floor(
    clamp(require_number("voxel_max_width", merged.voxel_max_width), 1, M.MAX_VOXEL_MAX_WIDTH)
  )
  merged.voxel_depth =
    math.floor(clamp(require_number("voxel_depth", merged.voxel_depth), 1, M.MAX_VOXEL_DEPTH))
  merged.light.ambient = clamp(require_number("light.ambient", merged.light.ambient), 0, 1)

  if #merged.light.direction ~= 3 then
    error("distract: render.light.direction must be three numbers")
  end
  for axis = 1, 3 do
    require_number("light.direction", merged.light.direction[axis])
  end

  return merged
end

--- The light direction as a unit vector, or straight down when the configured
--- direction has no length to normalise.
---@param settings table
---@return number[]
function M.light_direction(settings)
  local direction = settings.light and settings.light.direction or M.DEFAULTS.light.direction
  local length = math.sqrt(
    direction[1] * direction[1] + direction[2] * direction[2] + direction[3] * direction[3]
  )
  if length < 1e-9 then
    return { 0, 1, 0 }
  end
  return { direction[1] / length, direction[2] / length, direction[3] / length }
end

--- How one asset is drawn.
---
--- A manifest may pin itself to a mode — a speech bubble reads as flat overlay
--- furniture in a 3D session too — and everything else follows the configuration.
---@param settings table
---@param manifest table|nil
---@return string mode
function M.mode_for(settings, manifest)
  local declared = manifest and manifest.render
  if declared ~= nil then
    if not MODES[declared] then
      error(
        string.format(
          "distract: manifest '%s' declares render = '%s'; expected '%s' or '%s'",
          tostring(manifest.name),
          tostring(declared),
          M.FLAT,
          M.VOXEL
        )
      )
    end
    return declared
  end
  return settings.mode
end

---@param settings table
---@param manifest table|nil
---@return boolean
function M.is_voxel(settings, manifest)
  return M.mode_for(settings, manifest) == M.VOXEL
end

return M
