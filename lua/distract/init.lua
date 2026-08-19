local M = {}

local backends = require("distract.backends")
local position = require("distract.position")
local viewport = require("distract.viewport")

--- Built-in assets. Manifests are required on demand rather than at module
--- load: each one pulls in its sprite module for the frame layout, and eagerly
--- loading all three used to be paid on every Neovim start whether or not
--- anything was ever spawned.
local BUILTIN_ASSETS = { "cat", "crab", "sun", "cat_walking", "gudong", "iris", "minty" }

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
  -- 'halfblock' (in-terminal truecolor), 'kitty' (graphics protocol) or
  -- 'overlay' (GPU window). Left unset, the best backend this terminal can
  -- actually draw is chosen; see `default_backend`.
  backend = nil,
  fps = 30,
  idle_timeout_ms = 5000,
  debounce_ms = 50,
  -- Overlay only: terminal cell size in physical pixels. Leave unset to use the
  -- terminal's own report where available, otherwise a 10x20 default.
  -- See `:help distract-overlay`.
  cell_width = nil,
  cell_height = nil,
  -- Half-block only: imported art (a manifest pointing at a GIF) is reduced to
  -- this many colours before it is drawn, because every distinct colour pair
  -- becomes a Neovim highlight group.
  max_sprite_colours = 128,
  -- Ceiling on how many of those highlight groups stay defined at once. The
  -- least recently drawn asset's groups are cleared when it is reached.
  max_highlight_groups = 4096,
  -- Hide sprites while this Neovim instance does not have focus, and show them
  -- again when it does. The simulation keeps running either way. Set false to
  -- keep drawing regardless, which is what a standalone desktop animation wants.
  restrict_to_instance = true,
  -- Overlay only: which display the overlay window opens on.
  --
  -- Left unset, the engine detects the display the terminal has focus on where
  -- the platform allows it (macOS today) and warns if it cannot, because the
  -- overlay is a separate OS window and neither Neovim nor the Lua side can see
  -- which screen it should be on.
  --
  -- `monitor` is a 0-based index into the window system's display list, 0 being
  -- the primary display. `position` is an explicit `{ x, y }` point in global
  -- desktop coordinates and wins over `monitor`.
  overlay = { monitor = nil, position = nil },
  -- Where entities are placed and what they stand on. See
  -- `distract.position` for the anchor and ground vocabulary.
  position = vim.deepcopy(position.DEFAULTS),
  -- Which rectangle entities may move in, what they must not cover, and where
  -- their surfaces sit in Neovim's float stacking. See `distract.viewport`.
  positioning = vim.deepcopy(viewport.DEFAULTS),
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

--- Lets a backend that has to prove itself register before a name is resolved.
---
--- The kitty renderer only exists if the terminal answers its query, so it is
--- asked exactly when someone requests it. A session on any other backend never
--- sends the probe and never waits for the answer.
local function admit_conditional_backends(requested)
  local kitty = require("distract.kitty")
  kitty.ensure_offered()
  kitty.ensure_registered(requested)
end

--- The backend a session gets when nobody names one.
---
--- A terminal that speaks the graphics protocol draws sprites at full pixel
--- fidelity, so a user on kitty, Ghostty or WezTerm gets that rather than
--- half-blocks. Everyone else gets the renderer that works everywhere. Naming a
--- backend still wins over both.
local function default_backend()
  local kitty = require("distract.kitty")
  if kitty.is_registered() then
    return kitty.NAME
  end
  return backends.HALFBLOCK
end

function M.setup(opts)
  opts = opts or {}
  M.config = vim.tbl_deep_extend("force", M.config, opts)
  admit_conditional_backends(M.config.backend)
  M.config.backend = backends.resolve(M.config.backend or default_backend())
  -- `tbl_deep_extend` copies into a plain table, so re-attach the lazy loader
  -- while keeping anything the user supplied.
  M.config.assets = setmetatable(M.config.assets or {}, getmetatable(lazy_assets()))

  viewport.configure(M.config.positioning)
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
      require("distract.terminal_sprites").reset_highlights()
      local renderer = require("distract.renderer")
      renderer.reset_backends()
      renderer.clear_all()
      renderer.refresh_highlights()
      renderer.invalidate_screen_map()
    end,
  })
