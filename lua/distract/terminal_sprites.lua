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

--- Forgets which highlight groups exist.
---
--- `:colorscheme` runs `:hi clear`, which deletes every group including these.
--- Without this the cache still claims they are defined and every sprite
--- renders in the default foreground until Neovim restarts.
function M.reset_highlights()
  hl_cache = {}
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

--- Sprite sets registered at runtime by user config or by an asset pack.
---
--- A registered entry is the same shape a built-in sprite module returns:
--- `{ frames = table|function, layout = table, width = n, height = n }`.
local registered = {}

--- Whether an asset name has art this module can actually draw.
function M.has_sprite(asset_name)
  return registered[asset_name] ~= nil
    or SPRITE_MODULES[asset_name] ~= nil
    or sprite_cache[asset_name] ~= nil
end

--- Registers a sprite set for `asset_name`, replacing any previous one.
---
--- This is what makes a custom asset drawable in the terminal. Without it the
--- only art this module can reach is the three built-ins.
function M.register(asset_name, sprite)
  if type(asset_name) ~= "string" or asset_name == "" then
    error("distract: register() needs an asset name")
  end
  if type(sprite) ~= "table" or sprite.frames == nil then
    error(string.format("distract: sprite set for '%s' has no `frames`", asset_name))
  end
  registered[asset_name] = sprite
  sprite_cache[asset_name] = nil
  M.reset_cache(asset_name)
end

-- Reported once per asset, not once per draw: an unknown asset is asked for at
-- 30 FPS, and a notification per tick makes the editor unusable.
local fallback_warned = {}

local function warn_fallback(asset_name)
  if fallback_warned[asset_name] then
    return
  end
  fallback_warned[asset_name] = true
  vim.notify(
    string.format(
      "[Distract] No terminal art for asset '%s'; drawing the cat instead. "
        .. "Register art with require('distract').register_asset('%s', { sprites = ... }) "
        .. "or use the overlay backend with a spritesheet.",
      asset_name,
      asset_name
    ),
    vim.log.levels.WARN
  )
end

local function load_sprite(asset_name)
  local cached = sprite_cache[asset_name]
  if cached then
    return cached
  end

  local sprite = registered[asset_name]

  if not sprite then
    local module_path = SPRITE_MODULES[asset_name]
    if not module_path then
      -- A sprite module on the runtimepath is as good as a built-in, so an
      -- asset pack can ship `lua/distract/sprites/<name>.lua` and work without
      -- calling register().
      local ok, loaded = pcall(require, "distract.sprites." .. asset_name)
      if ok and type(loaded) == "table" and loaded.frames ~= nil then
        sprite = loaded
      end
    else
      sprite = require(module_path)
    end
  end

  -- Falling back to the cat is still better than erroring mid-tick and killing
  -- the engine, but it is reported rather than passed off as the real asset.
  if not sprite then
    warn_fallback(asset_name)
    sprite = require(SPRITE_MODULES.cat)
  end

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

--- Mirrors a pixel matrix horizontally.
---
--- Rows are padded to the matrix width first. A ragged row reversed in place
--- would shift its pixels left by however many columns it was short, which
--- moves a mirrored sprite's art relative to its own bounding box.
function M.mirror_matrix(pixel_rows)
  local width = matrix_width(pixel_rows)
  local out = {}
  for r = 1, #pixel_rows do
    local row = pixel_rows[r]
    local flipped = {}
    for c = 1, width do
      flipped[c] = row[width - c + 1] or false
    end
    out[r] = flipped
  end
  return out
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
    local frames = M.get_pixel_frames(asset_name)
    local matrix = frames[frame_idx] or frames[1]
    if not matrix then
      return {}, {}, 0, 0
    end
    if flip_x then
      matrix = M.mirror_matrix(matrix)
    end
    local lines, highlights, w, h = M.render_halfblock_frame(matrix)
    entry = { lines = lines, highlights = highlights, width = w, height = h }
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
local function build_runs(lines, highlights)
  local by_row = {}
  for _, hl in ipairs(highlights) do
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
    local lines, highlights, w, h = M.get_rendered_frame(asset_name, frame_idx, flip_x)
    if w < 1 or h < 1 then
      return nil
    end
    entry = { rows = build_runs(lines, highlights), width = w, height = h }
    by_facing[frame_idx] = entry
  end

  return entry.rows, entry.width, entry.height
