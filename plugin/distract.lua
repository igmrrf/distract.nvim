if vim.g.loaded_distract then
  return
end
vim.g.loaded_distract = true

-- Required lazily. Loading the plugin module pulls in manifests and their
-- sprite layouts, and paying that on every Neovim start -- for users who may
-- never spawn anything -- is not the plugin's to spend.
local function distract()
  return require("distract")
end

vim.api.nvim_create_user_command("DistractStart", function()
  distract().start()
end, { desc = "Start Distract render engine" })

vim.api.nvim_create_user_command("DistractStop", function()
  distract().stop()
end, { desc = "Stop Distract render engine" })

vim.api.nvim_create_user_command("DistractBackend", function(opts)
  local backend = vim.trim(opts.args or "")
  if backend == "" then
    local caps = distract().get_backend_capabilities() or {}
    vim.notify(
      string.format(
        "[Distract] Current backend: '%s' (available: %s)\n"
          .. "  sprite scaling: %s, transparency: per %s, z: draw order%s",
        distract().get_backend(),
        table.concat(distract().get_available_backends(), ", "),
        caps.scale and "yes" or "no",
        tostring(caps.alpha),
        caps.scale and " and parallax" or " only"
      ),
      vim.log.levels.INFO
    )
  else
    distract().set_backend(backend)
  end
end, {
  nargs = "?",
  desc = "View or switch Distract rendering backend (halfblock, kitty, overlay)",
  complete = function(_, line)
    local parts = vim.split(line, "%s+")
    if #parts <= 2 then
      return distract().get_available_backends()
    end
    return {}
  end,
})

--- Anchors `:DistractSpawn` accepts. The `{ x, y, z }` form of `anchor` is a
--- Lua-only spelling of `x=`, `y=` and `z=`, which the command already has.
local SPAWN_ANCHORS = { auto = true, bottom = true, top = true, free = true }

--- Option names `:DistractSpawn` accepts, and how to read their values.
local SPAWN_OPTIONS = {
  x = tonumber,
  y = tonumber,
  z = tonumber,
  anchor = function(value)
    return SPAWN_ANCHORS[value] and value or nil
  end,
  flip_x = function(value)
    if value == "true" then
      return true
    elseif value == "false" then
      return false
    end
    return nil
  end,
}

--- Splits `key=value` arguments into a spawn opts table and a list of rejects.
local function parse_spawn_options(args)
  local opts, rejected = {}, {}
  for i = 2, #args do
    local token = args[i]
    if token ~= "" then
      local key, raw = token:match("^([%w_]+)=(.*)$")
      local reader = key and SPAWN_OPTIONS[key]
      local value = reader and reader(raw)
      if value == nil then
        table.insert(rejected, token)
      else
        opts[key] = value
      end
    end
  end
  return opts, rejected
end

vim.api.nvim_create_user_command("DistractSpawn", function(opts)
  local args = vim.split(vim.trim(opts.args), "%s+")
  local pet_type = args[1]
  if not pet_type or pet_type == "" then
    pet_type = "cat"
  end

  local spawn_opts, rejected = parse_spawn_options(args)
  if #rejected > 0 then
    -- Reported rather than ignored, and the spawn still happens: a typo in one
    -- option should not silently place the entity somewhere else entirely.
    vim.notify(
      string.format(
        "[Distract] Ignoring unrecognised spawn option(s): %s. Supported: %s.",
        table.concat(rejected, ", "),
        table.concat(vim.tbl_keys(SPAWN_OPTIONS), ", ")
      ),
      vim.log.levels.WARN
    )
  end

  distract().spawn(pet_type, spawn_opts)
end, {
  nargs = "*",
  desc = "Spawn an entity (e.g. cat, crab, sun), optionally with x=, y=, z=, anchor=, flip_x=",
  complete = function(_, line)
    local parts = vim.split(line, "%s+")
    if #parts <= 2 then
      return distract().get_asset_names()
    end
    return vim.tbl_map(function(key)
      return key .. "="
    end, vim.tbl_keys(SPAWN_OPTIONS))
  end,
})

