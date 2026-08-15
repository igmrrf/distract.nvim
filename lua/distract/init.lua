local M = {}

local default_cat = require("distract.manifests.cat")
local default_crab = require("distract.manifests.crab")
local default_sun = require("distract.manifests.sun")

local available_backends = { "halfblock", "overlay" }

-- Aliases that resolve to a backend which is genuinely implemented.
local BACKEND_ALIASES = {
  halfblock = "halfblock", tui = "halfblock", terminal = "halfblock", truecolor = "halfblock",
  overlay = "overlay", external = "overlay", gpu = "overlay", wgpu = "overlay",
}

-- Names that no longer name a backend of their own.
--   float/ascii  -- the ASCII art backend was removed in favour of truecolor
--                   pixel sprites; there is no text-art rendering path left.
--   kitty/ghostty/wezterm
--                -- the Kitty graphics protocol backend is not implemented.
-- Both used to resolve to something that silently drew the wrong thing. They
-- now resolve to halfblock, and the substitution is reported rather than hidden.
local SUBSTITUTED_ALIASES = {
  float = { to = "halfblock", why = "the ASCII backend was removed; sprites are truecolor pixel art now" },
  ascii = { to = "halfblock", why = "the ASCII backend was removed; sprites are truecolor pixel art now" },
  lua = { to = "halfblock", why = "the ASCII backend was removed; sprites are truecolor pixel art now" },
  window = { to = "halfblock", why = "the ASCII backend was removed; sprites are truecolor pixel art now" },
  kitty = { to = "halfblock", why = "the Kitty graphics protocol backend is not implemented yet" },
  ghostty = { to = "halfblock", why = "the Kitty graphics protocol backend is not implemented yet" },
  wezterm = { to = "halfblock", why = "the Kitty graphics protocol backend is not implemented yet" },
}

local substitution_warned = {}

--- Normalize backend alias to canonical name.
--- @param b string|nil requested backend name or alias
--- @param quiet boolean|nil suppress the substitution notice
local function normalize_backend(b, quiet)
  if not b then return "halfblock" end
  b = string.lower(vim.trim(b))

  local substitute = SUBSTITUTED_ALIASES[b]
  if substitute then
    if not quiet and not substitution_warned[b] then
      substitution_warned[b] = true
      vim.notify(string.format("[Distract] Backend '%s' is unavailable: %s. Using '%s' instead.",
        b, substitute.why, substitute.to), vim.log.levels.WARN)
    end
    return substitute.to
  end

  return BACKEND_ALIASES[b] or "halfblock"
end

M.config = {
  backend = "halfblock", -- 'halfblock' (In-terminal Truecolor), 'float' (ASCII Window), 'overlay' (GPU Overlay)
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
  return vim.deepcopy(available_backends)
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
