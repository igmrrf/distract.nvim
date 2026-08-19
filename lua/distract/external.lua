--- Overlay backend: drives the compiled Rust engine over JSON-RPC on stdio.

local M = {}

local asset_path = require("distract.asset_path")
local engine_binary = require("distract.engine_binary")
local overlay_grid = require("distract.overlay_grid")
local overlay_plugins = require("distract.overlay_plugins")
local overlay_report = require("distract.overlay_report")
local viewport = require("distract.viewport")
local plugins = require("distract.plugins")
local locomotion = require("distract.locomotion")
local position = require("distract.position")

--- The cadence a plugin that only wants events subscribes at, in milliseconds.
--- The engine clamps anything slower.
local SLOWEST_SNAPSHOT_MS = 5000

--- The scope last pushed, so an unchanged rectangle costs nothing.
local pushed_scope = nil

local job_id = nil
local config = {}
local is_shutting_down = false

function M.setup(opts)
  config = opts or {}
  overlay_grid.configure(config)
end

function M.is_running()
  return job_id ~= nil and job_id > 0
end

--- Where a compiled engine binary may live, and how to build one.
M.binary_candidates = engine_binary.candidates
M.build_command = engine_binary.build_command
M.build = engine_binary.build

function M.overlay_args(overlay)
  if type(overlay) ~= "table" then
    return {}
  end

  local point = overlay.position
  if point ~= nil then
    if type(point) ~= "table" or type(point.x) ~= "number" or type(point.y) ~= "number" then
      return nil, "overlay.position must be { x = <number>, y = <number> }"
    end
    return {
      "--overlay-position",
      string.format("%d,%d", math.floor(point.x), math.floor(point.y)),
    }
  end

  local monitor = overlay.monitor
  if monitor ~= nil then
    if type(monitor) ~= "number" or monitor < 0 or monitor ~= math.floor(monitor) then
      return nil, "overlay.monitor must be a non-negative whole number (0 is the primary display)"
    end
    return { "--overlay-monitor", tostring(monitor) }
  end

  return {}
end

