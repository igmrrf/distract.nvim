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

-- =========================================================================
-- Screen row -> buffer line
-- =========================================================================

--- Where each screen row's buffer line lives, so a sprite row can be drawn
--- straight onto it.
---
--- Rebuilt only when the layout actually changes. Building it costs a
--- `screenpos` per visible line, which is not something to pay 30 times a
--- second for a screen that has not scrolled.
local screen_map = {}
local screen_map_sig = nil
local screen_map_version = 0

--- A cheap fingerprint of everything that could move a buffer line to a
--- different screen row.
local function layout_signature(wins)
  local parts = {}
  for _, wi in ipairs(wins) do
    parts[#parts + 1] = table.concat({
      wi.winid,
      wi.bufnr,
      wi.winrow,
      wi.wincol,
      wi.width,
      wi.height,
      wi.topline,
      wi.botline,
      wi.textoff,
    }, ":")
  end
  return table.concat(parts, "|")
end

--- Whether a window is one we may draw into: a normal, non-floating window
--- showing an ordinary buffer.
local function is_drawable_window(wi)
  if wi.terminal == 1 then
    return false
  end
  local ok, cfg = pcall(api.nvim_win_get_config, wi.winid)
  if not ok then
    return false
  end
  -- A float of our own, or someone else's popup, is not a drawing surface.
  return cfg.relative == nil or cfg.relative == ""
end

local function rebuild_screen_map(wins)
  local map = {}

  for _, wi in ipairs(wins) do
    if is_drawable_window(wi) then
      -- `wincol`/`winrow` are 1-based screen coordinates of the window's
      -- top-left cell; `textoff` is the width of the gutter (number column,
      -- signs, folds) that virtual text is positioned relative to.
      local text_left = wi.wincol - 1 + wi.textoff
      local text_right = wi.wincol - 1 + wi.width - 1

      -- Only the row a line *starts* on is mapped. A wrapped line occupies
      -- several screen rows and only the first can be addressed by line number,
      -- so the continuation rows are left out and fall through to the float --
      -- correct rather than merely safe, since an extmark placed for them would
      -- land back on the row the line started on. Folded lines report row 0 and
      -- are skipped for the same reason.
      for lnum = wi.topline, wi.botline do
        local pos = vim.fn.screenpos(wi.winid, lnum, 1)
        if pos.row > 0 then
          map[pos.row - 1] = {
            buf = wi.bufnr,
            lnum = lnum,
            text_left = text_left,
            text_right = text_right,
          }
        end
      end
    end
  end

  return map
end

--- Refreshes the screen map if the layout moved. Returns its version, which
--- changes whenever the map does.
local function sync_screen_map()
  local wins = vim.fn.getwininfo()
  local sig = layout_signature(wins)
  if sig ~= screen_map_sig then
    screen_map_sig = sig
    screen_map = rebuild_screen_map(wins)
    screen_map_version = screen_map_version + 1
  end
  return screen_map_version
end

--- Drops the cached screen map. For tests, and for anything that changes the
--- layout without changing the fingerprint.
function M.invalidate_screen_map()
  screen_map_sig = nil
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

--- Registers the surface provider for an in-terminal backend.
---
--- The provider supplies content only. Placement, the overlay/float split and
--- the redraw guard stay here, so every in-terminal backend inherits them
--- rather than reimplementing them and drifting.
---@param name string canonical backend name
---@param build_surface fun(entity: table): DistractFrameSurface|nil
function M.register_backend(name, build_surface)
  if type(name) ~= "string" or name == "" then
    error("distract.renderer.register_backend: name must be a non-empty string")
  end
  if type(build_surface) ~= "function" then
    error("distract.renderer.register_backend: build_surface must be a function")
  end
  BACKEND_SURFACE[name] = build_surface
end

--- Whether this module implements an in-terminal backend by name.
function M.supports(backend)
  return BACKEND_SURFACE[backend] ~= nil
end

--- Draws all entities in terminal mode using the selected backend
function M.draw(entities, backend)
  local build_surface = BACKEND_SURFACE[backend or "halfblock"]
  if not build_surface then
    error(string.format("distract: no renderer for backend '%s'", tostring(backend)))
  end

  local bounds = { columns = vim.o.columns, lines = vim.o.lines }

  -- Where the editor's text is, so sprite rows can be drawn onto it instead of
  -- over it. Rebuilt only when the layout moved.
  sync_screen_map()

  local live_ids = {}
  for _, entity in ipairs(entities) do
    live_ids[entity.id] = true
    local surface = build_surface(entity)
    if surface then
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

--- The first sprite row that cannot be drawn onto buffer text.
---
--- Everything from there down goes to the float. It is a tail rather than a set
--- of individual rows because the rows that fail are almost always the ones
--- below the end of the file, which are contiguous and at the bottom; treating
--- an isolated failure in the middle as the start of the tail costs a few rows
--- of occluded text, which is what every row cost before.
local function first_unmappable_row(row, col, width, height)
  for r = 0, height - 1 do
    local slot = screen_map[row + r]
    if not slot or col < slot.text_left or col + width - 1 > slot.text_right then
      return r
    end
  end
  return height
end

