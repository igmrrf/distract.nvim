--- In-terminal renderer.
---
--- Each entity gets its own small floating window. That is deliberate rather
--- than incidental: a Neovim float always paints the screen cells it covers, so
--- a single full-screen float would blank the editor behind it. Keeping the
--- windows sprite-sized is what lets the editor show through everywhere the
--- sprite is not.
---
--- The cost that mattered was never the number of windows, it was calling into
--- the Neovim API on every tick for every entity: `nvim_win_set_config` forces a
--- redraw whether or not anything moved. Everything here is therefore guarded
--- by "did this actually change", so an idle pet costs no API calls at all.

local M = {}
local api = vim.api
local sprites = require("distract.terminal_sprites")

local ns_id = api.nvim_create_namespace("distract_sprites")

--- entity_id -> { buf, win, frame_idx, row, col, width, height }
local active_windows = {}

--- In-terminal backends this module can actually draw. Anything not listed here
--- must not be offered to users: an unknown name used to fall through to a
--- catch-all draw path, so a backend could be advertised without existing.
local BACKEND_DRAW = {
  halfblock = function(...)
    return M.draw_halfblock_entity(...)
  end,
}

--- Whether this module implements an in-terminal backend by name.
function M.supports(backend)
  return BACKEND_DRAW[backend] ~= nil
end

--- Draws all entities in terminal mode using the selected backend
function M.draw(entities, backend)
  local draw_entity = BACKEND_DRAW[backend or "halfblock"]
  if not draw_entity then
    error(string.format("distract: no renderer for backend '%s'", tostring(backend)))
  end

  local max_columns = vim.o.columns
  local max_lines = vim.o.lines

  local live_ids = {}
  for _, entity in ipairs(entities) do
    live_ids[entity.id] = true
    draw_entity(entity, max_columns, max_lines)
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

function M.draw_halfblock_entity(entity, max_columns, max_lines)
  local frame_count = #sprites.get_pixel_frames(entity.asset_name)
  local frame_idx = M.resolve_pixel_frame(entity, frame_count)

  -- Cached: the strings and highlight spans for a frame never change.
  local lines, highlights, sprite_w, sprite_h =
    sprites.get_rendered_frame(entity.asset_name, frame_idx)

  if sprite_w < 1 or sprite_h < 1 then
    -- Nothing renderable for this frame; drop any window we still hold rather
    -- than asking nvim_open_win for a zero-sized window.
    M.close_window(entity.id)
    return
  end

  -- A sprite larger than the viewport still has to produce a legal window size.
  local width = math.min(sprite_w, math.max(1, max_columns))
  local height = math.min(sprite_h, math.max(1, max_lines - 1))

  local x = math.floor(entity.x)
  local y = math.floor(entity.y)
  local col = math.max(0, math.min(x, max_columns - width))
  local row = math.max(0, math.min(y, max_lines - height - 1))

  local entry = active_windows[entity.id]
  if not entry or not api.nvim_win_is_valid(entry.win) or not api.nvim_buf_is_valid(entry.buf) then
    local buf = api.nvim_create_buf(false, true)
    local win = api.nvim_open_win(buf, false, {
      relative = "editor",
      width = width,
      height = height,
      row = row,
      col = col,
      style = "minimal",
      border = "none",
      focusable = false,
      noautocmd = true,
      zindex = (entity.z_index or 0) + 100,
    })

    api.nvim_set_option_value("winblend", 0, { win = win })
    api.nvim_set_option_value(
      "winhighlight",
      "Normal:NormalFloat,NormalNC:NormalFloat",
      { win = win }
    )

    entry =
      { buf = buf, win = win, frame_idx = -1, row = row, col = col, width = width, height = height }
    active_windows[entity.id] = entry
  elseif entry.row ~= row or entry.col ~= col or entry.width ~= width or entry.height ~= height then
    -- Only reposition when something actually moved. This call forces a full
    -- redraw, so making it unconditionally cost a redraw per entity per tick.
    entry.row, entry.col, entry.width, entry.height = row, col, width, height
    api.nvim_win_set_config(entry.win, {
      relative = "editor",
      row = row,
      col = col,
      width = width,
      height = height,
    })
  end

  -- Redraw frame content only when the frame changes.
  if entry.frame_idx ~= frame_idx then
    entry.frame_idx = frame_idx
    api.nvim_buf_set_lines(entry.buf, 0, -1, false, lines)
    api.nvim_buf_clear_namespace(entry.buf, ns_id, 0, -1)

    for _, hl in ipairs(highlights) do
      -- hl.col and hl.len are byte offsets: a half-block glyph is 3 bytes, so
      -- an end_col of col + 1 would split the codepoint and mis-colour the row.
      api.nvim_buf_set_extmark(entry.buf, ns_id, hl.row, hl.col, {
        end_row = hl.row,
        end_col = hl.col + hl.len,
        hl_group = hl.hl,
        priority = 100,
      })
    end
  end
end

function M.close_window(entity_id)
  local w = active_windows[entity_id]
  if w then
    if api.nvim_win_is_valid(w.win) then
      api.nvim_win_close(w.win, true)
    end
    if api.nvim_buf_is_valid(w.buf) then
      api.nvim_buf_delete(w.buf, { force = true })
    end
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
  return { row = w.row, col = w.col, width = w.width, height = w.height, frame_idx = w.frame_idx }
end

return M
