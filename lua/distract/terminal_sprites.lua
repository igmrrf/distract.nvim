local M = {}

local frame_buffers = require("distract.frame_buffers")
local frame_source = require("distract.frame_source")
local highlights = require("distract.highlights")
local raster3d = require("distract.raster3d")
local sources = require("distract.sprite_sources")

--- How many colours imported art is reduced to before it is drawn in
--- half-blocks. Procedural art is already drawn from a small palette and is
--- left at full fidelity.
M.DEFAULT_MAX_SPRITE_COLOURS = 128

local max_sprite_colours = M.DEFAULT_MAX_SPRITE_COLOURS

--- A highlight group for one cell's colours.
---
--- `owner` is the asset the colours belong to. Groups are bounded per owner, so
--- an asset that is evicted takes only its own colours with it -- and the frames
--- cached against them are dropped in the same breath.
---@param fg_rgb integer[]|nil
---@param bg_rgb integer[]|nil
---@param owner string|nil
---@return string group_name
function M.get_hl_group(fg_rgb, bg_rgb, owner)
  return highlights.group(fg_rgb, bg_rgb, owner)
end

--- Forgets which highlight groups exist.
---
--- `:colorscheme` runs `:hi clear`, which deletes every group including these.
--- Without this the cache still claims they are defined and every sprite
--- renders in the default foreground until Neovim restarts.
function M.reset_highlights()
  highlights.reset()
end

--- Applies the plugin's configuration to the drawing caches.
---@param opts table `{ max_sprite_colours = n, max_highlight_groups = n }`
function M.configure(opts)
  opts = opts or {}
  if opts.max_sprite_colours ~= nil then
    if type(opts.max_sprite_colours) ~= "number" or opts.max_sprite_colours < 1 then
      error("distract: max_sprite_colours must be a positive number")
    end
    max_sprite_colours = math.floor(opts.max_sprite_colours)
  end
  highlights.configure({ max_groups = opts.max_highlight_groups })
end

--- Which art an asset has, and which pixels a frame is drawn from, are
--- `distract.sprite_sources`' and `distract.frame_source`'s answers; this module
--- re-exports both so a caller asking for a frame and a caller asking what to
--- draw it from talk to one place.
M.pixel_matrix = function(asset_name, frame_idx, flip_x)
  return frame_source.matrix(asset_name, frame_idx, {
    flip_x = flip_x,
    max_colours = max_sprite_colours,
  })
end
M.mirror_matrix = frame_source.mirror_matrix
M.is_voxel = frame_source.is_voxel
M.bind_manifest = frame_source.bind_manifest
M.configure_render = frame_source.configure
M.on_render_change = frame_source.on_change

M.has_sprite = sources.has_sprite
M.unbind_manifest = sources.unbind_manifest
M.register = sources.register
M.get_pixel_frames = sources.get_pixel_frames
M.get_layout = sources.get_layout
M.get_dimensions = sources.get_dimensions
M.frame_delay_ms = sources.frame_delay_ms

-- Both half-block glyphs are 3-byte UTF-8 sequences. Extmark columns are byte
-- offsets, not character indices, so every cell advances the cursor by that
-- much rather than by one.
local UPPER_HALF = "\u{2580}"
local LOWER_HALF = "\u{2584}"

