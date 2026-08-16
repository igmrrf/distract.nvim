--- Overlay backend: drives the compiled Rust engine over JSON-RPC on stdio.

local M = {}

local job_id = nil
local config = {}
local is_shutting_down = false
local build_job = nil

function M.setup(opts)
  config = opts or {}
end

local function plugin_root()
  return vim.fn.fnamemodify(debug.getinfo(1).source:sub(2), ":h:h:h")
end

local function exe_suffix()
  return vim.fn.has("win32") == 1 and ".exe" or ""
end

--- Where a compiled engine binary may live, most preferred first.
---
--- `engine/bin` is where a binary downloaded from a GitHub release should be
--- placed. The release workflow publishes per-platform archives, but nothing
--- looked anywhere they could plausibly be installed, so the published binaries
--- were unreachable and every user fell through to building from source.
function M.binary_candidates()
  local root = plugin_root()
  local ext = exe_suffix()
  return {
    root .. "/engine/bin/distract-engine" .. ext,
    root .. "/engine/target/release/distract-engine" .. ext,
    root .. "/engine/target/debug/distract-engine" .. ext,
  }
end

--- Locate the compiled Rust engine binary, or nil when none is installed.
local function find_binary()
  for _, path in ipairs(M.binary_candidates()) do
    if vim.fn.executable(path) == 1 or vim.fn.filereadable(path) == 1 then
      return path
    end
  end
  return nil
end

function M.build_command()
  return { "cargo", "build", "--release", "--manifest-path", plugin_root() .. "/engine/Cargo.toml" }
end

function M.is_running()
  return job_id ~= nil and job_id > 0
end

--- Compiles the engine without blocking the editor.
---
--- This used to be `vim.fn.system(...)`, which made Neovim completely
--- unresponsive for however long a cold Rust build takes — minutes — with a
--- single notification beforehand and no progress.
--- @param on_success function|nil called after a successful build
function M.build(on_success)
  if build_job then
    vim.notify("[Distract] Engine build already in progress.", vim.log.levels.INFO)
    return
  end

  local cmd = M.build_command()
  vim.notify(
    "[Distract] Building the overlay engine in the background:\n  "
      .. table.concat(cmd, " ")
      .. "\nThis can take a few minutes on a cold build.",
    vim.log.levels.INFO
  )

  local stderr_tail = {}
  build_job = vim.fn.jobstart(cmd, {
    on_stderr = function(_, data)
      for _, line in ipairs(data or {}) do
        if line ~= "" then
          table.insert(stderr_tail, line)
          -- Keep the last few lines only; a full cargo log is not a useful
          -- notification.
          if #stderr_tail > 20 then
            table.remove(stderr_tail, 1)
          end
        end
      end
    end,
    on_exit = function(_, code)
      build_job = nil
      if code == 0 then
        vim.notify("[Distract] Engine built.", vim.log.levels.INFO)
        if on_success then
          on_success()
        end
      else
        vim.notify(
          "[Distract] Engine build failed (exit "
            .. tostring(code)
            .. "):\n"
            .. table.concat(stderr_tail, "\n"),
          vim.log.levels.ERROR
        )
      end
    end,
  })

  if build_job <= 0 then
    build_job = nil
    vim.notify("[Distract] Could not start cargo. Is Rust installed?", vim.log.levels.ERROR)
  end
end

