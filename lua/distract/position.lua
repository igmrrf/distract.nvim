--- Where an entity is placed, and what it stands on.
---
--- Both floors are computed here, in Lua, for both backends. `external.lua`
--- already owns the cells-to-pixels conversion at the IPC boundary, so it owns
--- this too and the overlay never needs a buffer concept.
---
--- Units. A floor is a screen row in **terminal cells**: the exclusive bottom
--- edge an entity's feet rest on, so an entity of height `h` has its top-left
--- at `floor_row - h`. Positions are cells; `z` is dimensionless.

local M = {}

local backends = require("distract.backends")
local locomotion = require("distract.locomotion")

--- Anchor the placement derives from the entity's own locomotion: a grounded
--- animal starts on the floor, something that flies starts where it is put.
M.AUTO = "auto"
M.BOTTOM = "bottom"
M.TOP = "top"
--- No anchor at all: the middle of the screen, which is where everything
--- spawned before floors existed.
M.FREE = "free"

M.SCREEN = "screen"
M.TEXT = "text"

--- Placement settings, before a spawn overrides any of them.
---@class DistractPositionConfig
---@field anchor string|table `"auto"`, `"bottom"`, `"top"`, `"free"`, or `{ x, y, z }`
---@field ground string `"screen"` or `"text"`
---@field parallax table `{ per_unit, min, max }`
M.DEFAULTS = {
  anchor = M.AUTO,
  ground = M.SCREEN,
  -- Off unless asked for: `per_unit = 0` makes every parallax factor exactly
  -- 1, so no existing configuration changes behaviour.
  parallax = { per_unit = 0.0, min = 0.4, max = 1.6 },
}

--- Rows the statusline takes off the bottom of the screen.
local function statusline_rows()
  local mode = vim.o.laststatus
  if mode == 0 then
    return 0
  end
  if mode ~= 1 then
    return 1
  end

  local windows = 0
  for _, win in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
    local ok, cfg = pcall(vim.api.nvim_win_get_config, win)
    if ok and (cfg.relative == nil or cfg.relative == "") then
      windows = windows + 1
    end
  end
  return windows > 1 and 1 or 0
end

--- The bottom of the usable screen, in cells.
---
--- `cmdheight` and the statusline are the editor's, not ours: an entity that
--- walks over them is standing on chrome rather than on the floor.
---@return number
function M.screen_floor_row()
  return math.max(1, vim.o.lines - vim.o.cmdheight - statusline_rows())
end

--- The screen row the last line of the current buffer starts on, in cells.
---
--- Nil when that row cannot be addressed: `screenpos` reports row 0 for a
--- folded line, and a wrapped line occupies rows that no line number maps to.
--- Callers fall back to the screen floor rather than guessing.
---@return number|nil
function M.text_floor_row()
  local ok, wins = pcall(vim.fn.getwininfo, vim.api.nvim_get_current_win())
  local info = ok and wins and wins[1]
  if not info then
    return nil
  end

  local line_count = vim.api.nvim_buf_line_count(info.bufnr)
  local last_visible = math.min(info.botline or line_count, line_count)
  if last_visible < 1 then
    return nil
  end

  local pos_ok, pos = pcall(vim.fn.screenpos, info.winid, last_visible, 1)
  if not pos_ok or not pos or pos.row == nil or pos.row <= 0 then
    return nil
  end
  -- `screenpos` rows are 1-based, and an entity rests *on top of* the last
  -- line rather than over it, so its 0-based row is the floor.
  return pos.row - 1
end

--- The floor an entity stands on, in cells.
---@param ground string `"screen"` or `"text"`
---@return number
function M.floor_row(ground)
  if ground ~= M.TEXT then
    return M.screen_floor_row()
  end
  return M.text_floor_row() or M.screen_floor_row()
end

--- Placement settings with a spawn's overrides applied.
---@param config_position table|nil the `position` block from `setup`
---@param opts table|nil per-spawn options
---@return DistractPositionConfig
function M.settings(config_position, opts)
  opts = opts or {}
  local settings = vim.tbl_deep_extend("force", vim.deepcopy(M.DEFAULTS), config_position or {})
  if opts.anchor ~= nil then
    settings.anchor = opts.anchor
  end
  if opts.ground ~= nil then
    settings.ground = opts.ground
  end
  return settings
end

--- How far an entity's motion is damped by its distance.
---
--- `per_unit` defaults to zero, so this is exactly 1 until a configuration asks
--- for depth. The same factor damps both axes, so a distant thing drifts slower
--- in x and falls slower in y rather than moving diagonally against itself.
---@param z number|nil
---@param parallax table|nil `{ per_unit, min, max }`
---@return number
function M.parallax_factor(z, parallax)
  local settings = parallax or M.DEFAULTS.parallax
  local per_unit = settings.per_unit or 0.0
  if per_unit == 0.0 or not z or z == 0 then
    return 1.0
  end
  local minimum = settings.min or M.DEFAULTS.parallax.min
  local maximum = settings.max or M.DEFAULTS.parallax.max
  return math.min(maximum, math.max(minimum, 1.0 + z * per_unit))