end

-- =========================================================================
-- Prepared frame buffers
-- =========================================================================

--- A frame's *content* is immutable, so the buffer holding it can be too.
---
--- Writing a frame used to cost `nvim_buf_set_lines` + `nvim_buf_clear_namespace`
--- + one `nvim_buf_set_extmark` per coloured cell — around 90 API calls for a
--- 24x16 sprite, every time the animation advanced, for every entity. A cat
--- sprinting at 12 FPS spent about 1,100 calls a second redrawing pictures it
--- had already drawn.
---
--- Each (asset, frame, facing) instead gets one scratch buffer, populated once.
--- Advancing the animation is then a single `nvim_win_set_buf`, and entities
--- sharing a frame share the buffer.
local frame_buffers = {}

local frame_ns = vim.api.nvim_create_namespace("distract_sprite_frames")

--- Namespace the prepared frame buffers carry their highlights in.
function M.frame_namespace()
  return frame_ns
end

local function build_frame_buffer(asset_name, frame_idx, flip_x)
  local lines, highlights, w, h = M.get_rendered_frame(asset_name, frame_idx, flip_x)
  if w < 1 or h < 1 then
    return nil
  end

  local buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)

  for _, hl in ipairs(highlights) do
    -- hl.col and hl.len are byte offsets: a half-block glyph is 3 bytes, so an
    -- end_col of col + 1 would split the codepoint and mis-colour the row.
    vim.api.nvim_buf_set_extmark(buf, frame_ns, hl.row, hl.col, {
      end_row = hl.row,
      end_col = hl.col + hl.len,
      hl_group = hl.hl,
      priority = 100,
    })
  end

  -- Nothing should be able to edit a sprite, and nothing should prompt about
  -- one on exit.
  vim.api.nvim_set_option_value("modifiable", false, { buf = buf })
  vim.api.nvim_set_option_value("bufhidden", "hide", { buf = buf })

  return { buf = buf, width = w, height = h }
end

--- A scratch buffer holding one frame, ready to be shown in a window.
---
--- Returns `bufnr, width, height`, or `nil` when the frame has no art.
function M.get_frame_buffer(asset_name, frame_idx, flip_x)
  local facing = flip_x and "flipped" or "facing"
  local by_asset = frame_buffers[asset_name]
  if not by_asset then
    by_asset = { facing = {}, flipped = {} }
    frame_buffers[asset_name] = by_asset
  end
  local by_facing = by_asset[facing]

  local entry = by_facing[frame_idx]
  -- A user is free to `:bwipeout` anything, including a sprite buffer, so a
  -- cached handle is checked rather than trusted.
  if entry and not vim.api.nvim_buf_is_valid(entry.buf) then
    entry = nil
  end
  if not entry then
    entry = build_frame_buffer(asset_name, frame_idx, flip_x)
    if not entry then
      return nil
    end
    by_facing[frame_idx] = entry
  end

  return entry.buf, entry.width, entry.height
end

--- Drops the render and frame-buffer caches, for one asset or for all of them.
--- Needed by tests, by `register`, and by a colourscheme reload, since
--- highlight groups are recreated lazily.
function M.reset_cache(asset_name)
  local function drop_buffers(by_asset)
    if not by_asset then
      return
    end
    for _, by_facing in pairs(by_asset) do
      for _, entry in pairs(by_facing) do
        if vim.api.nvim_buf_is_valid(entry.buf) then
          vim.api.nvim_buf_delete(entry.buf, { force = true })
        end
      end
    end
  end

  if asset_name then
    render_cache[asset_name] = nil
    runs_cache[asset_name] = nil
    drop_buffers(frame_buffers[asset_name])
    frame_buffers[asset_name] = nil
  else
    render_cache = {}
    runs_cache = {}
    for _, by_asset in pairs(frame_buffers) do
      drop_buffers(by_asset)
    end
    frame_buffers = {}
  end
end

return M
