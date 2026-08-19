--- The rectangle sprites are allowed to move in, and what they must not cover.
---
--- Two different numbers live here and are deliberately not the same one:
---
--- * the **rect** is where the simulation may put an entity, in terminal cells,
---   and it is what wrapping, bouncing and clamping measure against;
--- * `z_index_offset` is Neovim float stacking, which decides what draws *over*
---   what. `position.z` is depth and parallax, and is unrelated to both.
---
--- Nothing here reads the buffer's contents. A provider registering solid
--- ground to walk on is `distract.obstacles`; this module answers "where is the
--- editor's own furniture" and nothing else.

local M = {}

M.EDITOR = "editor"
M.WINDOW = "window"
M.BUFFER = "buffer"
M.ABSOLUTE = "absolute"

local SCOPES = { [M.EDITOR] = true, [M.WINDOW] = true, [M.BUFFER] = true, [M.ABSOLUTE] = true }

--- Neovim float stacking. LSP hover and completion menus sit at 50 and above,
--- so the default puts a sprite underneath them rather than over the
--- documentation the user is reading.
local DEFAULT_Z_INDEX_OFFSET = 40

---@class DistractViewportConfig
---@field scope string `"editor"`, `"window"`, `"buffer"` or `"absolute"`
---@field exclude_floating boolean hide a sprite that would cover a floating window
---@field exclude_filetypes string[] windows whose sprites must never be covered
---@field z_index_offset integer Neovim float stacking for sprite surfaces
M.DEFAULTS = {
  -- The editor grid, which is what every release so far has used. `"buffer"`
  -- and `"window"` are opt-in because they move where existing pets live.
  scope = M.EDITOR,
  exclude_floating = true,
  exclude_filetypes = { "toggleterm", "lazy", "TelescopePrompt", "fzf", "help" },
  z_index_offset = DEFAULT_Z_INDEX_OFFSET,
}

local config = vim.deepcopy(M.DEFAULTS)

---@class DistractRect
---@field row integer top screen row, 0-based
---@field col integer left screen column, 0-based
---@field width integer
---@field height integer

--- Validates and stores the `positioning` block from `setup`.
---@param positioning table|nil
function M.configure(positioning)
  if not positioning then
    return
  end
  if positioning.scope ~= nil and not SCOPES[positioning.scope] then
    error(
      string.format(
        "distract: positioning.scope must be one of 'editor', 'window', 'buffer', 'absolute'; got '%s'",
        tostring(positioning.scope)
      )
    )
  end
  config = vim.tbl_deep_extend("force", config, positioning)
  if positioning.exclude_filetypes then
    -- A list is replaced, never merged: `tbl_deep_extend` would keep the
    -- defaults at the indices a shorter user list does not cover.
    config.exclude_filetypes = vim.deepcopy(positioning.exclude_filetypes)
  end
end

function M.reset()
  config = vim.deepcopy(M.DEFAULTS)
end

function M.scope()
  return config.scope
end

function M.z_index_offset()
  return config.z_index_offset or DEFAULT_Z_INDEX_OFFSET
end

local function editor_rect()
  return { row = 0, col = 0, width = vim.o.columns, height = vim.o.lines }
end

--- The current window's rect, or the editor's if it cannot be measured.
---
--- A window whose position cannot be read is not a failure worth reporting: a
--- headless run and a window closing mid-tick both land here, and falling back
--- to the editor grid is what the plugin did before scopes existed.
local function window_rect(text_area_only)
  local win = vim.api.nvim_get_current_win()
  local ok, pos = pcall(vim.api.nvim_win_get_position, win)
  if not ok or type(pos) ~= "table" then
    return editor_rect()
  end

  local width = vim.api.nvim_win_get_width(win)
  local height = vim.api.nvim_win_get_height(win)
  local col = pos[2]

  if text_area_only then
    -- The gutter: sign column, number column and fold column together. Sprites
    -- drawn over it look like they are outside the text.
    local info = vim.fn.getwininfo(win)[1]
    local gutter = info and info.textoff or 0
    col = col + gutter
    width = math.max(1, width - gutter)
  end

  return { row = pos[1], col = col, width = width, height = height }
end

--- Where entities may be, in terminal cells.
---@return DistractRect
function M.rect()
  local scope = config.scope
  if scope == M.WINDOW then
    return window_rect(false)
  end
  if scope == M.BUFFER then
    return window_rect(true)
  end
  return editor_rect()
end

--- The rect as the engines want their bounds: a size plus an origin.
---@return table `{ columns, lines, col, row }`
function M.bounds()
  local rect = M.rect()
  return { columns = rect.width, lines = rect.height, col = rect.col, row = rect.row }
end

local function is_excluded_filetype(buf)
  local ok, filetype = pcall(vim.api.nvim_get_option_value, "filetype", { buf = buf })
  if not ok then
    return false
  end
  return vim.tbl_contains(config.exclude_filetypes or {}, filetype)
end

--- Rects a sprite must not be drawn over, in cells.
---
--- Floating windows are the motivating case — an LSP hover or a completion menu
--- is what the user is reading — and a listed filetype is treated the same way
--- whether it floats or not, because a terminal split is just as much someone's
--- work as a popup is.
---@param ignored table<integer, boolean>|nil windows that are not obstacles
---@return DistractRect[]
function M.blocking_rects(ignored)
  if config.scope == M.ABSOLUTE then
    return {}
  end

  ignored = ignored or {}
  local blocked = {}
  for _, win in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
    local ok, win_config = pcall(vim.api.nvim_win_get_config, win)
    if ok and not ignored[win] then
      local is_float = win_config.relative ~= nil and win_config.relative ~= ""
      local buf = vim.api.nvim_win_get_buf(win)
      local blocks = (is_float and config.exclude_floating) or is_excluded_filetype(buf)
      local position_ok, pos = pcall(vim.api.nvim_win_get_position, win)
      if blocks and position_ok then
        table.insert(blocked, {
          row = pos[1],
          col = pos[2],
          width = vim.api.nvim_win_get_width(win),
          height = vim.api.nvim_win_get_height(win),
        })
      end
    end
  end
  return blocked
end

local function overlaps(left, right)
  return left.col < right.col + right.width
    and right.col < left.col + left.width
    and left.row < right.row + right.height
    and right.row < left.row + left.height
end

M.overlaps = overlaps

--- Whether a sprite's footprint would cover something the user is working in.
---@param rect DistractRect the sprite's footprint, in cells
---@param rects DistractRect[]|nil measured rects, when the caller already has them
---@return boolean
function M.is_blocked(rect, rects)
  for _, blocked in ipairs(rects or M.blocking_rects()) do
    if overlaps(rect, blocked) then
      return true
    end
  end
  return false
end

return M
