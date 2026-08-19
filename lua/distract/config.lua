local M = {}

local position = require("distract.position")
local render = require("distract.render")
local viewport = require("distract.viewport")

M.BUILTIN_ASSETS = { "cat", "crab", "sun", "cat_walking", "gudong", "iris", "minty" }

function M.load_builtin_manifest(name)
  local ok, manifest = pcall(require, "distract.manifests." .. name)
  if ok then
    return manifest
  end
  return nil
end

function M.lazy_assets()
  return setmetatable({}, {
    __index = function(asset_table, name)
      if not vim.tbl_contains(M.BUILTIN_ASSETS, name) then
        return nil
      end
      local manifest = M.load_builtin_manifest(name)
      rawset(asset_table, name, manifest)
      return manifest
    end,
  })
end

function M.defaults()
  return {
    backend = nil,
    fps = 30,
    idle_timeout_ms = 5000,
    debounce_ms = 50,
    cell_width = nil,
    cell_height = nil,
    max_sprite_colours = 128,
    max_highlight_groups = 4096,
    restrict_to_instance = true,
    overlay = { monitor = nil, position = nil },
    position = vim.deepcopy(position.DEFAULTS),
    positioning = vim.deepcopy(viewport.DEFAULTS),
    render = vim.deepcopy(render.DEFAULTS),
    assets = M.lazy_assets(),
  }
end

return M