function M.start()
  if M.is_running() then
    return
  end
  is_shutting_down = false

  local bin_path = engine_binary.find()
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

  local overlay_args, overlay_err = M.overlay_args(require("distract").config.overlay)
  if not overlay_args then
    vim.notify("[Distract] " .. overlay_err, vim.log.levels.ERROR)
    return
  end

  local command = { bin_path }
  vim.list_extend(command, overlay_args)

  job_id = vim.fn.jobstart(command, {
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

  plugins.bind_world({ backend = "overlay", entities = overlay_plugins.entities })
  M.sync_plugin_subscription()
  pushed_scope = nil
  M.sync_viewport_scope()
  M.sync_render()

  -- Send viewport / grid bounds
  M.update_grid()
end

--- Pushes the render settings, so the overlay draws under the numbers Neovim
--- validated.
---
--- The same rule the floor, the viewport scope and the obstacle list follow: the
--- configuration is the editor's, and an engine that read its own would be free
--- to disagree with the terminal backends about what a session looks like.
---@param settings table|nil validated settings; the configured ones by default
function M.sync_render(settings)
  if not M.is_running() then
    return
  end
  M.send_command({
    command = "UpdateRender",
    settings = settings or require("distract").config.render,
  })
end

--- Tells the engine whether anything is listening, and on what cadence.
---
--- Off by default: a session with no plugins gets no per-frame traffic at all.
--- Called on start and whenever a plugin is registered or removed, because the
--- answer is derived from the registrations rather than stored.
function M.sync_plugin_subscription()
  if not M.is_running() then
    return
  end
  local snapshot_ms = overlay_plugins.desired_snapshot_ms()
  if not snapshot_ms and overlay_plugins.wants_world_events() then
    -- The journal only records while the engine is subscribed, so an events-only
    -- plugin still subscribes; the slowest cadence keeps snapshots off the wire
    -- in all but name.
    snapshot_ms = SLOWEST_SNAPSHOT_MS
  end
  M.send_command({ command = "Subscribe", snapshot_ms = snapshot_ms })
end

function M.handle_ipc_message(raw_json)
  local ok, msg = pcall(vim.fn.json_decode, raw_json)
  if not ok or type(msg) ~= "table" then
    return
  end

  if overlay_report.notify(msg) then
    return
  end

  if msg.status == "snapshot" then
    local cell_width, cell_height = M.cell_size()
    overlay_plugins.on_snapshot(msg, { width = cell_width, height = cell_height })
    overlay_plugins.flush_commands(M.send_command)
  elseif msg.status == "world_events" then
    overlay_plugins.on_world_events(msg)
    overlay_plugins.flush_commands(M.send_command)
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
        manifest_payload.spritesheet.path = asset_path.resolve(manifest_payload.spritesheet.path)
        abs_path = manifest_payload.spritesheet.path
      end
    end
  end

  -- Spawn coordinates are terminal cells on both backends. The overlay
  -- positions in physical pixels, so they are converted here rather than left
  -- for the caller: `spawn { x = 40 }` used to mean column 40 in the terminal
  -- and pixel 40 — roughly column 4 — on the overlay.
  local cell_w, cell_h = M.cell_size()
  local placement = M.resolve_placement(asset, opts)

  send_or_start({
    command = "Spawn",
    entity_type = entity_name,
    path = abs_path,
    manifest = manifest_payload,
    x = placement.x and (placement.x * cell_w) or nil,
    y = placement.y and (placement.y * cell_h) or nil,
    z = placement.z,
    parallax = placement.parallax,
    anchor = placement.anchor,
    flip_x = opts.flip_x or false,
  })
end

--- Resolves a spawn's placement into what the engine needs to be told.
---
--- Anchors are resolved to a concrete name here rather than sent as `auto`,
--- because deciding `auto` needs the manifest's locomotion class. The vertical
--- arithmetic stays on the engine's side: it knows the sprite's frame height in
--- pixels, and the floor it was pushed covers the rest.
---@param asset table|nil the manifest, when one is registered
---@param opts table per-spawn options
---@return table `{ x, y, z, parallax, anchor }`; positions in cells
function M.resolve_placement(asset, opts)
  local settings = position.settings(config.position, opts)
  local manifest = asset or {}
  local initial_def = manifest.states and manifest.states[manifest.initial_state]
  local anchor = position.effective_anchor(
    settings.anchor,
    position.manifest_anchor(asset),
    locomotion.locomotion_for(manifest, initial_def)
  )

  local x, y, z = opts.x, opts.y, opts.z
  if type(anchor) == "table" then
    x, y, z = x or anchor.x, y or anchor.y, z or anchor.z
    anchor = nil
  end

  return {
    x = x,
    y = y,
    z = z,
    parallax = position.parallax_for(z, settings, "overlay"),
    -- An explicit position leaves nothing for an anchor to decide.
    anchor = (x == nil or y == nil) and anchor or nil,
  }
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

--- The geometry the engine is told about: cell size and the floor.
M.set_reported_cell_size = overlay_grid.set_reported_cell_size
M.cell_size = overlay_grid.cell_size
M.query_cell_size = overlay_grid.query_cell_size
M.get_ground_row = overlay_grid.get_ground_row

--- Pushes the registered obstacles, converted to overlay pixels.
---
--- Sent whenever the collection ran rather than diffed: a provider's answer
--- changes with the buffer, and comparing two lists of rectangles costs more than
--- the message does.
---@param rects table[] rectangles in terminal cells
function M.set_obstacles(rects)
  if not M.is_running() then
    return
  end

  local cell_width, cell_height = M.cell_size()
  local converted = {}
  for _, rect in ipairs(rects or {}) do
    table.insert(converted, {
      x = rect.x * cell_width,
      y = rect.y * cell_height,
      width = rect.width * cell_width,
      height = rect.height * cell_height,
      type = rect.type,
    })
  end

  M.send_command({ command = "UpdateObstacles", obstacles = converted })
end

--- Pushes the rectangle entities may move in, if it moved.
---
--- Measured in Neovim, in cells, and converted here: the engine cannot see a
--- window's text area, what is floating over it or which splits the user is
--- working in. An `editor` scope sends no rectangle at all, which returns the
--- engine to the whole overlay window.
function M.sync_viewport_scope()
  if not M.is_running() then
    return
  end

  local cell_width, cell_height = M.cell_size()
  local scope = nil
  if viewport.scope() ~= viewport.EDITOR and viewport.scope() ~= viewport.ABSOLUTE then
    local rect = viewport.rect()
    scope = {
      x = rect.col * cell_width,
      y = rect.row * cell_height,
      width = rect.width * cell_width,
      height = rect.height * cell_height,
    }
  end

  local signature = scope and table.concat({ scope.x, scope.y, scope.width, scope.height }, ":")
    or "editor"
  if signature == pushed_scope then
    return
  end
  pushed_scope = signature

  M.send_command({
    command = "UpdateViewportScope",
    x = scope and scope.x or nil,
    y = scope and scope.y or nil,
    width = scope and scope.width or nil,
    height = scope and scope.height or nil,
  })
end

--- Shows or hides the overlay window.
---
--- The window belongs to the engine process, so this is the only way to hide it;
--- the simulation there keeps running for the same reason it does in-terminal.
---@param is_visible boolean
function M.set_visible(is_visible)
  M.send_command({ command = "SetVisible", visible = is_visible })
end

function M.update_grid()
  M.send_command(overlay_grid.grid_command())
end

function M.set_ground_row(row)
  if overlay_grid.set_ground_row(row) then
    M.update_grid()
  end
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
  plugins.dispatch_teardown()
  plugins.unbind_world()
  overlay_plugins.reset()
end

return M
