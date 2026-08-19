local M = {}

local engine_binary = require("distract.engine_binary")
local highlights = require("distract.highlights")
local kitty = require("distract.kitty")
local plugins = require("distract.plugins")

local function report_terminal_environment()
  vim.health.start("Terminal & Presentation Environment")

  if vim.fn.has("nvim-0.10") == 1 then
    vim.health.ok("Neovim version is >= 0.10")
  else
    vim.health.warn("Neovim version is older than 0.10; some virtual text features may be limited")
  end

  if vim.o.termguicolors then
    vim.health.ok("termguicolors is enabled (truecolor supported)")
  else
    vim.health.warn(
      "termguicolors is not enabled; halfblock renderer and Kitty placeholder IDs require truecolor"
    )
  end

  if kitty.is_available() then
    vim.health.ok("Kitty graphics protocol is supported in this terminal")
  else
    vim.health.info(
      "Kitty graphics protocol is unavailable; using halfblock truecolor unicode renderer"
    )
  end
end

local function report_overlay_engine()
  vim.health.start("Overlay Engine (Hardware-Accelerated Window)")

  local binary_path = engine_binary.find()
  if binary_path then
    vim.health.ok(string.format("Compiled engine binary found at: %s", binary_path))
  else
    vim.health.info(
      "Precompiled overlay engine binary not found. Build with :DistractBuild or download with :DistractDownload"
    )
  end

  if vim.fn.executable("cargo") == 1 then
    vim.health.ok("Cargo / Rust toolchain is available for building from source")
  else
    vim.health.info("Cargo / Rust toolchain not found on PATH")
  end

  if vim.fn.has("mac") == 1 then
    vim.health.ok("macOS window server supports transparent click-through overlay")
  elseif vim.fn.has("win32") == 1 then
    vim.health.ok("Windows desktop supports transparent layered overlay window")
  elseif vim.fn.has("unix") == 1 then
    if vim.env.WAYLAND_DISPLAY then
      vim.health.ok("Wayland session detected")
    elseif vim.env.DISPLAY then
      vim.health.warn(
        "X11 session detected: click-through overlay is disabled on X11 to prevent click trapping"
      )
    end
  end
end

local function report_runtime_configuration()
  vim.health.start("Runtime State & Capacities")

  local distract = require("distract")
  local active_backend = distract.get_backend()
  vim.health.info(string.format("Active backend: %s", active_backend))

  local render_settings = distract.get_render()
  vim.health.info(
    string.format(
      "Render mode: %s (yaw: %.0f°, depth: %.2f, ambient: %.2f)",
      render_settings.mode,
      render_settings.yaw_degrees,
      render_settings.depth_per_unit,
      render_settings.light.ambient
    )
  )

  local live_groups = highlights.count()
  vim.health.info(
    string.format("Allocated highlight groups: %d / %d", live_groups, highlights.DEFAULT_MAX_GROUPS)
  )

  local registered_assets = distract.get_asset_names()
  vim.health.info(
    string.format(
      "Available assets (%d): %s",
      #registered_assets,
      table.concat(registered_assets, ", ")
    )
  )

  local registered_plugins = plugins.names()
  if #registered_plugins > 0 then
    vim.health.info(
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
