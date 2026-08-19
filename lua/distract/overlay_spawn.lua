local M = {}

local asset_path = require("distract.asset_path")
local locomotion = require("distract.locomotion")
local position = require("distract.position")

function M.overlay_args(overlay)
  if type(overlay) ~= "table" then
    return {}
  end

  local point = overlay.position
  if point ~= nil then
    if type(point) ~= "table" or type(point.x) ~= "number" or type(point.y) ~= "number" then
      return nil, "overlay.position must be { x = <number>, y = <number> }"
    end
    return {
      "--overlay-position",
      string.format("%d,%d", math.floor(point.x), math.floor(point.y)),
    }
  end

  local monitor = overlay.monitor
  if monitor ~= nil then
    if type(monitor) ~= "number" or monitor < 0 or monitor ~= math.floor(monitor) then
      return nil, "overlay.monitor must be a non-negative whole number (0 is the primary display)"
    end
    return { "--overlay-monitor", tostring(monitor) }
  end

  return {}
end

function M.resolve_placement(config_position, asset, opts)
  local settings = position.settings(config_position, opts)
  local manifest = asset or {}
  local initial_def = manifest.states and manifest.states[manifest.initial_state]
  local anchor = position.effective_anchor(
    settings.anchor,
    position.manifest_anchor(asset),
    locomotion.locomotion_for(manifest, initial_def)
  )

  local position_x, position_y, position_z = opts.x, opts.y, opts.z
  if type(anchor) == "table" then
    position_x, position_y, position_z =
      position_x or anchor.x, position_y or anchor.y, position_z or anchor.z
    anchor = nil
  end

  return {
    x = position_x,
    y = position_y,
    z = position_z,
    parallax = position.parallax_for(position_z, settings, "overlay"),
    anchor = (position_x == nil or position_y == nil) and anchor or nil,
  }
end

function M.build_spawn_command(entity_name, asset, opts, placement, cell_w, cell_h)
  local manifest_payload = nil
  local abs_path = nil

  if asset then
    manifest_payload = vim.deepcopy(asset)
    if manifest_payload.spritesheet then
      if next(manifest_payload.spritesheet) == nil or not manifest_payload.spritesheet.path then
        manifest_payload.spritesheet = nil
      else
        manifest_payload.spritesheet.path = asset_path.resolve(manifest_payload.spritesheet.path)
        abs_path = manifest_payload.spritesheet.path
      end
    end
  end

  return {
    command = "Spawn",
    entity_type = entity_name,
    path = abs_path,
    manifest = manifest_payload,
    x = placement.x and (placement.x * cell_w) or nil,
    y = placement.y and (placement.y * cell_h) or nil,
    z = placement.z,
    parallax = placement.parallax,
    anchor = placement.anchor,
    flip_x = opts.flip_x or false,
  }
end

return M