function M.start()
  if M.is_running() then
    return
  end
  is_shutting_down = false

  local bin_path = find_binary()
  if not bin_path then
    -- Refuse and say exactly what to do, rather than freezing the editor on a
    -- synchronous build the user did not ask for.
    vim.notify(
      "[Distract] Overlay engine is not built.\nRun :DistractBuild, or build it yourself:\n  "
        .. table.concat(M.build_command(), " "),
      vim.log.levels.WARN
    )
    return
  end

  job_id = vim.fn.jobstart({ bin_path }, {
    on_stdout = function(_, data)
      for _, line in ipairs(data) do
        if line ~= "" then
          M.handle_ipc_message(line)
        end
      end
    end,
    -- Engine diagnostics. Two sources are filtered out because they are
    -- unavoidable and not actionable: macOS emits an ApplePersistence warning
    -- for any non-bundled process, and wgpu logs adapter selection at startup.
    on_stderr = function(_, data)
      for _, line in ipairs(data) do
        if line ~= "" and not line:match("ApplePersistence") and not line:match("wgpu") then
          vim.schedule(function()
            vim.notify("[Distract engine] " .. line, vim.log.levels.DEBUG)
          end)
        end
      end
    end,
    on_exit = function(_, code)
      local was_clean = is_shutting_down or code == 0 or code == 143 or code == -1
      job_id = nil
      is_shutting_down = false
      if not was_clean then
        vim.notify(
          "[Distract] Engine terminated unexpectedly (code " .. tostring(code) .. ")",
          vim.log.levels.WARN
        )
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
    vim.notify(
      string.format("[Distract] Spawned %s (#%d) [%s]", msg.asset_name, msg.id, msg.state),
      vim.log.levels.INFO
    )
  elseif status == "action_triggered" then
    vim.notify(
      string.format("[Distract] %s (#%d) -> %s", msg.asset_name, msg.id, msg.action),
      vim.log.levels.INFO
    )
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
        table.insert(
          lines,
          string.format(
            "  • #%d %s (state: %s, pos: %.0f, %.0f)",
            ent.id,
            ent.asset_name,
            ent.state,
            ent.x,
            ent.y
          )
        )
      end
      vim.notify(table.concat(lines, "\n"), vim.log.levels.INFO)
    end
  elseif status == "error" then
    vim.notify("[Distract Error] " .. tostring(msg.message), vim.log.levels.ERROR)
  end
end

--- Sends a command, if the engine is running.
---
--- Deliberately does not start the engine. It used to, so `:DistractClear` or
--- `:DistractStatus` after `:DistractStop` spawned the overlay process again
--- purely to answer a question about entities that no longer exist.
function M.send_command(cmd_tbl)
  if not M.is_running() then
    return false
  end

  local encoded = vim.fn.json_encode(cmd_tbl)
  vim.fn.chansend(job_id, encoded .. "\n")
  return true
end

--- Sends a command, starting the engine first if it is not up.
--- Only for commands that are meant to bring the overlay to life.
local function send_or_start(cmd_tbl)
  if not M.is_running() then
    M.start()
  end
  return M.send_command(cmd_tbl)
end

function M.spawn(entity_name, opts)
  opts = opts or {}
  local asset = config.assets and config.assets[entity_name]

  local manifest_payload = nil
  local abs_path = nil

  if asset then
    -- Checked before it goes on the wire. The overlay validates it too, but a
    -- refusal that arrives back through the IPC error path is not the clean
    -- message the terminal backend gives, and one manifest should be refused
    -- with the same words whichever renderer is running.
    local violation = require("distract.locomotion").validate(asset)
    if violation then
      vim.notify(
        string.format("[Distract] Cannot spawn '%s': %s.", entity_name, violation),
        vim.log.levels.ERROR
      )
      return
    end

    -- Deep copy asset manifest
    manifest_payload = vim.deepcopy(asset)
    if manifest_payload.spritesheet then
      if next(manifest_payload.spritesheet) == nil or not manifest_payload.spritesheet.path then
        manifest_payload.spritesheet = nil
      else
        local p = manifest_payload.spritesheet.path
        if not p:match("^/") and not p:match("^%a:[/\\]") and not p:match("^~") then
          manifest_payload.spritesheet.path = plugin_root() .. "/" .. p
        else
          manifest_payload.spritesheet.path = vim.fn.expand(p)
        end
        abs_path = manifest_payload.spritesheet.path
      end
    end
  end

  -- Spawn coordinates are terminal cells on both backends. The overlay
  -- positions in physical pixels, so they are converted here rather than left
  -- for the caller: `spawn { x = 40 }` used to mean column 40 in the terminal
  -- and pixel 40 — roughly column 4 — on the overlay.
  local cell_w, cell_h = M.cell_size()
  local x = opts.x and (opts.x * cell_w) or nil
  local y = opts.y and (opts.y * cell_h) or nil

  send_or_start({
    command = "Spawn",
    entity_type = entity_name,
    path = abs_path,
    manifest = manifest_payload,
    x = x,
    y = y,
    flip_x = opts.flip_x or false,
  })
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

  send_or_start(cmd)
end

function M.despawn(id)
  M.send_command({ command = "Despawn", id = id })
end

function M.clear()
  if not M.send_command({ command = "ClearAll" }) then
    vim.notify("[Distract] Overlay engine is not running.", vim.log.levels.INFO)
  end
end

function M.get_status()
  if not M.send_command({ command = "GetStatus" }) then
    vim.notify("[Distract] Overlay engine is not running.", vim.log.levels.INFO)
  end
end

function M.send_event(event_type, context)
  M.send_command({
    command = "EditorEvent",
    event = event_type,
    context = context or vim.empty_dict(),
  })
end

--- Terminal cell size in physical pixels.
---
--- There is no portable way to ask a terminal for this from inside Neovim, and
--- it was previously hardcoded to 10x20 on both sides — so on any font that is
--- not exactly that, and never on a HiDPI display, overlay entities were
--- positioned against a coordinate space that matched nothing on screen.
---
--- Resolution order:
---   1. `cell_width` / `cell_height` from user config, if set.
---   2. The terminal's own report, when it answers `CSI 16 t`.
---   3. A documented 10x20 default.
---
--- See `:help distract-overlay`.
local DEFAULT_CELL_W, DEFAULT_CELL_H = 10, 20
local reported_cell = nil

--- Records a cell size reported by the terminal via `CSI 16 t`.
--- @param height number cell height in pixels
--- @param width number cell width in pixels
function M.set_reported_cell_size(height, width)
  if type(width) == "number" and type(height) == "number" and width > 0 and height > 0 then
    reported_cell = { width = width, height = height }
  end
end

function M.cell_size()
  local w = tonumber(config.cell_width)
  local h = tonumber(config.cell_height)
  if w and h and w > 0 and h > 0 then
    return w, h
  end
  if reported_cell then
    return reported_cell.width, reported_cell.height
  end
  return DEFAULT_CELL_W, DEFAULT_CELL_H
end

--- Asks the terminal for its cell size in pixels.
---
--- `CSI 16 t` is answered by kitty, WezTerm, Ghostty, foot and iTerm2, and
--- silently ignored elsewhere — so this is best effort and never blocks.
function M.query_cell_size()
  if vim.fn.has("nvim-0.10") ~= 1 then
    return
  end
  pcall(function()
    io.stdout:write("\27[16t")
  end)
end

function M.update_grid()
  local cw, ch = M.cell_size()
  M.send_command({
    command = "UpdateGrid",
    width = vim.o.columns,
    height = vim.o.lines,
    cell_width = cw,
    cell_height = ch,
  })
end

function M.stop()
  if M.is_running() then
    is_shutting_down = true
    local current_job = job_id
    pcall(function()
      local encoded = vim.fn.json_encode({ command = "Shutdown" })
      vim.fn.chansend(current_job, encoded .. "\n")
    end)

    -- Wait up to 100ms synchronously for clean process termination
    local res = vim.fn.jobwait({ current_job }, 100)
    if res and res[1] == -1 then
      pcall(vim.fn.jobstop, current_job)
    end
    job_id = nil
    is_shutting_down = false
  end
end

return M