end

function M.get_backend()
  return M.config.backend or default_backend()
end

function M.get_available_backends()
  admit_conditional_backends(nil)
  return backends.names()
end

--- What the running backend can do with a sprite.
---@return DistractBackendCapabilities|nil
function M.get_backend_capabilities()
  return backends.capabilities(M.config.backend)
end

--- Switches backend, preserving whether the plugin was running.
---
--- Entities do not migrate: the two backends keep separate worlds. That is
--- reported rather than left for the user to discover.
function M.set_backend(backend_name)
  admit_conditional_backends(backend_name)
  local norm = backends.resolve(backend_name)
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
        "[Distract] Switched backend to '%s' and restarted. "
          .. "Entities do not carry over; spawn again with :DistractSpawn.",
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
  -- Measured here rather than inside either engine: an engine is told where the
  -- floor is, it does not go looking. A spawn is the one moment the answer has
  -- to be current even when the plugin was never started, so no autocommand has
  -- pushed one yet.
  require("distract.events").sync_floor(M.config.position)
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

  -- A backend holds the snapshot of `config` it was set up with, so a manifest
  -- registered afterwards only reaches it through another `setup`.
  if is_setup then
    backend_module(M.config.backend).setup(M.config)
  end
end

--- Registers a plugin against the engine's lifecycle hooks.
---
--- Hooks observe the simulation and request changes through the `world` handle
--- their `on_init` receives; the entity a hook is handed is read-only. That is
--- what keeps one plugin behaving the same way on the in-terminal backends,
--- which simulate in Lua, and on the overlay, which simulates in its own
--- process. See `:help distract-plugins`.
---
--- @param name string unique plugin name
--- @param spec table hooks: `on_init`, `on_tick`, `on_state_change`,
---   `on_collision`, `on_editor_event`, `on_draw`, `on_teardown`
function M.register_plugin(name, spec)
  require("distract.plugins").register(name, spec)
  -- What the overlay subscribes to is derived from the registrations, so a
  -- plugin registered after the engine started still gets its hooks called.
  require("distract.external").sync_plugin_subscription()
end

--- Removes a plugin, running its `on_teardown` first.
--- @param name string
--- @return boolean removed
function M.unregister_plugin(name)
  local removed = require("distract.plugins").unregister(name)
  require("distract.external").sync_plugin_subscription()
  return removed
end

--- Registers a provider of solid platforms and hazards.
---
--- The provider is called with the current window and buffer on a debounced
--- cadence — editing, scrolling and window changes — and never per tick per
--- entity, because a Tree-sitter query per frame is a performance trap. Its
--- rectangles are in terminal cells:
---
--- ```lua
--- require("distract").register_obstacle_provider(function(win_id, buf_id)
---   return {
---     { x = 10, y = 15, width = 40, height = 1, type = "solid_platform" },
---     { x = 0, y = 25, width = 80, height = 1, type = "hazard" },
---   }
--- end)
--- ```
---
--- @param provider function `function(win_id, buf_id) -> table[]`
--- @return integer id for `unregister_obstacle_provider`
function M.register_obstacle_provider(provider)
  local id = require("distract.obstacles").register_provider(provider)
  require("distract.events").sync_obstacles()
  return id
end

--- Removes an obstacle provider.
--- @param id integer as returned by `register_obstacle_provider`
--- @return boolean removed
function M.unregister_obstacle_provider(id)
  local removed = require("distract.obstacles").unregister_provider(id)
  require("distract.events").sync_obstacles()
  return removed
end

--- Names of the registered plugins, in dispatch order.
--- @return string[]
function M.get_plugin_names()
  return require("distract.plugins").names()
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
