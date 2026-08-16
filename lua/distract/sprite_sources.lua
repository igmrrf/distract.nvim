--- Which art an asset has, and where it comes from.
---
--- Three sources, in precedence order: a sprite set registered at runtime, a
--- GIF the asset's manifest points at, and the procedural modules under
--- `distract.sprites`. An asset with none of those draws the cat and says so.
---
--- Rendering is `distract.terminal_sprites`' job; this module answers what
--- there is to render.

local gif_sprite = require("distract.gif.sprite")

local M = {}

--- Sprites are drawn procedurally by `distract.sprites.*` rather than stored as
--- hand-authored pixel tables. Each module returns its frames plus a `layout`
--- mapping state name -> 0-based frame indices, which the matching manifest
--- references directly so indices cannot drift out of sync with the art.
local SPRITE_MODULES = {
  cat = "distract.sprites.cat",
  crab = "distract.sprites.crab",
  sun = "distract.sprites.sun",
}

--- Generation is not free, so each asset is drawn once on first use and cached.
--- Requiring a sprite module only builds its pose curves and layout; the
--- rasterisation happens the first time frames are actually asked for. Every
--- manifest requires this module for its layout, so eager drawing cost ~10ms of
--- every Neovim startup whether or not anything was ever spawned.
local sprite_cache = {}

--- Sprite sets registered at runtime by user config or by an asset pack.
---
--- A registered entry is the same shape a built-in sprite module returns:
--- `{ frames = table|function, layout = table, width = n, height = n }`.
local registered = {}

--- GIF files an asset's manifest points at, decoded on first draw.
---@type table<string, DistractGifSource>
local gif_sources = {}

--- Assets whose GIF has already been reported as undecodable, for the same
--- reason a missing asset is: a file that fails to decode fails on every draw,
--- and it is not going to change mid-tick.
local decode_warned = {}

--- Reported once per asset, not once per draw: an unknown asset is asked for at
--- 30 FPS, and a notification per tick makes the editor unusable.
local fallback_warned = {}

local change_handler = nil

--- Called with an asset whose art has just changed, so whatever rendered it can
--- drop what it cached.
---@param handler fun(asset_name: string)|nil
function M.on_change(handler)
  change_handler = handler
end

local function announce_change(asset_name)
  sprite_cache[asset_name] = nil
  if change_handler then
    change_handler(asset_name)
  end
end

--- Whether an asset name has art this module can actually find.
function M.has_sprite(asset_name)
  return registered[asset_name] ~= nil
    or gif_sources[asset_name] ~= nil
    or SPRITE_MODULES[asset_name] ~= nil
    or sprite_cache[asset_name] ~= nil
end

--- Points an asset at the art its manifest declares.
---
--- Only a GIF is taken: it is the one image format that decodes without the
--- compiled engine, so any other spritesheet stays the overlay's to draw and
--- this asset keeps whatever registered or procedural art it already had.
---
--- Rebinding the same source is a no-op, which matters because a spawn re-reads
--- the manifest and would otherwise throw away a decoded animation every time.
---@param asset_name string
---@param manifest table|nil
function M.bind_manifest(asset_name, manifest)
  local source = gif_sprite.source_of(manifest)
  if gif_sprite.same_source(gif_sources[asset_name], source) then
    return
  end

  gif_sources[asset_name] = source
  decode_warned[asset_name] = nil
  announce_change(asset_name)
end

--- Forgets an asset's declared art. For tests, and for an asset being replaced.
---@param asset_name string
function M.unbind_manifest(asset_name)
  M.bind_manifest(asset_name, nil)
end

--- Registers a sprite set for `asset_name`, replacing any previous one.
---
--- This is what makes a custom asset drawable in the terminal. Without it the
--- only art that can be reached is the three built-ins.
function M.register(asset_name, sprite)
  if type(asset_name) ~= "string" or asset_name == "" then
    error("distract: register() needs an asset name")
  end
  if type(sprite) ~= "table" or sprite.frames == nil then
    error(string.format("distract: sprite set for '%s' has no `frames`", asset_name))
  end
  registered[asset_name] = sprite
  announce_change(asset_name)
end

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

local function warn_decode_failure(asset_name, source, error_message)
  if decode_warned[asset_name] then
    return
  end
  decode_warned[asset_name] = true
  vim.notify(
    string.format(
      "[Distract] Could not decode '%s' for asset '%s': %s",
      source.path,
      asset_name,
      error_message
    ),
    vim.log.levels.WARN
  )
end

--- Decodes the GIF an asset is bound to, or nil when there is none to decode.
local function load_gif_sprite(asset_name)
  local source = gif_sources[asset_name]
  if not source then
    return nil
  end

  local sprite, err = gif_sprite.build(source)
  if not sprite then
    warn_decode_failure(asset_name, source, err)
    return nil
  end
  return sprite
end

local function load_module_sprite(asset_name)
  local module_path = SPRITE_MODULES[asset_name]
  if module_path then
    return require(module_path)
  end

  -- A sprite module on the runtimepath is as good as a built-in, so an asset
  -- pack can ship `lua/distract/sprites/<name>.lua` and work without calling
  -- register().
  local ok, loaded = pcall(require, "distract.sprites." .. asset_name)
  if ok and type(loaded) == "table" and loaded.frames ~= nil then
    return loaded
  end
  return nil
end

local function load_sprite(asset_name)
  local cached = sprite_cache[asset_name]
  if cached then
    return cached
  end

  local sprite = registered[asset_name]
    or load_gif_sprite(asset_name)
    or load_module_sprite(asset_name)

  -- Falling back to the cat is still better than erroring mid-tick and killing
  -- the engine, but it is reported rather than passed off as the real asset.
  -- An asset whose GIF failed to decode has already been told exactly what went
  -- wrong; "no terminal art" on top of that says less, not more.
  if not sprite then
    if not gif_sources[asset_name] then
      warn_fallback(asset_name)
    end
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

--- How long the source file says one frame is shown for.
---
--- Only imported art has timing of its own; procedural art is drawn at whatever
--- rate its manifest asks for, so `nil` here means "the manifest decides".
---@param asset_name string
---@param frame_idx integer 1-based index into the asset's pixel frames
---@return integer|nil delay_ms
function M.frame_delay_ms(asset_name, frame_idx)
  local delays = load_sprite(asset_name).delays_ms
  return delays and delays[frame_idx] or nil
end

--- Whether an asset's art should be reduced to a smaller palette before it is
--- drawn in half-blocks. Imported art says so; procedural art does not need it.
---@param asset_name string
---@return boolean
function M.needs_quantising(asset_name)
  return load_sprite(asset_name).quantise == true
end

return M
