--- In-terminal renderer.
---
--- A sprite is drawn on two different surfaces, because neither one alone can
--- draw it without destroying something.
---
--- A float paints *every* screen cell it covers, transparent ones included, so
--- a sprite-sized float blanks a sprite-sized rectangle of your code. Measured:
--- a buffer cell reading `E` reads ` ` once a float is over it. What a float
--- can do is draw anywhere on the screen.
---
--- Overlay virtual text touches only the cells it is given, so the code either
--- side of a sprite pixel survives. What it cannot do is draw where there is no
--- buffer line — below the end of the file, which is exactly where a pet
--- usually walks.
---
--- So: rows of the sprite that sit over real buffer text are drawn as overlay
--- extmarks, and the rest fall back to a float whose `Normal` has no background
--- of its own. Together they give a sprite with nothing painted behind it.
---
--- The cost that mattered was never the number of windows, it was calling into
--- the Neovim API on every tick for every entity: `nvim_win_set_config` forces a
--- redraw whether or not anything moved. Everything here is therefore guarded
--- by "did this actually change", so an idle pet costs no API calls at all.

local M = {}
local api = vim.api
local sprites = require("distract.terminal_sprites")
local placement = require("distract.placement")
local screen_map = require("distract.screen_map")
local viewport = require("distract.viewport")

--- This renderer is the half-block backend, whose smallest addressable unit is
--- half a character cell, so it always asks for the cell-grid art rather than a
--- manifest's native-resolution sidecar. Hoisted rather than built per call:
--- this runs once per entity per tick.
local renderer_float = require("distract.renderer_float")
local renderer_overlay = require("distract.renderer_overlay")

local active_windows = {}

M.overlay_namespace = renderer_overlay.overlay_namespace
M.background_group = renderer_float.background_group
M.refresh_highlights = renderer_float.refresh_highlights

--- One entity's current picture, in whatever form its backend produced it.
---
--- The two surfaces are the same `width` x `height` rectangle of cells seen two
--- ways: `buf` is a buffer holding the whole frame for the float to show, and
--- `runs()` describes it row by row for the overlay extmarks. `runs` is a
--- function because a sprite entirely below the last buffer line never needs
--- them.
---
--- `key` changes exactly when the picture does and nothing else. It is what
--- keeps a stationary entity from costing any API calls, so a backend whose key
--- moves every tick has given up that property for every entity it draws.
---@class DistractFrameSurface
---@field key integer|string
---@field buf integer
---@field width integer
---@field height integer
---@field runs fun(): table<integer, table>|nil

--- In-terminal backends this module can actually draw. Registered rather than
--- listed, because a backend can need a capable terminal and has to be able to
--- decline. Anything absent must not be offered to users: an unknown name used
--- to fall through to a catch-all draw path, so a backend could be advertised
--- without existing.
---@type table<string, fun(entity: table): DistractFrameSurface|nil>
local BACKEND_SURFACE = {}

--- What each backend has to throw away when its cached art stops being valid.
---@type table<string, fun()>
local BACKEND_RESET = {}

--- Registers the surface provider for an in-terminal backend.
---
--- The provider supplies content only. Placement, the overlay/float split and
--- the redraw guard stay here, so every in-terminal backend inherits them
--- rather than reimplementing them and drifting.
---@param name string canonical backend name
---@param build_surface fun(entity: table): DistractFrameSurface|nil
---@param on_reset fun() drops whatever the backend cached for its frames
function M.register_backend(name, build_surface, on_reset)
  if type(name) ~= "string" or name == "" then
    error("distract.renderer.register_backend: name must be a non-empty string")
  end
  if type(build_surface) ~= "function" or type(on_reset) ~= "function" then
    error("distract.renderer.register_backend: build_surface and on_reset must be functions")
  end
  BACKEND_SURFACE[name] = build_surface
  BACKEND_RESET[name] = on_reset
end

--- Removes a backend this module was drawing for.
---
--- The registry is process-wide, so a backend that registers on proof it can
--- draw -- and can lose that proof, in a test or on `reset` -- has to be able to
--- take itself back out. A name `distract.backends` no longer offers while this
--- module still answers `supports` for it is exactly the on-paper-only backend
--- the two registries are kept in step to prevent.
---@param name string
function M.unregister_backend(name)
  BACKEND_SURFACE[name] = nil
  BACKEND_RESET[name] = nil
