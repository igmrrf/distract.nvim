local M = {}

local job_id = nil
local config = {}
local is_shutting_down = false

function M.setup(opts)
  config = opts or {}
end

--- Locate the compiled Rust engine binary.
local function get_binary_path()
  local plugin_root = vim.fn.fnamemodify(debug.getinfo(1).source:sub(2), ":h:h:h")
  local is_win = vim.fn.has("win32") == 1
  local ext = is_win and ".exe" or ""
  local release_bin = plugin_root .. "/engine/target/release/distract-engine" .. ext
  local debug_bin = plugin_root .. "/engine/target/debug/distract-engine" .. ext

  if vim.fn.filereadable(release_bin) == 1 then
    return release_bin
  elseif vim.fn.filereadable(debug_bin) == 1 then
    return debug_bin
  else
    return release_bin
  end
end


function M.is_running()
  return job_id ~= nil and job_id > 0
end

function M.start()
  if M.is_running() then
    return
  end
  is_shutting_down = false

  local bin_path = get_binary_path()

  if vim.fn.filereadable(bin_path) == 0 then
    vim.notify("[Distract] Engine binary not found at " .. bin_path .. ". Compiling with cargo...", vim.log.levels.INFO)
    local plugin_root = vim.fn.fnamemodify(debug.getinfo(1).source:sub(2), ":h:h:h")
    local build_cmd = "cargo build --release --manifest-path " .. plugin_root .. "/engine/Cargo.toml"
    local build_out = vim.fn.system(build_cmd)
    if vim.v.shell_error ~= 0 then
      vim.notify("[Distract] Failed to compile engine:\n" .. build_out, vim.log.levels.ERROR)
      return
    end
  end

  job_id = vim.fn.jobstart({ bin_path }, {
    on_stdout = function(_, data)
      for _, line in ipairs(data) do
        if line ~= "" then
          M.handle_ipc_message(line)
        end
      end
    end,
    on_stderr = function(_, data)
      for _, line in ipairs(data) do
        if line ~= "" and not line:match("ApplePersistence") and not line:match("wgpu") then
          -- Log engine debug output
        end
      end
    end,
    on_exit = function(_, code)
      local was_clean = is_shutting_down or code == 0
      job_id = nil
      is_shutting_down = false
      if not was_clean then
        vim.notify("[Distract] Engine terminated unexpectedly (code " .. tostring(code) .. ")", vim.log.levels.WARN)
      end
    end,
  })

  if job_id <= 0 then
    vim.notify("[Distract] Failed to start engine process.", vim.log.levels.ERROR)
    job_id = nil
    return
  end

  -- Send viewport / grid bounds
  M.update_grid()
end

function M.handle_ipc_message(raw_json)
  local ok, msg = pcall(vim.fn.json_decode, raw_json)
  if not ok or type(msg) ~= "table" then
    return
  end

  local status = msg.status
  if status == "ready" then
    vim.notify("[Distract] Engine v" .. tostring(msg.version) .. " active", vim.log.levels.INFO)
  elseif status == "spawned" then
    vim.notify(string.format("[Distract] Spawned %s (#%d) [%s]", msg.asset_name, msg.id, msg.state), vim.log.levels.INFO)
  elseif status == "action_triggered" then
    vim.notify(string.format("[Distract] %s (#%d) -> %s", msg.asset_name, msg.id, msg.action), vim.log.levels.INFO)
  elseif status == "despawned" then
    vim.notify(string.format("[Distract] Despawned entity #%d", msg.id), vim.log.levels.INFO)
  elseif status == "cleared" then
    vim.notify("[Distract] All entities cleared", vim.log.levels.INFO)
  elseif status == "status_report" then
    local count = msg.count or 0
    if count == 0 then
      vim.notify("[Distract] No active entities.", vim.log.levels.INFO)
    else
      local lines = { string.format("[Distract] %d active entities:", count) }
      for _, ent in ipairs(msg.entities or {}) do
        table.insert(lines, string.format("  • #%d %s (state: %s, pos: %.0f, %.0f)", ent.id, ent.asset_name, ent.state, ent.x, ent.y))
      end
      vim.notify(table.concat(lines, "\n"), vim.log.levels.INFO)
    end
  elseif status == "error" then
    vim.notify("[Distract Error] " .. tostring(msg.message), vim.log.levels.ERROR)
  end
end

function M.send_command(cmd_tbl)
  if not M.is_running() then
    M.start()
  end
  if not M.is_running() then
    return
  end

  local encoded = vim.fn.json_encode(cmd_tbl)
  vim.fn.chansend(job_id, encoded .. "\n")
end

function M.spawn(entity_name, opts)
  opts = opts or {}
  local asset = config.assets and config.assets[entity_name]

  local manifest_payload = nil
  local abs_path = nil

  if asset then
    -- Deep copy asset manifest
    manifest_payload = vim.deepcopy(asset)
    if manifest_payload.spritesheet then
      if next(manifest_payload.spritesheet) == nil or not manifest_payload.spritesheet.path then
        manifest_payload.spritesheet = nil
      else
        local plugin_root = vim.fn.fnamemodify(debug.getinfo(1).source:sub(2), ":h:h:h")
        local p = manifest_payload.spritesheet.path
        if not p:match("^/") and not p:match("^%a:[/\\]") and not p:match("^~") then
          manifest_payload.spritesheet.path = plugin_root .. "/" .. p
        else
          manifest_payload.spritesheet.path = vim.fn.expand(p)
        end
        abs_path = manifest_payload.spritesheet.path
      end
    end
  end

  local spawn_cmd = {
    command = "Spawn",
    entity_type = entity_name,
    path = abs_path,
    manifest = manifest_payload,
    x = opts.x or (vim.o.columns / 2 * 10),
    y = opts.y or (vim.o.lines / 2 * 20),
    flip_x = opts.flip_x or false,
  }

  M.send_command(spawn_cmd)
end

function M.trigger_action(action_name, target)
  local cmd = {
    command = "TriggerAction",
    action = action_name,
  }
  if type(target) == "number" then
    cmd.id = target
  elseif type(target) == "string" and target ~= "" then
    cmd.asset_name = target
  end

  M.send_command(cmd)
end

function M.despawn(id)
  M.send_command({
    command = "Despawn",
    id = id,
  })
end

function M.clear()
  M.send_command({
    command = "ClearAll",
  })
end

function M.get_status()
  M.send_command({
    command = "GetStatus",
  })
end

function M.send_event(event_type, context)
  if not M.is_running() then
    return
  end
  M.send_command({
    command = "EditorEvent",
    event = event_type,
    context = context or {},
  })
end

function M.update_grid()
  if not M.is_running() then
    return
  end
  M.send_command({
    command = "UpdateGrid",
    width = vim.o.columns,
    height = vim.o.lines,
    cell_width = 10,
    cell_height = 20,
  })
end

function M.stop()
  if M.is_running() then
    is_shutting_down = true
    M.send_command({ command = "Shutdown" })

    -- Give the process up to 300ms to shut down cleanly before jobstop
    local current_job = job_id
    vim.defer_fn(function()
      if job_id == current_job then
        vim.fn.jobstop(current_job)
        job_id = nil
      end
    end, 300)
  end
end

return M