vim.api.nvim_create_user_command("DistractAction", function(opts)
  local args = vim.split(vim.trim(opts.args), "%s+")
  local action_name = args[1]
  local target = args[2]

  if not action_name or action_name == "" then
    vim.notify("Usage: :DistractAction <action_name> [asset_name_or_id]", vim.log.levels.WARN)
    return
  end

  distract().action(action_name, target)
end, {
  nargs = "+",
  desc = "Trigger a custom action on entities (e.g. jump, yawn, clip, eclipse)",
  complete = function(_, line)
    local parts = vim.split(line, "%s+")
    if #parts == 2 then
      return distract().get_all_actions()
    elseif #parts == 3 then
      return distract().get_asset_names()
    end
    return {}
  end,
})

vim.api.nvim_create_user_command("DistractClear", function()
  distract().clear()
end, { desc = "Clear all active entities from screen" })

vim.api.nvim_create_user_command("DistractStatus", function()
  distract().status()
end, { desc = "Print status report of active entities" })

vim.api.nvim_create_user_command("DistractToggle", function()
  local ext = require("distract.external")
  local eng = require("distract.engine")
  if ext.is_running() or eng.is_running() then
    distract().stop()
  else
    distract().start()
  end
end, { desc = "Toggle Distract render engine" })

vim.api.nvim_create_user_command("DistractBuild", function()
  distract().build()
end, { desc = "Build the overlay engine binary in the background" })

vim.api.nvim_create_user_command("DistractDownload", function()
  distract().download()
end, { desc = "Download prebuilt overlay engine binary from GitHub releases" })

--- Fields `:DistractRender` accepts as `key=value`, and how to read each one.
local RENDER_OPTIONS = {
  mode = tostring,
  yaw = tonumber,
  fov = tonumber,
  depth = tonumber,
  slab = tonumber,
  ambient = tonumber,
}

--- Where each option lands in the `render` config table.
local function render_config_for(key, value)
  if key == "mode" then
    return { mode = value }
  elseif key == "yaw" then
    return { yaw_degrees = value }
  elseif key == "fov" then
    return { fov_y_degrees = value }
  elseif key == "depth" then
    return { depth_per_unit = value }
  elseif key == "slab" then
    return { voxel_depth = value }
  end
  return { light = { ambient = value } }
end

vim.api.nvim_create_user_command("DistractRender", function(opts)
  local args = vim.split(vim.trim(opts.args or ""), "%s+", { trimempty = true })
  if #args == 0 then
    local settings = distract().get_render()
    vim.notify(
      string.format(
        "[Distract] Render mode: '%s'\n"
          .. "  camera: %.0f° vertical, %.3f depth per z unit\n"
          .. "  model: turned %.0f°, %d voxels thick, fitted to %d wide\n"
          .. "  light: ambient %.2f",
        settings.mode,
        settings.fov_y_degrees,
        settings.depth_per_unit,
        settings.yaw_degrees,
        settings.voxel_depth,
        settings.voxel_max_width,
        settings.light.ambient
      ),
      vim.log.levels.INFO
    )
    return
  end

  local changes = {}
  for _, argument in ipairs(args) do
    local key, raw = argument:match("^(%w+)=(.+)$")
    if not key then
      -- A bare `2d` or `3d` is the whole point of the command; anything else
      -- would be a silent no-op, which is worse than a message.
      key, raw = "mode", argument
    end
    local reader = RENDER_OPTIONS[key]
    if not reader then
      vim.notify(
        string.format(
          "[Distract] Unknown render option '%s'. Accepts: %s",
          key,
          table.concat(vim.tbl_keys(RENDER_OPTIONS), ", ")
        ),
        vim.log.levels.ERROR
      )
      return
    end
    local value = reader(raw)
    if value == nil then
      vim.notify(
        string.format("[Distract] render %s needs a number, got '%s'", key, raw),
        vim.log.levels.ERROR
      )
      return
    end
    changes = vim.tbl_deep_extend("force", changes, render_config_for(key, value))
  end

  local ok, err = pcall(distract().set_render, changes)
  if not ok then
    vim.notify("[Distract] " .. tostring(err), vim.log.levels.ERROR)
    return
  end
  vim.notify(
    string.format("[Distract] Render mode: '%s'", distract().get_render().mode),
    vim.log.levels.INFO
  )
end, {
  nargs = "*",
  desc = "View or change how Distract draws: 2d, 3d, or key=value (yaw, fov, depth, slab, ambient)",
  complete = function(_, line)
    local parts = vim.split(line, "%s+")
    if #parts <= 2 then
      return { "2d", "3d", "yaw=", "fov=", "depth=", "slab=", "ambient=" }
    end
    return {}
  end,
})