end

--- Drops every backend's cached art.
---
--- `:colorscheme` runs `:hi clear`, which deletes the highlight groups the
--- half-block glyphs are painted with and the groups that name kitty's images.
--- Anything built against them has to go with them, whichever backend built it.
function M.reset_backends()
  for _, on_reset in pairs(BACKEND_RESET) do
    on_reset()
  end
end

--- Drops the cached screen map. For tests, and for anything that changes the
--- layout without changing the fingerprint.
function M.invalidate_screen_map()
  screen_map.invalidate()
end

--- Whether this module implements an in-terminal backend by name.
function M.supports(backend)
  return BACKEND_SURFACE[backend] ~= nil
end

--- Whether this entity's current state wraps at the edge.
---
--- Only a wrapping entity is sliced: every other boundary mode keeps the sprite
--- inside the bounds, so there is never a departing half to draw.
local function wraps_at_the_edge(entity)
  local states = entity.manifest and entity.manifest.states
  local state_def = states and states[entity.current_state]
  local physics = state_def and state_def.physics
  -- `wrap` is the default when a state declares no mode, matching `engine.lua`.
  return physics == nil or (physics.wrap_mode or "wrap") == "wrap"
end

--- The floats this renderer owns, so they are not mistaken for someone else's.
local function own_windows()
  local ignored = {}
  for _, entry in pairs(active_windows) do
    for _, slice in ipairs(entry.slices or {}) do
      if slice.win then
        ignored[slice.win] = true
      end
    end
  end
  return ignored
end

local renderer_surface = require("distract.renderer_surface")

M.is_occluding = renderer_surface.is_occluding
M.resolve_pixel_frame = renderer_surface.resolve_pixel_frame
M.resolve_flip = renderer_surface.resolve_flip

--- Draws all entities in terminal mode using the selected backend
function M.draw(entities, backend)
  local build_surface = BACKEND_SURFACE[backend or "halfblock"]
  if not build_surface then
    error(string.format("distract: no renderer for backend '%s'", tostring(backend)))
  end

  local bounds = viewport.bounds()
  screen_map.sync()
  local blocked = viewport.blocking_rects(own_windows())

  local live_ids = {}
  for _, entity in ipairs(entities) do
    live_ids[entity.id] = true
    local surface = build_surface(entity)
    if surface and not M.is_occluding(entity, surface, bounds, blocked) then
      M.place_surface(entity, surface, bounds)
    else
      M.close_window(entity.id)
    end
  end

  for id, _ in pairs(active_windows) do
    if not live_ids[id] then
      M.close_window(id)
    end
  end
end