--- Draws sprite rows 0..`limit`-1 as overlay virtual text on the buffer lines
--- underneath them. Returns the extmarks placed.
local function draw_overlay_rows(rows, row, col, limit)
  local marks = {}
  for r = 0, limit - 1 do
    local slot = screen_map[row + r]
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

--- Places the float that covers sprite rows `from`..`height`-1.
---
--- The float shows the whole frame buffer, scrolled so `from` is its top line.
--- That is what lets every entity keep sharing one buffer per frame instead of
--- needing one per (frame, split point).
local function place_float(entity, entry, surface_buf, geom)
  local win = entry and entry.win
  if not win or not api.nvim_win_is_valid(win) then
    win = api.nvim_open_win(surface_buf, false, {
      relative = "editor",
      width = geom.width,
      height = geom.float_height,
      row = geom.float_row,
      col = geom.col,
      style = "minimal",
      border = "none",
      focusable = false,
      noautocmd = true,
      zindex = (entity.z_index or 0) + 100,
    })
    api.nvim_set_option_value("winblend", 0, { win = win })
    api.nvim_set_option_value(
      "winhighlight",
      "Normal:" .. M.background_group() .. ",NormalNC:" .. M.background_group(),
      { win = win }
    )
  elseif
    entry.float_row ~= geom.float_row
    or entry.col ~= geom.col
    or entry.width ~= geom.width
    or entry.float_height ~= geom.float_height
  then
    -- Only reposition when something actually moved. This call forces a full
    -- redraw, so making it unconditionally cost a redraw per entity per tick.
    api.nvim_win_set_config(win, {
      relative = "editor",
      row = geom.float_row,
      col = geom.col,
      width = geom.width,
      height = geom.float_height,
    })
  end

  -- Show a different picture by showing a different buffer.
  if not entry or entry.buf ~= surface_buf then
    api.nvim_win_set_buf(win, surface_buf)
  end

  -- Scroll to the first row the float is responsible for.
  if not entry or entry.buf ~= surface_buf or entry.overlay_limit ~= geom.overlay_limit then
    pcall(api.nvim_win_set_cursor, win, { geom.overlay_limit + 1, 0 })
    pcall(api.nvim_win_call, win, function()
      vim.fn.winrestview({ topline = geom.overlay_limit + 1, lnum = geom.overlay_limit + 1 })
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
  local max_columns = bounds.columns
  local max_lines = bounds.lines

  -- A sprite larger than the viewport still has to produce a legal window size.
  local width = math.min(surface.width, math.max(1, max_columns))
  local height = math.min(surface.height, math.max(1, max_lines - 1))

  local x = math.floor(entity.x)
  local y = math.floor(entity.y)
  local col = math.max(0, math.min(x, max_columns - width))
  local row = math.max(0, math.min(y, max_lines - height - 1))

  -- Rows over buffer text are drawn onto it; the tail that is not goes to the
  -- float.
  local overlay_limit = first_unmappable_row(row, col, width, height)
  local geom = {
    row = row,
    col = col,
    width = width,
    height = height,
    overlay_limit = overlay_limit,
    float_row = row + overlay_limit,
    float_height = height - overlay_limit,
  }

  -- Nothing below is worth redoing unless the picture, the placement, or the
  -- editor layout under it has changed. An idle pet costs no API calls.
  local sig = table.concat({
    surface.key,
    row,
    col,
    width,
    height,
    overlay_limit,
    screen_map_version,
  }, ":")

  local entry = active_windows[entity.id]
  if entry and entry.sig == sig and (not entry.win or api.nvim_win_is_valid(entry.win)) then
    return
  end

  local previous = entry
  if previous then
    clear_overlay(previous)
  end

  local marks = nil
  if overlay_limit > 0 then
    local rows = surface.runs()
    marks = draw_overlay_rows(rows or {}, row, col, overlay_limit)
  end

  local win = nil
  if geom.float_height > 0 then
    win = place_float(entity, previous, surface.buf, geom)
  elseif previous and previous.win and api.nvim_win_is_valid(previous.win) then
    -- Every row landed on buffer text, so there is nothing left for a float to
    -- do and nothing of the editor stays covered.
    api.nvim_win_close(previous.win, true)
  end

  active_windows[entity.id] = {
    buf = surface.buf,
    win = win,
    row = row,
    col = col,
    width = width,
    height = height,
    float_row = geom.float_row,
    float_height = geom.float_height,
    overlay_limit = overlay_limit,
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
  local frame_count = #sprites.get_pixel_frames(entity.asset_name)
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

M.register_backend("halfblock", halfblock_surface)

function M.close_window(entity_id)
  local w = active_windows[entity_id]
  if w then
    clear_overlay(w)
    if w.win and api.nvim_win_is_valid(w.win) then
      api.nvim_win_close(w.win, true)
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
function M.window_state(entity_id)
  local w = active_windows[entity_id]
  if not w then
    return nil
  end
  return {
    row = w.row,
    col = w.col,
    width = w.width,
    height = w.height,
    buf = w.buf,
    win = w.win,
    float_row = w.float_row,
    float_height = w.float_height,
    overlay_limit = w.overlay_limit,
    overlay_marks = w.marks and #w.marks or 0,
  }
end

return M
