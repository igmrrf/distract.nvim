--- Where a manifest's `spritesheet.path` actually is on disk.
---
--- One resolver rather than one per backend: the overlay sends an absolute path
--- over IPC and the in-terminal backends open the same file themselves, so two
--- answers to "relative to what?" is the divergence class this plugin keeps
--- finding. A relative path is relative to the plugin root, which is what makes
--- `assets/cat_walking_1.gif` mean the same thing in a manifest wherever the
--- editor was started from.

local M = {}

local GIF_EXTENSION = "%.gif$"

--- The directory this plugin is installed in.
function M.plugin_root()
  return vim.fn.fnamemodify(debug.getinfo(1).source:sub(2), ":h:h:h")
end

--- Absolute path for a manifest-declared asset path.
---@param path string
---@return string
function M.resolve(path)
  local is_absolute = path:match("^/") or path:match("^%a:[/\\]") or path:match("^~")
  if not is_absolute then
    return M.plugin_root() .. "/" .. path
  end
  return vim.fn.expand(path)
end

--- Whether a path names a GIF, which is the one image format the in-terminal
--- backends can decode without a compiled engine.
---@param path string|nil
---@return boolean
function M.is_gif(path)
  return type(path) == "string" and path:lower():match(GIF_EXTENSION) ~= nil
end

return M