--- Places one entity's surface, splitting it between buffer text and a float.
---
--- Every in-terminal backend goes through here. The split, the clamping and the
--- redraw guard are the parts that were expensive to get right and are not
--- worth having twice.
---@param entity table
---@param surface DistractFrameSurface
---@param bounds { columns: integer, lines: integer }
function M.place_surface(entity, surface, bounds)
  -- Rows over buffer text are drawn onto it; the tail that is not goes to a
  -- float. Clamping, the wrap slicing and that split are geometry, and live in
  -- `placement.lua`.
  local geom = placement.resolve({
    x = entity.x,
    y = entity.y,
    width = surface.width,
    height = surface.height,
    bounds = bounds,
    wrap = renderer_surface.wraps_at_the_edge(entity),
  })

  if #geom.slices == 0 then
    M.close_window(entity.id)
    return
  end

  -- Nothing below is worth redoing unless the picture, the placement, or the
  -- editor layout under it has changed. An idle pet costs no API calls.
  local sig =
    table.concat({ surface.key, renderer_overlay.slice_signature(geom), screen_map.version() }, ":")

  local entry = active_windows[entity.id]
  if entry and entry.sig == sig and renderer_float.windows_are_valid(entry) then
    return
  end

  local previous = entry
  if previous then
    renderer_overlay.clear_overlay(previous)
  end

  local marks = nil
  if geom.overlay_limit > 0 then
    marks = renderer_overlay.draw_overlay_rows(
      surface.runs() or {},
      geom.row,
      geom.col,
      geom.overlay_limit
    )
  end

  -- The primary slice's float covers whatever the buffer overlay could not, and
  -- every other slice is a whole float of its own.
  local float_slices = {}
  if geom.float_height > 0 then
    table.insert(float_slices, {
      row = geom.float_row,
      col = geom.col,
      width = geom.width,
      height = geom.float_height,
      src_row = geom.overlay_limit,
      src_col = geom.slices[1].src_col,
    })
  end
  for index = 2, #geom.slices do
    table.insert(float_slices, geom.slices[index])
  end

  local previous_wins = (previous and previous.slices) or {}
  local placed = {}
  for index, slice in ipairs(float_slices) do
    local win = renderer_float.place_float(entity, previous_wins[index], surface.buf, slice)
    placed[index] = vim.tbl_extend("force", slice, { win = win, buf = surface.buf })
  end

  -- A sprite that has stopped wrapping needs fewer floats than it had.
  for index = #float_slices + 1, #previous_wins do
    local stale = previous_wins[index].win
    if stale and api.nvim_win_is_valid(stale) then
      api.nvim_win_close(stale, true)
    end
  end

  active_windows[entity.id] = {
    asset_name = entity.asset_name,
    buf = surface.buf,
    win = placed[1] and placed[1].win or nil,
    slices = placed,
    row = geom.row,
    col = geom.col,
    width = geom.width,
    height = geom.height,
    float_row = geom.float_row,
    float_height = geom.float_height,
    overlay_limit = geom.overlay_limit,
    sig = sig,
    marks = marks,
  }
end

M.register_backend("halfblock", renderer_surface.halfblock_surface, function()
  sprites.reset_cache()
end)

function M.close_window(entity_id)
  local w = active_windows[entity_id]
  if w then
    renderer_overlay.clear_overlay(w)
    for _, slice in ipairs(w.slices or {}) do
      if slice.win and api.nvim_win_is_valid(slice.win) then
        api.nvim_win_close(slice.win, true)
      end
    end
    -- The buffer is not deleted: frame buffers are shared between every entity
    -- showing that frame and outlive any one window. `terminal_sprites.reset_cache`
    -- owns their lifetime.
    active_windows[entity_id] = nil
  end
end

function M.clear_all()
  for id, _ in pairs(active_windows) do
    M.close_window(id)
  end
  active_windows = {}
end

--- Placement currently held for an entity, for tests and diagnostics.
--- Where every drawn entity currently sits, in terminal cells.
---
--- The one geometry a plugin drawing its own layer -- a speech bubble, a
--- highlight -- needs, reported in cells on every backend so a plugin does not
--- have to know which renderer produced it.
---@return table[] `{ id, asset_name, row, col, width, height }`
function M.placements()
  local layers = {}
  for id, entry in pairs(active_windows) do
    table.insert(layers, {
      id = id,
      asset_name = entry.asset_name,
      row = entry.row,
      col = entry.col,
      width = entry.width,
      height = entry.height,
    })
  end
  -- `pairs` has no defined order, and a plugin that draws per layer would place
  -- its own art in a different order on every frame.
  table.sort(layers, function(left, right)
    return left.id < right.id
  end)
  return layers
end

function M.window_state(entity_id)
  local w = active_windows[entity_id]
  if not w then
    return nil
  end
  local slices = {}
  for index, slice in ipairs(w.slices or {}) do
    slices[index] = {
      row = slice.row,
      col = slice.col,
      width = slice.width,
      height = slice.height,
      src_row = slice.src_row,
      src_col = slice.src_col,
    }
  end

  return {
    row = w.row,
    col = w.col,
    width = w.width,
    height = w.height,
    buf = w.buf,
    win = w.win,
    slices = slices,
    float_row = w.float_row,
    float_height = w.float_height,
    overlay_limit = w.overlay_limit,
    overlay_marks = w.marks and #w.marks or 0,
  }
end

return M
