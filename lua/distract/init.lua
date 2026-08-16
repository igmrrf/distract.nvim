local M = {}

--- Backends that exist and can be selected.
local available_backends = { "halfblock", "overlay" }

--- Built-in assets. Manifests are required on demand rather than at module
--- load: each one pulls in its sprite module for the frame layout, and eagerly
--- loading all three used to be paid on every Neovim start whether or not
--- anything was ever spawned.
local BUILTIN_ASSETS = { "cat", "crab", "sun" }

--- Aliases that resolve to a backend which is genuinely implemented.
local BACKEND_ALIASES = {
  halfblock = "halfblock",
  tui = "halfblock",
  terminal = "halfblock",
  truecolor = "halfblock",
  overlay = "overlay",
  external = "overlay",
  gpu = "overlay",
  wgpu = "overlay",
}

--- Names that no longer name a backend of their own.
---   float/ascii  -- the ASCII art backend was removed in favour of truecolor
---                   pixel sprites; there is no text-art rendering path left.
---   kitty/ghostty/wezterm
---                -- the Kitty graphics protocol backend is not implemented.
--- Both used to resolve to something that silently drew the wrong thing. They
--- now resolve to halfblock, and the substitution is reported rather than hidden.
local SUBSTITUTED_ALIASES = {
  float = {
    to = "halfblock",
    why = "the ASCII backend was removed; sprites are truecolor pixel art now",
  },
  ascii = {
    to = "halfblock",
    why = "the ASCII backend was removed; sprites are truecolor pixel art now",
  },
  lua = {
    to = "halfblock",
    why = "the ASCII backend was removed; sprites are truecolor pixel art now",
  },
  window = {
    to = "halfblock",
    why = "the ASCII backend was removed; sprites are truecolor pixel art now",
  },
  kitty = { to = "halfblock", why = "the Kitty graphics protocol backend is not implemented yet" },
  ghostty = { to = "halfblock", why = "the Kitty graphics protocol backend is not implemented yet" },
  wezterm = { to = "halfblock", why = "the Kitty graphics protocol backend is not implemented yet" },
}

local substitution_warned = {}

--- Normalize backend alias to canonical name.
--- @param b string|nil requested backend name or alias
--- @param quiet boolean|nil suppress the substitution notice
local function normalize_backend(b, quiet)
  if not b then
    return "halfblock"
  end
  b = string.lower(vim.trim(b))

  local substitute = SUBSTITUTED_ALIASES[b]
  if substitute then
    if not quiet and not substitution_warned[b] then
      substitution_warned[b] = true
      vim.notify(
        string.format(
          "[Distract] Backend '%s' is unavailable: %s. Using '%s' instead.",
          b,
          substitute.why,
          substitute.to
        ),
        vim.log.levels.WARN
      )
    end
    return substitute.to
  end

  return BACKEND_ALIASES[b] or "halfblock"
end

--- Loads a built-in manifest, or nil if there is no such asset.
local function load_builtin_manifest(name)
  local ok, manifest = pcall(require, "distract.manifests." .. name)
  if ok then
    return manifest
  end
  return nil
end

--- `config.assets` resolves built-in manifests on first access. A user-supplied
--- manifest set on the table directly always wins, because `__index` is only
--- consulted for absent keys.
local function lazy_assets()
  return setmetatable({}, {
    __index = function(t, name)
      if not vim.tbl_contains(BUILTIN_ASSETS, name) then
        return nil
      end
      local manifest = load_builtin_manifest(name)
      rawset(t, name, manifest)
      return manifest
    end,
  })
end

M.config = {
  -- 'halfblock' (in-terminal truecolor) or 'overlay' (GPU window).
  backend = "halfblock",
  fps = 30,
  idle_timeout_ms = 5000,
  debounce_ms = 50,
  -- Overlay only: terminal cell size in physical pixels. Leave unset to use the
  -- terminal's own report where available, otherwise a 10x20 default.
  -- See `:help distract-overlay`.
  cell_width = nil,
  cell_height = nil,
  assets = lazy_assets(),
}

local is_setup = false
local group = vim.api.nvim_create_augroup("Distract", { clear = true })

local function backend_module(backend)
  if backend == "overlay" then
    return require("distract.external")
  end
  return require("distract.engine")
end

