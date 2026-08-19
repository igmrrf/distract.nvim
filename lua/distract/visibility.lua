--- Whether this Neovim instance's sprites should be on screen.
---
--- A companion belongs to the editor it was spawned from. Both renderers draw
--- somewhere the editor does not own — the overlay is a separate always-on-top
--- OS window, and the in-terminal float survives whatever is in front of the
--- terminal — so an unfocused instance used to keep painting over whatever the
--- user had actually switched to, including a second Neovim.
---
--- Hiding is a *drawing* decision and never a simulation one. The step keeps
--- running while hidden, for the same reason `is_quiescent` gates the redraw and
--- not the step: an entity halfway through a wrap must not be stranded at the
--- edge of the screen until the user comes back.

local M = {}

--- Whether sprites are hidden when this instance loses focus.
---
--- On by default. `false` keeps the engine drawing regardless, which is what a
--- standalone desktop animation wants — the overlay engine is useful without
--- Neovim in front of it.
local is_restricted_to_instance = true

--- Whether this instance currently has focus, as far as the editor told us.
local has_focus = true

---@param opts table|nil the `setup` config
function M.configure(opts)
  if opts and opts.restrict_to_instance ~= nil then
    is_restricted_to_instance = opts.restrict_to_instance and true or false
  end
end

function M.is_restricted_to_instance()
  return is_restricted_to_instance
end

--- Whether the running backend should be drawing right now.
function M.is_visible()
  return has_focus or not is_restricted_to_instance
end

--- Records a focus change.
---@param gained boolean
---@return boolean whether what should be drawn changed
function M.set_focus(gained)
  local was_visible = M.is_visible()
  has_focus = gained and true or false
  return M.is_visible() ~= was_visible
end

--- For tests, and for a config reload.
function M.reset()
  is_restricted_to_instance = true
  has_focus = true
end

return M
