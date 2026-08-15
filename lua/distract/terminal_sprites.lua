local M = {}

-- Cache for dynamically generated Neovim highlight groups
local hl_cache = {}

--- Ensure a Neovim highlight group exists for foreground/background RGB colors
function M.get_hl_group(fg_rgb, bg_rgb)
  local key = string.format("Distract_%s_%s",
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
-- Procedural Color Palettes
-- =========================================================================
local O = { 245, 140, 40 }   -- Orange
local DO = { 200, 100, 20 }  -- Dark Orange
local W = { 255, 255, 255 }  -- White
local P = { 255, 160, 180 }  -- Pink
local K = { 40, 40, 40 }     -- Black / Dark Grey
local R = { 230, 50, 40 }    -- Red
local DR = { 180, 30, 25 }   -- Dark Red
local CL = { 250, 100, 60 }  -- Claw Orange
local G = { 255, 215, 0 }    -- Gold
local Y = { 255, 250, 180 }  -- Bright Yellow
local OG = { 255, 140, 20 }  -- Orange Glow
local MD = { 20, 20, 30 }    -- Moon Dark
local CG = { 255, 220, 100 } -- Corona Glow
local _ = nil                -- Transparent

-- =========================================================================
-- Half-Block Pixel-Art Frame Matrices (16 columns x 8 character rows = 16x16 pixels)
-- =========================================================================

-- CAT FRAMES (16x16 pixels -> 8 half-block rows)
local cat_pixel_frames = {
  -- Frame 1: Idle
  {
    { _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _ },
    { _, _, _, _, _, _, _, _, _, _, DO, _, _, DO, _, _ },
    { _, _, _, _, _, _, _, _, _, O, O, O, O, O, O, _ },
    { _, _, _, _, _, _, _, _, _, O, K, O, O, K, O, _ },
    { _, _, _, _, _, _, _, _, _, O, O, P, P, O, O, _ },
    { _, _, DO, _, O, O, O, O, O, O, O, O, O, O, _, _ },
    { _, _, DO, _, O, O, O, O, O, O, O, O, O, O, _, _ },
    { _, _, _, _, W, W, _, _, _, _, W, W, _, _, _, _ },
  },
  -- Frame 2: Walk 1
  {
    { _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _ },
    { _, _, _, _, _, _, _, _, _, _, DO, _, _, DO, _, _ },
    { _, _, _, _, _, _, _, _, _, O, O, O, O, O, O, _ },
    { _, _, _, _, _, _, _, _, _, O, K, O, O, K, O, _ },
    { _, _, _, _, _, _, _, _, _, O, O, P, P, O, O, _ },
    { _, DO, _, _, O, O, O, O, O, O, O, O, O, O, _, _ },
    { _, _, DO, _, O, O, O, O, O, O, O, O, O, O, _, _ },
    { _, W, W, _, _, _, _, _, _, _, _, _, W, W, _, _ },
  },
  -- Frame 3: Walk 2
  {
    { _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _ },
    { _, _, _, _, _, _, _, _, _, _, DO, _, _, DO, _, _ },
    { _, _, _, _, _, _, _, _, _, O, O, O, O, O, O, _ },
    { _, _, _, _, _, _, _, _, _, O, K, O, O, K, O, _ },
    { _, _, _, _, _, _, _, _, _, O, O, P, P, O, O, _ },
    { _, _, _, DO, O, O, O, O, O, O, O, O, O, O, _, _ },
    { _, _, DO, _, O, O, O, O, O, O, O, O, O, O, _, _ },
    { _, _, _, _, _, W, W, _, _, W, W, _, _, _, _, _ },
  },
  -- Frame 4: Sleep
  {
    { _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _ },
    { _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _ },
    { _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _ },
    { _, _, _, _, _, _, _, _, _, _, DO, _, DO, _, _, _ },
    { _, _, _, _, _, _, _, _, _, O, O, O, O, O, _, _ },
    { _, _, _, _, O, O, O, O, O, O, K, K, K, O, _, _ },
    { _, DO, DO, O, O, O, O, O, O, O, O, O, O, O, _, _ },
    { _, _, _, W, W, W, W, W, _, _, _, _, _, _, _, _ },
  },
}

-- CRAB FRAMES
local crab_pixel_frames = {
  -- Frame 1: Stand
  {
    { _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _ },
    { _, _, _, _, _, R, _, _, _, _, R, _, _, _, _, _ },
    { _, _, _, _, W, K, _, _, _, W, K, _, _, _, _, _ },
    { _, CL, CL, _, R, R, R, R, R, R, _, CL, CL, _, _, _ },
    { CL, CL, _, R, R, DR, DR, DR, DR, R, R, _, CL, CL, _ },
    { _, _, _, R, DR, DR, DR, DR, DR, DR, R, _, _, _, _ },
    { _, _, DR, _, _, DR, _, _, DR, _, _, DR, _, _, _ },
    { _, DR, _, _, DR, _, _, _, _, DR, _, _, DR, _, _ },
  },
  -- Frame 2: Walk Sideways
  {
    { _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _ },
    { _, _, _, _, _, R, _, _, _, _, R, _, _, _, _, _ },
    { _, _, _, _, W, K, _, _, _, W, K, _, _, _, _, _ },
    { CL, CL, _, _, R, R, R, R, R, R, _, _, CL, CL, _ },
    { _, CL, CL, R, R, DR, DR, DR, DR, R, R, CL, CL, _, _ },
    { _, _, _, R, DR, DR, DR, DR, DR, DR, R, _, _, _, _ },
    { _, DR, _, _, DR, _, _, _, _, DR, _, _, DR, _, _ },
    { DR, _, _, DR, _, _, _, _, _, _, DR, _, _, DR, _ },
  },
  -- Frame 3: Clip Claws Open
  {
    { _, CL, _, _, _, _, _, _, _, _, _, _, _, CL, _, _ },
    { CL, _, _, _, _, R, _, _, _, _, R, _, _, _, CL, _ },
    { _, _, _, _, W, K, _, _, _, W, K, _, _, _, _, _ },
    { _, CL, _, _, R, R, R, R, R, R, _, _, CL, _, _, _ },
    { CL, _, _, R, R, DR, DR, DR, DR, R, R, _, _, CL, _ },
    { _, _, _, R, DR, DR, DR, DR, DR, DR, R, _, _, _, _ },
    { _, _, DR, _, _, DR, _, _, DR, _, _, DR, _, _, _ },
    { _, DR, _, _, DR, _, _, _, _, DR, _, _, DR, _, _ },
  },
  -- Frame 4: Snapped Closed
  {
    { _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _ },
    { _, _, _, _, _, R, _, _, _, _, R, _, _, _, _, _ },
    { _, _, _, _, W, K, _, _, _, W, K, _, _, _, _, _ },
    { _, DR, DR, DR, R, R, R, R, R, R, DR, DR, DR, _, _, _ },
    { _, DR, DR, R, R, DR, DR, DR, DR, R, R, DR, DR, _, _ },
    { _, _, _, R, DR, DR, DR, DR, DR, DR, R, _, _, _, _ },
    { _, _, DR, _, _, DR, _, _, DR, _, _, DR, _, _, _ },
    { _, DR, _, _, DR, _, _, _, _, DR, _, _, DR, _, _ },
  },
}

-- SUN FRAMES
local sun_pixel_frames = {
  -- Frame 1: Pulse 1
  {
    { _, _, _, _, G, _, _, G, _, _, G, _, _, _, _, _ },
    { _, _, OG, G, G, G, G, G, G, G, G, OG, _, _, _, _ },
    { _, G, G, Y, Y, Y, Y, Y, Y, Y, G, G, _, _, _, _ },
    { G, G, Y, Y, Y, Y, Y, Y, Y, Y, Y, G, G, _, _, _ },
    { G, G, Y, Y, Y, Y, Y, Y, Y, Y, Y, G, G, _, _, _ },
    { _, G, G, Y, Y, Y, Y, Y, Y, Y, G, G, _, _, _, _ },
    { _, _, OG, G, G, G, G, G, G, G, G, OG, _, _, _, _ },
    { _, _, _, _, G, _, _, G, _, _, G, _, _, _, _, _ },
  },
  -- Frame 2: Pulse 2
  {
    { _, G, _, _, _, G, _, _, G, _, _, _, G, _, _, _ },
    { G, _, OG, G, G, G, G, G, G, G, G, OG, _, G, _, _ },
    { _, G, G, Y, Y, Y, Y, Y, Y, Y, G, G, _, _, _, _ },
    { _, G, Y, Y, Y, Y, Y, Y, Y, Y, Y, G, _, _, _, _ },
    { _, G, Y, Y, Y, Y, Y, Y, Y, Y, Y, G, _, _, _, _ },
    { _, G, G, Y, Y, Y, Y, Y, Y, Y, G, G, _, _, _, _ },
    { G, _, OG, G, G, G, G, G, G, G, G, OG, _, G, _, _ },
    { _, G, _, _, _, G, _, _, G, _, _, _, G, _, _, _ },
  },
  -- Frame 3: Partial Eclipse
  {
    { _, _, _, _, G, _, _, G, _, _, G, _, _, _, _, _ },
    { _, _, OG, G, G, G, G, G, G, G, G, OG, _, _, _, _ },
    { _, G, MD, MD, MD, MD, Y, Y, Y, Y, G, G, _, _, _, _ },
    { G, MD, MD, MD, MD, MD, MD, Y, Y, Y, Y, G, G, _, _ },
    { G, MD, MD, MD, MD, MD, MD, Y, Y, Y, Y, G, G, _, _ },
    { _, G, MD, MD, MD, MD, Y, Y, Y, Y, G, G, _, _, _, _ },
    { _, _, OG, G, G, G, G, G, G, G, G, OG, _, _, _, _ },
    { _, _, _, _, G, _, _, G, _, _, G, _, _, _, _, _ },
  },
  -- Frame 4: Total Eclipse with Corona
  {
    { _, _, _, CG, CG, CG, CG, CG, CG, CG, CG, _, _, _, _, _ },
    { _, CG, CG, MD, MD, MD, MD, MD, MD, MD, CG, CG, Y, _, _, _ },
    { _, CG, MD, MD, MD, MD, MD, MD, MD, MD, MD, CG, Y, Y, _, _ },
    { CG, MD, MD, MD, MD, MD, MD, MD, MD, MD, MD, CG, _, _, _, _ },
    { CG, MD, MD, MD, MD, MD, MD, MD, MD, MD, MD, CG, _, _, _, _ },
    { _, CG, MD, MD, MD, MD, MD, MD, MD, MD, MD, CG, _, _, _, _ },
    { _, CG, CG, MD, MD, MD, MD, MD, MD, MD, CG, CG, _, _, _, _ },
    { _, _, _, CG, CG, CG, CG, CG, CG, CG, CG, _, _, _, _, _ },
  },
}

-- ASCII text fallback representations
local ascii_sprites = {
  cat = {
    idle = { "(=^･ω･^=)", "(=^･ｪ･^=)" },
    walk = { " ~(=^･ω･^)~", "~(=^･ｪ･^)~ " },
    walk_fast = { ">>(=^･ω･^)>>", ">>(=^･ｪ･^)>>" },
    jump = { "/\\_/\\ ( >.< )", "/\\_/\\ ( ^.^ )" },
    yawn = { "(=^OωO^=)~", "(=^oωo^=)~" },
    sleep = { "(= -ω- =) zZ", "(= -.- =) zZ" },
  },
  crab = {
    idle = { "(V) (°,,,,°) (V)", "(V) ( °..° ) (V)" },
    walk = { "(V) ( .. ) (V)", "(v) ( .. ) (v)" },
    walk_fast = { ">>(V) ( .. ) (V)>>", ">>(v) ( .. ) (v)>>" },
    clip_claws = { "(>) (°,,,,°) (<)", "(<) (°,,,,°) (>)" },
    burrow = { ".. ( .. ) ..", "_.. ( .. ) .._" },
    sleep = { "(v) (- -) (v) zZ", "(v) (..) (v) zZ" },
  },
  sun = {
    shining = { "( ☼ )", "\\ ☼ /", "( ☼ )", "/ ☼ \\" },
    rising = { "_/\\_ ☼", "  /\\ ☼" },
    setting = { "☼ _/\\_", "☼ \\_ " },
    eclipse = { "( ◐ )", "( 🌑 )" },
    flare = { "*:.☼.:*", ".:*☼*:." },
  },
}

--- Converts a pixel matrix into a table of half-block strings and extmark highlight spans
function M.render_halfblock_frame(pixel_rows)
  local lines = {}
  local highlights = {} -- list of {row, col_start, col_end, hl_group}

  for r = 1, #pixel_rows, 2 do
    local top_row = pixel_rows[r] or {}
    local bot_row = pixel_rows[r + 1] or {}
    local line_chars = {}
    local row_idx = #lines -- 0-indexed for Neovim

    for c = 1, #top_row do
      local top_color = top_row[c]
      local bot_color = bot_row[c]

      if top_color and bot_color then
        table.insert(line_chars, "▀")
        local hl = M.get_hl_group(top_color, bot_color)
        table.insert(highlights, { row = row_idx, col = #line_chars - 1, hl = hl })
      elseif top_color and not bot_color then
        table.insert(line_chars, "▀")
        local hl = M.get_hl_group(top_color, nil)
        table.insert(highlights, { row = row_idx, col = #line_chars - 1, hl = hl })
      elseif not top_color and bot_color then
        table.insert(line_chars, "▄")
        local hl = M.get_hl_group(bot_color, nil)
        table.insert(highlights, { row = row_idx, col = #line_chars - 1, hl = hl })
      else
        table.insert(line_chars, " ")
      end
    end

    table.insert(lines, table.concat(line_chars))
  end

  return lines, highlights
end

function M.get_pixel_frames(asset_name)
  if asset_name == "crab" then
    return crab_pixel_frames
  elseif asset_name == "sun" then
    return sun_pixel_frames
  else
    return cat_pixel_frames
  end
end

function M.get_ascii_sprite(asset_name, state_name, frame_idx)
  local asset = ascii_sprites[asset_name] or ascii_sprites.cat
  local state_frames = asset[state_name] or asset.idle or { "(^・ω・^)" }
  local idx = ((frame_idx - 1) % #state_frames) + 1
  return state_frames[idx]
end

return M
