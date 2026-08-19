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
---@type table
local HALFBLOCK_CAPABILITY = { native_resolution = false }

--- entity_id -> { buf, win, row, col, width, height, sig, marks }
local active_windows = {}

local overlay_ns = api.nvim_create_namespace("distract_sprite_overlay")

--- Namespace the buffer-overlay extmarks live in.
function M.overlay_namespace()
  return overlay_ns
end

--- Highlight group a sprite window uses for `Normal`.
---
--- `NormalFloat` is wrong for a sprite: most colourschemes give it a background
--- of its own, so every sprite dragged a visible rectangle of that colour
--- around the screen. `bg = "NONE"` lets the terminal's own background through,
--- which is as close to transparent as a float can get.
local BACKGROUND_GROUP = "DistractSpriteNormal"
local background_defined = false

function M.background_group()
  if not background_defined then
    api.nvim_set_hl(0, BACKGROUND_GROUP, { bg = "NONE", fg = "NONE" })
    background_defined = true
  end
  return BACKGROUND_GROUP
end

--- Re-declares the sprite background, after a colourscheme has cleared it.
function M.refresh_highlights()
  background_defined = false
  M.background_group()
end

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

--- Whether drawing this entity would cover something the user is working in.
---
--- A sprite over an LSP hover, a completion menu or a terminal split is worse
--- than no sprite, so the frame is skipped for that entity rather than drawn
--- underneath where it would still repaint over the text.
---@return boolean
function M.is_occluding(entity, surface, bounds, blocked)
  if #blocked == 0 then
    return false
  end
  local geom = placement.resolve({
    x = entity.x,
    y = entity.y,
    width = surface.width,
    height = surface.height,
    bounds = bounds,
    wrap = wraps_at_the_edge(entity),
  })
  for _, slice in ipairs(geom.slices) do
    for _, rect in ipairs(blocked) do
      if viewport.overlaps(slice, rect) then
        return true
      end
    end
  end
  return false
end

--- Draws all entities in terminal mode using the selected backend
function M.draw(entities, backend)
  local build_surface = BACKEND_SURFACE[backend or "halfblock"]
  if not build_surface then
    error(string.format("distract: no renderer for backend '%s'", tostring(backend)))
  end

  local bounds = viewport.bounds()

  -- Where the editor's text is, so sprite rows can be drawn onto it instead of
  -- over it. Rebuilt only when the layout moved.
  screen_map.sync()

  -- What a sprite must not cover, measured once for the frame. The sprites' own
  -- floats are excluded, or every sprite would block itself.
  local blocked = viewport.blocking_rects(own_windows())

  local live_ids = {}
  for _, entity in ipairs(entities) do
    live_ids[entity.id] = true
    local surface = build_surface(entity)
    if surface and not M.is_occluding(entity, surface, bounds, blocked) then
      M.place_surface(entity, surface, bounds)
    else
      -- Nothing renderable for this frame; drop anything we still hold rather
      -- than asking nvim_open_win for a zero-sized window.
      M.close_window(entity.id)
    end
  end

  -- Clean up windows for despawned entities
  for id, _ in pairs(active_windows) do
    if not live_ids[id] then
      M.close_window(id)
    end
  end
end

