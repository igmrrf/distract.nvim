local M = {}

local engine_binary = require("distract.engine_binary")
local engine_download = require("distract.engine_download")
local highlights = require("distract.highlights")
local kitty = require("distract.kitty")
local plugins = require("distract.plugins")

--- The host's health reporters, under whichever names this Neovim has them.
---
--- `vim.health.start` and friends arrived in 0.10; 0.9 spells the same four
--- functions `report_start` / `report_ok` / `report_warn` / `report_info`. This
--- module is the only place that touches them, so the two spellings are
--- reconciled here rather than at each call. Resolved once at require time:
--- neither name appears and disappears during a session.
---@param modern string
---@param legacy string
---@return fun(message: string)
local function reporter(modern, legacy)
  local report = vim.health[modern] or vim.health[legacy]
  if type(report) ~= "function" then
    error(
      string.format(
        "distract.health: this Neovim has neither vim.health.%s nor vim.health.%s",
        modern,
        legacy
      )
    )
  end
  return report
end

local start = reporter("start", "report_start")
local report_ok = reporter("ok", "report_ok")
local report_warn = reporter("warn", "report_warn")
local report_info = reporter("info", "report_info")

local function report_terminal_environment()
  start("Terminal & Presentation Environment")

  if vim.fn.has("nvim-0.10") == 1 then
    report_ok("Neovim version is >= 0.10")
  else
    report_warn("Neovim version is older than 0.10; some virtual text features may be limited")
  end

  if vim.o.termguicolors then
    report_ok("termguicolors is enabled (truecolor supported)")
  else
    report_warn(
      "termguicolors is not enabled; halfblock renderer and Kitty placeholder IDs require truecolor"
    )
  end

  if kitty.is_available() then
    report_ok("Kitty graphics protocol is supported in this terminal")
  else
    report_info(
      "Kitty graphics protocol is unavailable; using halfblock truecolor unicode renderer"
    )
  end
end

local function report_overlay_engine()
  start("Overlay Engine (Hardware-Accelerated Window)")

  local binary_path = engine_binary.find()
  if binary_path then
    report_ok(string.format("Compiled engine binary found at: %s", binary_path))
  else
    report_info(
      "Precompiled overlay engine binary not found. Build it with :DistractBuild, "
        .. "or install a verified prebuilt one with :DistractDownload"
    )
  end

  local artifact = engine_download.detect_platform_artifact()
  if artifact then
    report_ok(string.format("A prebuilt binary is published for this platform: %s", artifact))
  else
    report_info("No prebuilt binary is published for this platform; it must be built from source")
  end

  if vim.fn.executable("cargo") == 1 then
    report_ok("Cargo / Rust toolchain is available for building from source")
  else
    report_info("Cargo / Rust toolchain not found on PATH")
  end

  if vim.fn.executable("curl") == 1 or vim.fn.executable("wget") == 1 then
    report_ok("curl or wget is available for :DistractDownload")
  else
    report_info("Neither curl nor wget is on PATH; :DistractDownload cannot fetch a binary")
  end

  if vim.fn.has("mac") == 1 then
    report_ok("macOS window server supports transparent click-through overlay")
  elseif vim.fn.has("win32") == 1 then
    report_ok("Windows desktop supports transparent layered overlay window")
  elseif vim.fn.has("unix") == 1 then
    if vim.env.WAYLAND_DISPLAY then
      report_ok("Wayland session detected")
    elseif vim.env.DISPLAY then
      report_warn(
        "X11 session detected: click-through overlay is disabled on X11 to prevent click trapping"
      )
    end
  end
end

local function report_runtime_configuration()
  start("Runtime State & Capacities")

  local distract = require("distract")
  local active_backend = distract.get_backend()
  report_info(string.format("Active backend: %s", active_backend))

  local render_settings = distract.get_render()
  report_info(
    string.format(
      "Render mode: %s (yaw: %.0f°, depth: %.2f, ambient: %.2f)",
      render_settings.mode,
      render_settings.yaw_degrees,
      render_settings.depth_per_unit,
      render_settings.light.ambient
    )
  )

  local live_groups = highlights.count()
  report_info(
    string.format("Allocated highlight groups: %d / %d", live_groups, highlights.DEFAULT_MAX_GROUPS)
  )

  local registered_assets = distract.get_asset_names()
  report_info(
    string.format(
      "Available assets (%d): %s",
      #registered_assets,
      table.concat(registered_assets, ", ")
    )
  )

  local registered_plugins = plugins.names()
  if #registered_plugins > 0 then
    report_info(
      string.format(
        "Registered plugins (%d): %s",
        #registered_plugins,
        table.concat(registered_plugins, ", ")
      )
    )
  end
end

function M.check()
  report_terminal_environment()
  report_overlay_engine()
  report_runtime_configuration()
end

return M