--- Converts a pixel matrix into half-block strings plus extmark highlight spans.
---
--- `owner` is the asset the colours belong to, which is what bounds the
--- highlight groups they create.
---
--- Returns `lines, highlights, width, height` where `width`/`height` are in
--- terminal cells (suitable for `nvim_open_win`) and each highlight carries a
--- byte offset `col` and byte length `len` (suitable for `nvim_buf_set_extmark`).
function M.render_halfblock_frame(pixel_rows, owner)
  local lines = {}
  local spans = {}
  local width = frame_source.matrix_width(pixel_rows)

  for r = 1, #pixel_rows, 2 do
    local top_row = pixel_rows[r] or {}
    local bot_row = pixel_rows[r + 1] or {}
    local line_chars = {}
    local row_idx = #lines -- 0-indexed for Neovim
    local byte_col = 0

    for c = 1, width do
      local top_color = top_row[c]
      local bot_color = bot_row[c]
      local glyph, hl

      if top_color and bot_color then
        glyph = UPPER_HALF
        hl = M.get_hl_group(top_color, bot_color, owner)
      elseif top_color then
        glyph = UPPER_HALF
        hl = M.get_hl_group(top_color, nil, owner)
      elseif bot_color then
        glyph = LOWER_HALF
        hl = M.get_hl_group(bot_color, nil, owner)
      else
        -- Transparent cell: a space keeps the line rectangular and lets the
        -- editor behind the float show through.
        glyph = " "
      end

      if hl then
        table.insert(spans, { row = row_idx, col = byte_col, len = #glyph, hl = hl })
      end
      table.insert(line_chars, glyph)
      byte_col = byte_col + #glyph
    end

    table.insert(lines, table.concat(line_chars))
  end

  return lines, spans, width, #lines
end

-- Rendering a frame depends only on `(asset, frame index, facing)`, and the
-- result is immutable, so it is built once instead of on every draw. At 30 FPS
-- per entity that is the difference between rebuilding every sprite string ~30
-- times a second and never rebuilding it again.
local render_cache = {}

--- Cached `render_halfblock_frame` for one frame of an asset.
---
--- `flip_x` mirrors the art horizontally. It is part of the cache key rather
--- than applied afterwards because mirroring the *rendered* output would mean
--- reversing byte offsets in every highlight span on every draw; mirroring the
--- pixel matrix once and rendering that is both simpler and free after the
--- first call.
---
--- Returns `lines, highlights, width, height`.
function M.get_rendered_frame(asset_name, frame_idx, flip_x)
  local facing = flip_x and "flipped" or "facing"
  local by_asset = render_cache[asset_name]
  if not by_asset then
    by_asset = { facing = {}, flipped = {} }
    render_cache[asset_name] = by_asset
  end
  local by_facing = by_asset[facing]

  local entry = by_facing[frame_idx]
  if not entry then
    local matrix = M.pixel_matrix(asset_name, frame_idx, flip_x)
    if not matrix then
      return {}, {}, 0, 0
    end
    local lines, spans, width, height = M.render_halfblock_frame(matrix, asset_name)
    entry = { lines = lines, highlights = spans, width = width, height = height }
    by_facing[frame_idx] = entry
  end

  return entry.lines, entry.highlights, entry.width, entry.height
end

-- =========================================================================
-- Frame runs, for drawing straight onto a buffer
-- =========================================================================

--- Splits a frame into per-row runs of adjacent drawn cells.
---
--- A float paints every cell it covers, including the transparent ones, so it
--- blanks the editor text underneath the sprite's whole bounding box. Drawing
--- the sprite as overlay virtual text instead touches only the cells that
--- actually have a pixel in them — but that needs the frame described as runs
--- rather than as padded lines, because a run of spaces would occlude exactly
--- what it is supposed to leave alone.
---
--- Returns `rows, width, height`, where `rows` is a list indexed by 0-based
--- sprite row of:
---   `{ { col = <0-based cell offset>, chunks = { { text, hl_group }, ... } } }`
---
--- Adjacent cells sharing a highlight are merged into one chunk, so a row is
--- typically one or two extmarks rather than one per cell.
local function build_runs(lines, highlight_spans)
  local by_row = {}
  for _, hl in ipairs(highlight_spans) do
    local row = by_row[hl.row]
    if not row then
      row = {}
      by_row[hl.row] = row
    end
    row[hl.col] = hl
  end

  local rows = {}
  for row_idx = 0, #lines - 1 do
    local spans = by_row[row_idx] or {}
    local line = lines[row_idx + 1]
    local runs = {}

    local run = nil
    local cell = 0
    local byte = 0
    while byte < #line do
      local hl = spans[byte]
      if hl then
        local text = line:sub(byte + 1, byte + hl.len)
        if not run then
          run = { col = cell, chunks = {} }
        end
        local last = run.chunks[#run.chunks]
        if last and last[2] == hl.hl then
          last[1] = last[1] .. text
        else
          run.chunks[#run.chunks + 1] = { text, hl.hl }
        end
        byte = byte + hl.len
      else
        -- A cell with no highlight is transparent. It ends the current run and
        -- is itself drawn as nothing at all.
        if run then
          runs[#runs + 1] = run
          run = nil
        end
        byte = byte + 1
      end
      cell = cell + 1
    end
    if run then
      runs[#runs + 1] = run
    end

    rows[row_idx] = runs
  end

  return rows
end

local runs_cache = {}

--- Cached per-row runs for one frame. Returns `rows, width, height`.
function M.get_frame_runs(asset_name, frame_idx, flip_x)
  local facing = flip_x and "flipped" or "facing"
  local by_asset = runs_cache[asset_name]
  if not by_asset then
    by_asset = { facing = {}, flipped = {} }
    runs_cache[asset_name] = by_asset
  end
  local by_facing = by_asset[facing]

  local entry = by_facing[frame_idx]
  if not entry then
    local lines, spans, width, height = M.get_rendered_frame(asset_name, frame_idx, flip_x)
    if width < 1 or height < 1 then
      return nil
    end
    entry = { rows = build_runs(lines, spans), width = width, height = height }
    by_facing[frame_idx] = entry
  end

  return entry.rows, entry.width, entry.height
end

--- Namespace the prepared frame buffers carry their highlights in.
function M.frame_namespace()
  return frame_buffers.namespace()
end

--- A buffer holding one frame, ready to be shown in a window.
---
--- Returns `bufnr, width, height`, or `nil` when the frame has no art.
function M.get_frame_buffer(asset_name, frame_idx, flip_x)
  local lines, spans, width, height = M.get_rendered_frame(asset_name, frame_idx, flip_x)
  if width < 1 or height < 1 then
    return nil
  end

  local key = string.format("%d:%s", frame_idx, flip_x and "flipped" or "facing")
  return frame_buffers.acquire(asset_name, key, {
    lines = lines,
    highlights = spans,
    width = width,
    height = height,
  })
end

--- Drops the render and frame-buffer caches, for one asset or for all of them.
--- Needed by tests, by `register`, and by a colourscheme reload, since
--- highlight groups are recreated lazily.
function M.reset_cache(asset_name)
  if asset_name then
    render_cache[asset_name] = nil
    runs_cache[asset_name] = nil
  else
    render_cache = {}
    runs_cache = {}
  end
  frame_buffers.release(asset_name)
end

-- Frames are cached as highlight group *names*. Clearing an asset's groups
-- without dropping what referenced them would leave that asset drawing in the
-- default foreground until something else invalidated it.
highlights.on_evict(function(owner)
  M.reset_cache(owner)
end)

-- Art that changed is art that has to be re-rendered; the frames cached here
-- are keyed by asset name and say nothing about where the pixels came from.
sources.on_change(function(asset_name)
  M.reset_cache(asset_name)
  raster3d.reset(asset_name)
end)

-- So is art whose render mode, camera or light changed. `nil` means every asset.
frame_source.on_change(function(asset_name)
  M.reset_cache(asset_name)
end)

return M