--- Resolves which pixel frame to draw for an entity.
---
--- `entity.frame_idx` is the entity's position within the current state's
--- animation, not a sheet index. The manifest's `animation.frames` list maps
--- that position onto a 0-based sheet frame, so both have to be applied:
--- without the mapping a sleeping cat draws its idle art and the sun's eclipse
--- draws its shining art.
---
--- Returns a 1-based index into the asset's pixel frame table.
function M.resolve_pixel_frame(entity, frame_count)
  if not frame_count or frame_count < 1 then
    return 1
  end

  local manifest = entity.manifest
  local state_def = manifest and manifest.states and manifest.states[entity.current_state]
  local frames = state_def and state_def.animation and state_def.animation.frames

  if not frames or #frames == 0 then
    return 1
  end

  local position = ((math.max(1, entity.frame_idx or 1) - 1) % #frames) + 1
  local sheet_idx = frames[position] or 0

  -- Manifest frame indices are 0-based; pixel frame tables are 1-based.
  return (sheet_idx % frame_count) + 1
end

--- Whether an entity's art should be drawn mirrored.
---
--- `entity.flip_x` is which way the entity is heading; `animation.flip_x` is
--- whether the art for this state was authored facing the other way. They
--- combine, exactly as the overlay's `build_instances` combines them, so a
--- state whose art already faces left is not mirrored twice.
function M.resolve_flip(entity)
  local manifest = entity.manifest
  local state_def = manifest and manifest.states and manifest.states[entity.current_state]
  local anim_flip = state_def and state_def.animation and state_def.animation.flip_x or false
  local entity_flip = entity.flip_x or false
  return entity_flip ~= anim_flip
end

--- Removes an entity's overlay extmarks.
local function slice_signature(geom)
  local parts = { geom.overlay_limit }
  for _, slice in ipairs(geom.slices) do
    table.insert(
      parts,
      table.concat(
        { slice.row, slice.col, slice.width, slice.height, slice.src_row, slice.src_col },
        ","
      )
    )
  end
  return table.concat(parts, "|")
end

local function windows_are_valid(entry)
  for _, slice in ipairs(entry.slices or {}) do
    if slice.win and not api.nvim_win_is_valid(slice.win) then
      return false
    end
  end
  return true
end

local function clear_overlay(entry)
  if not entry or not entry.marks then
    return
  end
  for _, mark in ipairs(entry.marks) do
    if api.nvim_buf_is_valid(mark[1]) then
      pcall(api.nvim_buf_del_extmark, mark[1], overlay_ns, mark[2])
    end
  end
  entry.marks = nil
end

--- Draws sprite rows 0..`limit`-1 as overlay virtual text on the buffer lines
--- underneath them. Returns the extmarks placed.
local function draw_overlay_rows(rows, row, col, limit)
  local marks = {}
  for r = 0, limit - 1 do
    local slot = screen_map.slot(row + r)
    for _, run in ipairs(rows[r] or {}) do
      -- `virt_text_win_col` is measured from the window's first *text* column,
      -- so the gutter has to come out of the screen column. Unlike
      -- `virt_text_pos = "overlay"` it does not need the underlying line to be
      -- long enough to reach that column.
      local ok, id = pcall(api.nvim_buf_set_extmark, slot.buf, overlay_ns, slot.lnum - 1, 0, {
        virt_text = run.chunks,
        virt_text_win_col = col + run.col - slot.text_left,
        hl_mode = "combine",
        priority = 200,
        ephemeral = false,
      })
      if ok then
        marks[#marks + 1] = { slot.buf, id }
      end
    end
  end
  return marks
end

--- Places one float showing one slice of a surface.
---
--- The float shows the whole frame buffer, scrolled so the slice's first row and
--- column are its top-left. That is what lets every entity keep sharing one
--- buffer per frame instead of needing one per (frame, split point) — and it is
--- what makes a wrapped sprite's departing half free: it is the same buffer at a
--- different scroll offset.
---@param slice DistractSlice
local function place_float(entity, previous, surface_buf, slice)
  local win = previous and previous.win
  if not win or not api.nvim_win_is_valid(win) then
    win = api.nvim_open_win(surface_buf, false, {
      relative = "editor",
      width = slice.width,
      height = slice.height,
      row = slice.row,
      col = slice.col,
      style = "minimal",
      border = "none",
      focusable = false,
      noautocmd = true,
      zindex = (entity.z_index or 0) + viewport.z_index_offset(),
    })
    api.nvim_set_option_value("winblend", 0, { win = win })
    api.nvim_set_option_value("wrap", false, { win = win })
    api.nvim_set_option_value(
      "winhighlight",
      "Normal:" .. M.background_group() .. ",NormalNC:" .. M.background_group(),
      { win = win }
    )
  elseif
    previous.row ~= slice.row
    or previous.col ~= slice.col
    or previous.width ~= slice.width
    or previous.height ~= slice.height
  then
    -- Only reposition when something actually moved. This call forces a full
    -- redraw, so making it unconditionally cost a redraw per entity per tick.
    api.nvim_win_set_config(win, {
      relative = "editor",
      row = slice.row,
      col = slice.col,
      width = slice.width,
      height = slice.height,
    })
  end

  -- Show a different picture by showing a different buffer.
  if not previous or previous.buf ~= surface_buf then
    api.nvim_win_set_buf(win, surface_buf)
  end

  -- Scroll to the slice's own corner of the surface.
  if
    not previous
    or previous.buf ~= surface_buf
    or previous.src_row ~= slice.src_row
    or previous.src_col ~= slice.src_col
  then
    pcall(api.nvim_win_set_cursor, win, { slice.src_row + 1, 0 })
    pcall(api.nvim_win_call, win, function()
      vim.fn.winrestview({
        topline = slice.src_row + 1,
        lnum = slice.src_row + 1,
        leftcol = slice.src_col,
      })
    end)
  end

  return win
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
    wrap = wraps_at_the_edge(entity),
  })

  if #geom.slices == 0 then
    M.close_window(entity.id)
    return
  end

  -- Nothing below is worth redoing unless the picture, the placement, or the
  -- editor layout under it has changed. An idle pet costs no API calls.
  local sig = table.concat({ surface.key, slice_signature(geom), screen_map.version() }, ":")

  local entry = active_windows[entity.id]
  if entry and entry.sig == sig and windows_are_valid(entry) then
    return
  end

  local previous = entry
  if previous then
    clear_overlay(previous)
  end

  local marks = nil
  if geom.overlay_limit > 0 then
    marks = draw_overlay_rows(surface.runs() or {}, geom.row, geom.col, geom.overlay_limit)
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
    local win = place_float(entity, previous_wins[index], surface.buf, slice)
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

--- The half-block surface: two sprite pixel rows stacked into one cell.
---
--- The buffer for a frame is built once, with its highlights already in it, and
--- shared by every entity showing that frame. Advancing the animation is then
--- one `nvim_win_set_buf` rather than a rewrite of every coloured cell, which
--- is also why the buffer handle is a sound cache key.
---@param entity table
---@return DistractFrameSurface|nil
local function halfblock_surface(entity)
  local frame_count = #sprites.get_pixel_frames(entity.asset_name, HALFBLOCK_CAPABILITY)
  local frame_idx = M.resolve_pixel_frame(entity, frame_count)
  local flip_x = M.resolve_flip(entity)

  local frame_buf, sprite_w, sprite_h =
    sprites.get_frame_buffer(entity.asset_name, frame_idx, flip_x)

  if not frame_buf or sprite_w < 1 or sprite_h < 1 then
    return nil
  end

  return {
    key = frame_buf,
    buf = frame_buf,
    width = sprite_w,
    height = sprite_h,
    runs = function()
      return sprites.get_frame_runs(entity.asset_name, frame_idx, flip_x)
    end,
  }
end

M.register_backend("halfblock", halfblock_surface, function()
  sprites.reset_cache()
end)

function M.close_window(entity_id)
  local w = active_windows[entity_id]
  if w then
    clear_overlay(w)
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
