local M = {}

-- Cache for dynamically generated Neovim highlight groups
local hl_cache = {}

--- Ensure a Neovim highlight group exists for foreground/background RGB colors
function M.get_hl_group(fg_rgb, bg_rgb)
  local key = string.format(
    "Distract_%s_%s",
    fg_rgb and string.format("%02x%02x%02x", fg_rgb[1], fg_rgb[2], fg_rgb[3]) or "none",
    bg_rgb and string.format("%02x%02x%02x", bg_rgb[1], bg_rgb[2], bg_rgb[3]) or "none"
  )

  if not hl_cache[key] then
    local hl_opts = {}
    if fg_rgb then
      hl_opts.fg = string.format("#%02x%02x%02x", fg_rgb[1], fg_rgb[2], fg_rgb[3])
    end
    if bg_rgb then
      hl_opts.bg = string.format("#%02x%02x%02x", bg_rgb[1], bg_rgb[2], bg_rgb[3])
    end
    vim.api.nvim_set_hl(0, key, hl_opts)
    hl_cache[key] = key
  end

  return key
end

-- =========================================================================
-- Generated sprite registry
-- =========================================================================

-- Sprites are drawn procedurally by `distract.sprites.*` rather than stored as
-- hand-authored pixel tables. Each module returns its frames plus a `layout`
-- mapping state name -> 0-based frame indices, which the matching manifest
-- references directly so indices cannot drift out of sync with the art.
local SPRITE_MODULES = {
  cat = "distract.sprites.cat",
  crab = "distract.sprites.crab",
  sun = "distract.sprites.sun",
}

-- Generation is not free, so each asset is drawn once on first use and cached.
-- Requiring a sprite module only builds its pose curves and layout; the
-- rasterisation happens the first time frames are actually asked for. Every
-- manifest requires this module for its layout, so eager drawing cost ~10ms of
-- every Neovim startup whether or not anything was ever spawned.
local sprite_cache = {}

local function load_sprite(asset_name)
  local cached = sprite_cache[asset_name]
  if cached then
    return cached
  end

  local module_path = SPRITE_MODULES[asset_name] or SPRITE_MODULES.cat
  local sprite = require(module_path)
  sprite_cache[asset_name] = sprite
  return sprite
end

--- Frame matrices for an asset. Unknown assets fall back to the cat.
--- Draws the asset on first call.
function M.get_pixel_frames(asset_name)
  local sprite = load_sprite(asset_name)
  if type(sprite.frames) == "function" then
    return sprite.frames()
  end
  return sprite.frames
end

--- State name -> 0-based frame indices, for a manifest to reference.
function M.get_layout(asset_name)
  return load_sprite(asset_name).layout
end

--- Canvas dimensions in pixels (not terminal cells).
function M.get_dimensions(asset_name)
  local sprite = load_sprite(asset_name)
  return sprite.width, sprite.height
end

-- Both half-block glyphs are 3-byte UTF-8 sequences. Extmark columns are byte
-- offsets, not character indices, so every cell advances the cursor by that
-- much rather than by one.
local UPPER_HALF = "\u{2580}"
local LOWER_HALF = "\u{2584}"

--- Widest row in the matrix. Rows are expected to be uniform, but a custom
--- matrix may be ragged; padding to the maximum keeps every rendered line
--- rectangular so the float window width stays correct.
local function matrix_width(pixel_rows)
  local width = 0
  for _, row in ipairs(pixel_rows) do
    if #row > width then
      width = #row
    end
  end
  return width
end

--- Converts a pixel matrix into half-block strings plus extmark highlight spans.
---
--- Returns `lines, highlights, width, height` where `width`/`height` are in
--- terminal cells (suitable for `nvim_open_win`) and each highlight carries a
--- byte offset `col` and byte length `len` (suitable for `nvim_buf_set_extmark`).
function M.render_halfblock_frame(pixel_rows)
  local lines = {}
  local highlights = {}
  local width = matrix_width(pixel_rows)

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
        hl = M.get_hl_group(top_color, bot_color)
      elseif top_color then
        glyph = UPPER_HALF
        hl = M.get_hl_group(top_color, nil)
      elseif bot_color then
        glyph = LOWER_HALF
        hl = M.get_hl_group(bot_color, nil)
      else
        -- Transparent cell: a space keeps the line rectangular and lets the
        -- editor behind the float show through.
        glyph = " "
      end

      if hl then
        table.insert(highlights, { row = row_idx, col = byte_col, len = #glyph, hl = hl })
      end
      table.insert(line_chars, glyph)
      byte_col = byte_col + #glyph
    end

    table.insert(lines, table.concat(line_chars))
  end

  return lines, highlights, width, #lines
end

-- Rendering a frame depends only on `(asset, frame index)`, and the result is
-- immutable, so it is built once instead of on every draw. At 30 FPS per entity
-- that is the difference between rebuilding every sprite string ~30 times a
-- second and never rebuilding it again.
local render_cache = {}

--- Cached `render_halfblock_frame` for one frame of an asset.
--- Returns `lines, highlights, width, height`.
function M.get_rendered_frame(asset_name, frame_idx)
  local by_asset = render_cache[asset_name]
  if not by_asset then
    by_asset = {}
    render_cache[asset_name] = by_asset
  end

  local entry = by_asset[frame_idx]
  if not entry then
    local frames = M.get_pixel_frames(asset_name)
    local matrix = frames[frame_idx] or frames[1]
    if not matrix then
      return {}, {}, 0, 0
    end
    local lines, highlights, w, h = M.render_halfblock_frame(matrix)
    entry = { lines = lines, highlights = highlights, width = w, height = h }
    by_asset[frame_idx] = entry
  end

  return entry.lines, entry.highlights, entry.width, entry.height
end

--- Drops the render cache. Only needed by tests and by a colourscheme reload,
--- since highlight groups are recreated lazily.
function M.reset_cache()
  render_cache = {}
end

return M
