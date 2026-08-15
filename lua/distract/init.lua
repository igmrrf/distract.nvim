local M = {}

local default_cat = require("distract.manifests.cat")
local default_crab = require("distract.manifests.crab")
local default_sun = require("distract.manifests.sun")

local available_backends = { "halfblock", "kitty", "float", "overlay" }

--- Normalize backend alias to canonical name
local function normalize_backend(b)
  if not b then return "halfblock" end
  b = string.lower(vim.trim(b))
  if b == "halfblock" or b == "tui" or b == "terminal" or b == "truecolor" then
    return "halfblock"
  elseif b == "kitty" or b == "ghostty" or b == "wezterm" then
    return "kitty"
  elseif b == "float" or b == "ascii" or b == "lua" or b == "window" then
    return "float"
  elseif b == "overlay" or b == "external" or b == "gpu" or b == "wgpu" then
    return "overlay"
  else
    return "halfblock"
  end
end

M.config = {
  backend = "halfblock", -- 'halfblock' (In-terminal Truecolor), 'kitty' (Ghostty Graphics), 'float' (ASCII Window), 'overlay' (GPU Overlay)
  fps = 30,
  idle_timeout_ms = 5000,
  debounce_ms = 50,
  assets = {
    cat = default_cat,
    crab = default_crab,
    sun = default_sun,
  },
}

local is_setup = false

function M.setup(opts)
  opts = opts or {}
  M.config = vim.tbl_deep_extend("force", M.config, opts)
  M.config.backend = normalize_backend(M.config.backend)

  if M.config.backend == "overlay" or M.config.backend == "external" then
    require("distract.external").setup(M.config)
  else
    require("distract.engine").setup(M.config)
  end
  is_setup = true

  vim.api.nvim_create_autocmd("VimLeavePre", {
    callback = function()
      M.stop()
    end,
  })
end

function M.get_backend()
  return M.config.backend
end

function M.get_available_backends()
  return { "halfblock", "kitty", "float", "overlay" }
end

function M.set_backend(backend_name)
  local norm = normalize_backend(backend_name)
  if norm == M.config.backend then
    vim.notify(string.format("[Distract] Backend is already '%s'", norm), vim.log.levels.INFO)
    return
  end

  M.stop()
  M.config.backend = norm
  if norm == "overlay" then
    require("distract.external").setup(M.config)
  else
    require("distract.engine").setup(M.config)
  end
  vim.notify(string.format("[Distract] Switched backend to '%s'", norm), vim.log.levels.INFO)
end

function M.is_overlay()
  return M.config.backend == "overlay" or M.config.backend == "external"
end

function M.start()
  if not is_setup then
    M.setup()
  end

  if M.is_overlay() then
    require("distract.external").start()
  else
    require("distract.engine").start()
  end
  require("distract.events").setup(M.config)
end

function M.stop()
  if M.is_overlay() then
    require("distract.external").stop()
  else
    require("distract.engine").stop()
  end
  require("distract.events").teardown()
end

function M.spawn(asset_name, opts)
  if not is_setup then
    M.setup()
  end

  asset_name = asset_name or "cat"
  if M.is_overlay() then
    require("distract.external").spawn(asset_name, opts)
  else
    require("distract.engine").spawn(asset_name, opts)
  end
end

function M.action(action_name, target)
  if not is_setup then
    M.setup()
  end

  if M.is_overlay() then
    require("distract.external").trigger_action(action_name, target)
  else
    require("distract.engine").trigger_action(action_name, target)
  end
end

function M.clear()
  if M.is_overlay() then
    require("distract.external").clear()
  else
    require("distract.engine").clear()
  end
end

function M.status()
  if M.is_overlay() then
    require("distract.external").get_status()
  else
    require("distract.engine").get_status()
  end
end

function M.get_asset_names()
  local names = {}
  for name, _ in pairs(M.config.assets) do
    table.insert(names, name)
  end
  table.sort(names)
  return names
end

function M.get_all_actions()
  local actions = {}
  local seen = {}
  for _, asset in pairs(M.config.assets) do
    if asset.custom_actions then
      for action_name, _ in pairs(asset.custom_actions) do
        if not seen[action_name] then
          seen[action_name] = true
          table.insert(actions, action_name)
        end
      end
    end
  end
  table.sort(actions)
  return actions
end

return M
