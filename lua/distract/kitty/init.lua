--- The kitty graphics protocol backend.
---
--- Registration is deliberate rather than automatic. The backend exists only if
--- the terminal on the other end can draw for it, so it is offered as a choice
--- only once that has been established -- otherwise `:DistractBackend kitty`
--- would advertise something that renders as a screenful of unknown codepoints.
---
--- Until it registers, `kitty`, `ghostty` and `wezterm` stay substitutions in
--- `distract.backends` and resolve to the half-block renderer with a notice.

local M = {}

local backends = require("distract.backends")
local detect = require("distract.kitty.detect")
local kitty_renderer = require("distract.kitty.renderer")
local renderer = require("distract.renderer")

M.NAME = "kitty"

--- Terminals whose own name should reach this backend.
M.ALIASES = { "ghostty", "wezterm" }

--- Per-pixel alpha and a resamplable placement, which is what `z` needs to mean
--- parallax rather than draw order alone.
---@type DistractBackendCapabilities
M.CAPABILITIES = { scale = true, alpha = "pixel", native_resolution = true }

--- Requests that should make this module probe the terminal.
---
--- Probing costs an escape sequence and up to `detect.RESPONSE_TIMEOUT_MS`, so
--- a session that never asks for kitty never pays for it.
local WANTED = { kitty = true, ghostty = true, wezterm = true }

local warned_no_truecolor = false

local function warn_no_truecolor()
  if warned_no_truecolor then
    return
  end
  warned_no_truecolor = true
  vim.notify(
    "[Distract] The kitty backend needs `termguicolors`: a placeholder cell "
      .. "carries its image id in its foreground colour, and without truecolor "
      .. "Neovim rounds that id to the nearest palette entry.",
    vim.log.levels.WARN
  )
end

local registered = false

--- Whether this terminal can draw kitty graphics for us.
---@return boolean
function M.is_available()
  if not vim.o.termguicolors then
    warn_no_truecolor()
    return false
  end
  return detect.is_available()
end

--- Registers the backend, if the terminal supports it.
---
--- Both registrations happen together or neither does: a name offered by
--- `distract.backends` that `distract.renderer` cannot draw is a backend that
--- exists on paper only, which is the failure the capability table was
--- introduced to prevent.
---@return boolean registered
function M.setup()
  if registered then
    return true
  end
  if not M.is_available() then
    return false
  end

  renderer.register_backend(M.NAME, kitty_renderer.surface, kitty_renderer.reset)
  backends.register(M.NAME, M.CAPABILITIES, M.ALIASES)
  registered = true
  return true
end

--- Registers without being asked, when the environment already names a
--- terminal confirmed to implement the protocol.
---
--- This is what lets `:DistractBackend` offer `kitty` at all: a backend that
--- only appears once you have typed its name is one nobody discovers. The
--- environment check sends nothing and waits for nothing, so a terminal that is
--- not on the list still has to be asked for by name -- and being offered is
--- not being selected, the default backend is unchanged either way.
---@return boolean registered
function M.ensure_offered()
  if registered then
    return true
  end
  if not detect.env_says_yes() then
    return false
  end
  return M.setup()
end

--- Registers the backend if, and only if, this is the backend being asked for.
---@param requested string|nil the backend name the user configured or selected
---@return boolean registered
function M.ensure_registered(requested)
  if registered then
    return true
  end
  if type(requested) ~= "string" or not WANTED[string.lower(vim.trim(requested))] then
    return false
  end
  return M.setup()
end

--- Whether the backend has registered itself.
---@return boolean
function M.is_registered()
  return registered
end

--- Deletes every transmitted image and forgets the terminal's answer.
---
--- The registry is process-wide, so a spec that registers has to put it back;
--- `distract.backends.reset()` is the other half.
function M.reset()
  kitty_renderer.reset()
  renderer.unregister_backend(M.NAME)
  detect.reset()
  registered = false
  warned_no_truecolor = false
end

return M