function M.setup(opts)
  opts = opts or {}
  M.config = vim.tbl_deep_extend("force", M.config, opts)
  M.config.backend = normalize_backend(M.config.backend)
  -- `tbl_deep_extend` copies into a plain table, so re-attach the lazy loader
  -- while keeping anything the user supplied.
  M.config.assets = setmetatable(M.config.assets or {}, getmetatable(lazy_assets()))

  backend_module(M.config.backend).setup(M.config)
  is_setup = true

  -- Grouped and cleared: an ungrouped autocmd accumulated a duplicate on every
  -- `setup()` call, which config reloads and the test suite both do repeatedly.
  vim.api.nvim_clear_autocmds({ group = group })
  vim.api.nvim_create_autocmd("VimLeavePre", {
    group = group,
    callback = function()
      M.stop()
    end,
  })

  -- `:colorscheme` runs `:hi clear`, which deletes the per-colour highlight
  -- groups the sprites are painted with. Everything cached against them has to
  -- go with it, or every sprite draws in the default foreground until restart.
  vim.api.nvim_create_autocmd("ColorScheme", {
    group = group,
    callback = function()
      local sprites = require("distract.terminal_sprites")
      sprites.reset_highlights()
      sprites.reset_cache()
      local renderer = require("distract.renderer")
      renderer.clear_all()
      renderer.refresh_highlights()
      renderer.invalidate_screen_map()
    end,
  })
end

function M.get_backend()
  return M.config.backend
end

function M.get_available_backends()
  return vim.deepcopy(available_backends)
end

--- Switches backend, preserving whether the plugin was running.
---
--- Entities do not migrate: the two backends keep separate worlds. That is
--- reported rather than left for the user to discover.
function M.set_backend(backend_name)
  local norm = normalize_backend(backend_name)
  if norm == M.config.backend then
    vim.notify(string.format("[Distract] Backend is already '%s'", norm), vim.log.levels.INFO)
    return
  end

  local was_running = M.is_running()
  M.stop()
  M.config.backend = norm
  backend_module(norm).setup(M.config)

  if was_running then
    M.start()
    vim.notify(
      string.format(
        "[Distract] Switched backend to '%s' and restarted. Entities do not carry over; spawn again with :DistractSpawn.",
        norm
      ),
      vim.log.levels.INFO
    )
  else
    vim.notify(
      string.format("[Distract] Switched backend to '%s'. Start it with :DistractStart.", norm),
      vim.log.levels.INFO
    )
  end
end

function M.is_overlay()
  return M.config.backend == "overlay"
end

function M.is_running()
  return require("distract.external").is_running() or require("distract.engine").is_running()
end

function M.start()
  if not is_setup then
    M.setup()
  end

  if M.is_overlay() then
    require("distract.external").query_cell_size()
  end
  backend_module(M.config.backend).start()
  require("distract.events").setup(M.config)
end

function M.stop()
  backend_module(M.config.backend).stop()
  require("distract.events").teardown()
end

function M.spawn(asset_name, opts)
  if not is_setup then
    M.setup()
  end
  backend_module(M.config.backend).spawn(asset_name or "cat", opts)
end

function M.action(action_name, target)
  if not is_setup then
    M.setup()
  end
  backend_module(M.config.backend).trigger_action(action_name, target)
end

function M.clear()
  backend_module(M.config.backend).clear()
end

function M.status()
  backend_module(M.config.backend).get_status()
end

--- Builds the overlay engine binary asynchronously.
function M.build()
  require("distract.external").build()
end

--- Registers a custom asset: its manifest, its terminal art, or both.
---
--- Without this the terminal backend can only draw the three built-ins, so a
--- custom manifest used to spawn under its own name and render as a cat.
---
--- @param name string asset name, as passed to `:DistractSpawn`
--- @param spec table `{ manifest = <manifest>, sprites = <sprite module> }`
function M.register_asset(name, spec)
  if type(name) ~= "string" or name == "" then
    error("distract.register_asset: name must be a non-empty string")
  end
  spec = spec or {}

  if spec.sprites then
    require("distract.terminal_sprites").register(name, spec.sprites)
  end

  if spec.manifest then
    local manifest = vim.deepcopy(spec.manifest)
    manifest.name = manifest.name or name
    rawset(M.config.assets, name, manifest)
  end

  if not spec.sprites and not spec.manifest then
    error("distract.register_asset: nothing to register; pass `manifest`, `sprites`, or both")
  end
end

function M.get_asset_names()
  local seen = {}
  local names = {}
  local function push(name)
    if not seen[name] then
      seen[name] = true
      table.insert(names, name)
    end
  end
  for _, name in ipairs(BUILTIN_ASSETS) do
    push(name)
  end
  -- `pairs` only sees assets that have been materialised or user-supplied,
  -- which is why the built-in list is enumerated explicitly above.
  for name, _ in pairs(M.config.assets) do
    push(name)
  end
  table.sort(names)
  return names
end

function M.get_all_actions()
  local actions = {}
  local seen = {}
  for _, name in ipairs(M.get_asset_names()) do
    local asset = M.config.assets[name]
    if asset and asset.custom_actions then
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