end

--- The parallax factor a backend will actually honour.
---
--- A backend that cannot scale a sprite cannot show depth, so it is told once
--- and the factor collapses to 1 rather than damping motion the user would see
--- no depth cue for. `z` still sets draw order there.
---@param z number|nil
---@param settings DistractPositionConfig
---@param backend string canonical backend name
---@return number
function M.parallax_for(z, settings, backend)
  local parallax = settings.parallax or M.DEFAULTS.parallax
  if (parallax.per_unit or 0.0) == 0.0 or not z or z == 0 then
    return 1.0
  end
  if not backends.supports_parallax(backend) then
    backends.warn_parallax_unsupported(backend)
    return 1.0
  end
  return M.parallax_factor(z, parallax)
end

--- Anchors an asset may declare for itself.
local DECLARABLE = { [M.AUTO] = true, [M.BOTTOM] = true, [M.TOP] = true, [M.FREE] = true }

--- The anchor an asset declares, if it declares one.
---
--- Where a sun belongs is a property of a sun, not of a user's configuration,
--- the same way `z_index` and `locomotion` are. A typo is refused at the point
--- of asking rather than silently placing the asset in the middle of the
--- screen.
---@param manifest table|nil
---@return string|nil
function M.manifest_anchor(manifest)
  local declared = manifest and manifest.anchor
  if declared == nil then
    return nil
  end
  if not DECLARABLE[declared] then
    error(
      string.format(
        "distract: asset '%s' declares anchor '%s'; expected one of auto, bottom, top, free",
        tostring(manifest.name),
        tostring(declared)
      )
    )
  end
  return declared
end

--- The anchor an entity actually uses.
---
--- Three sources, most specific first: what this spawn or configuration asked
--- for, what the asset declares about itself, and finally what the entity can
--- physically do -- gravity binds the cat and the crab to the floor, and the
--- sun may drift anywhere. Each falls through to the next only by saying
--- `auto`, which is the vocabulary's way of having no opinion.
---@param requested string|table `"auto"` when nothing was asked for
---@param declared string|nil the asset's own preference
---@param entity_locomotion string
---@return string|table
function M.effective_anchor(requested, declared, entity_locomotion)
  if requested ~= M.AUTO then
    return requested
  end
  if declared ~= nil and declared ~= M.AUTO then
    return declared
  end
  if entity_locomotion == locomotion.OMNIDIRECTIONAL then
    return M.FREE
  end
  return M.BOTTOM
end

--- Where a spawn is placed and what it stands on.
---
---@class DistractPlacementRequest
---@field settings DistractPositionConfig placement settings for this spawn
---@field backend string canonical backend name, for the parallax capability
---@field locomotion string the initial state's locomotion class
---@field declared_anchor string|nil the anchor the asset declares for itself
---@field floor_row number|nil the floor in cells, or nil when none is measured
---@field sprite_h number sprite height, in cells, before parallax
---@field bounds table `{ columns, lines }`, in cells
---@field opts table per-spawn `x` / `y` / `z` overrides

--- Resolves a placement request into a spawn position.
---
--- Parallax shrinks the drawn sprite, so it shrinks the height subtracted from
--- the floor too -- otherwise a distant entity's feet would hang above it.
---
--- With no floor measured, `ground_y` is nil and a `bottom` anchor has nothing
--- to stand on, so the spawn falls back to the middle of the screen. That is
--- what `World::spawn` does with no floor pushed, and the two have to agree.
---@param request DistractPlacementRequest
---@return table `{ x, y, z, parallax, ground_y }` in cells, except the
---dimensionless `z`, which is nil when nothing asked for a depth
function M.placement(request)
  local opts = request.opts or {}
  local settings = request.settings
  local anchor = M.effective_anchor(settings.anchor, request.declared_anchor, request.locomotion)

  local placed = {}
  if type(anchor) == "table" then
    placed.x = anchor.x
    placed.y = anchor.y
    placed.z = anchor.z
  end

  local z = opts.z or placed.z
  local parallax = M.parallax_for(z, settings, request.backend)
  local ground_y = request.floor_row and (request.floor_row - request.sprite_h * parallax)

  if anchor == M.BOTTOM then
    placed.y = ground_y
  elseif anchor == M.TOP then
    placed.y = 0
  end

  return {
    x = opts.x or placed.x or math.floor(request.bounds.columns / 2),
    y = opts.y or placed.y or math.floor(request.bounds.lines / 2),
    z = z,
    parallax = parallax,
    ground_y = ground_y,
  }
end

return M
