local M = {}

local default_cat = require("distract.manifests.cat")
local default_crab = require("distract.manifests.crab")
local default_sun = require("distract.manifests.sun")

M.config = {
  backend = "external", -- 'external' (Graphical Engine) or 'lua' (ASCII Floating Windows)
  fps = 60,
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

  if M.config.backend == "external" then
    require("distract.external").setup(M.config)
  end
  is_setup = true

  vim.api.nvim_create_autocmd("VimLeavePre", {
    callback = function()
      M.stop()
    end,
  })
end

function M.start()
  if not is_setup then
    M.setup()
  end

  if M.config.backend == "external" then
    require("distract.external").start()
  else
    require("distract.engine").start()
  end
  require("distract.events").setup(M.config)
end

function M.stop()
  if M.config.backend == "external" then
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
  if M.config.backend == "external" then
    require("distract.external").spawn(asset_name, opts)
  else
    require("distract.engine").spawn(asset_name)
  end
end

function M.action(action_name, target)
  if not is_setup then
    M.setup()
  end

  if M.config.backend == "external" then
    require("distract.external").trigger_action(action_name, target)
  else
    vim.notify("Distract: Custom actions require the external graphical backend.", vim.log.levels.WARN)
  end
end

function M.clear()
  if M.config.backend == "external" then
    require("distract.external").clear()
  else
    require("distract.engine").stop()
  end
end

function M.status()
  if M.config.backend == "external" then
    require("distract.external").get_status()
  else
    vim.notify("Distract: ASCII fallback running.", vim.log.levels.INFO)
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
