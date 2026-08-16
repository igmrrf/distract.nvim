--- Scratch buffers holding one rendered sprite frame each.
---
--- A frame's *content* is immutable, so the buffer holding it can be too.
--- Writing a frame used to cost `nvim_buf_set_lines` + `nvim_buf_clear_namespace`
--- + one `nvim_buf_set_extmark` per coloured cell -- around 90 API calls for a
--- 24x16 sprite, every time the animation advanced, for every entity. A cat
--- sprinting at 12 FPS spent about 1,100 calls a second redrawing pictures it
--- had already drawn.
---
--- Each (asset, frame, facing) instead gets one buffer, populated once.
--- Advancing the animation is then a single `nvim_win_set_buf`, and entities
--- sharing a frame share the buffer.
---
--- Rendering lives in `distract.terminal_sprites`; this module only owns the
--- Neovim buffers that carry the result, and their lifetime.

local M = {}

---@class DistractRenderedFrame
---@field lines string[]
---@field highlights table[] `{ row, col, len, hl }`, byte offsets
---@field width integer terminal cells
---@field height integer terminal cells

local buffers = {}

local frame_ns = vim.api.nvim_create_namespace("distract_sprite_frames")

--- Namespace the prepared frame buffers carry their highlights in.
function M.namespace()
  return frame_ns
end

local function build(frame)
  local buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, frame.lines)

  for _, hl in ipairs(frame.highlights) do
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

  return { buf = buf, width = frame.width, height = frame.height }
end

--- The buffer holding one asset's frame, created on first use.
---
--- `key` identifies the frame within the asset -- its index and which way it
--- faces. The asset is separate because that is the granularity everything is
--- dropped at.
---@param asset_name string
---@param key string
---@param frame DistractRenderedFrame
---@return integer bufnr, integer width, integer height
function M.acquire(asset_name, key, frame)
  local by_asset = buffers[asset_name]
  if not by_asset then
    by_asset = {}
    buffers[asset_name] = by_asset
  end

  local entry = by_asset[key]
  -- A user is free to `:bwipeout` anything, including a sprite buffer, so a
  -- cached handle is checked rather than trusted.
  if entry and not vim.api.nvim_buf_is_valid(entry.buf) then
    entry = nil
  end
  if not entry then
    entry = build(frame)
    by_asset[key] = entry
  end

  return entry.buf, entry.width, entry.height
end

local function delete(by_asset)
  if not by_asset then
    return
  end
  for _, entry in pairs(by_asset) do
    if vim.api.nvim_buf_is_valid(entry.buf) then
      vim.api.nvim_buf_delete(entry.buf, { force = true })
    end
  end
end

--- Deletes the buffers held for one asset, or for all of them.
---@param asset_name string|nil
function M.release(asset_name)
  if asset_name then
    delete(buffers[asset_name])
    buffers[asset_name] = nil
    return
  end

  for _, by_asset in pairs(buffers) do
    delete(by_asset)
  end
  buffers = {}
end

return M
