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
    vim.notify(
      string.format(
        "[Distract] Current backend: '%s' (available: %s)",
        distract().get_backend(),
        table.concat(distract().get_available_backends(), ", ")
      ),
      vim.log.levels.INFO
    )
  else
    distract().set_backend(backend)
  end
end, {
  nargs = "?",
  desc = "View or switch Distract rendering backend (halfblock, overlay)",
  complete = function(_, line)
    local parts = vim.split(line, "%s+")
    if #parts <= 2 then
      return distract().get_available_backends()
    end
    return {}
  end,
})

--- Option names `:DistractSpawn` accepts, and how to read their values.
---
--- `z` and `anchor` are deliberately absent: they are specified, but until the
--- overlay understands them too, accepting them here would ship a flag that
--- worked in the terminal and quietly did nothing on the other backend.
local SPAWN_OPTIONS = {
  x = tonumber,
  y = tonumber,
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
  desc = "Spawn an entity (e.g. cat, crab, sun), optionally with x=, y=, flip_x=",
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
