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

vim.api.nvim_create_user_command("DistractSpawn", function(opts)
  local args = vim.split(vim.trim(opts.args), "%s+")
  local pet_type = args[1]
  if not pet_type or pet_type == "" then
    pet_type = "cat"
  end
  distract().spawn(pet_type)
end, {
  nargs = "?",
  desc = "Spawn an entity (e.g. cat, crab, sun)",
  complete = function(_, line)
    local parts = vim.split(line, "%s+")
    if #parts <= 2 then
      return distract().get_asset_names()
    end
    return {}
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
