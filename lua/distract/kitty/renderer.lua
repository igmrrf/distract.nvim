--- The kitty graphics backend's half of the in-terminal renderer.
---
--- It supplies content only: a buffer of placeholder cells for the float and
--- the same cells as runs for the buffer overlay. Placement, the overlay/float
--- split and the redraw guard belong to `distract.renderer` and are shared with
--- the half-block backend, which is what keeps the two from drifting.
---
--- Placeholders rather than direct cursor placement, so the image rides the
--- overlay path Neovim already redraws correctly. Direct placement would have
--- to fight the editor's own redraw and carry a second positioning system.

local M = {}

local api = vim.api
local protocol = require("distract.kitty.protocol")
local frames = require("distract.kitty.frames")
local writer = require("distract.kitty.writer")
local renderer = require("distract.renderer")
local sprites = require("distract.terminal_sprites")

local frame_ns = api.nvim_create_namespace("distract_kitty_frames")

--- The graphics protocol transmits real pixels, so this backend asks for an
--- asset's native-resolution art where its manifest declares one. Passed as a
--- literal rather than looked up through `distract.backends`: this module is the
--- kitty backend's own internals, and `kitty/init.lua` already requires it, so
--- reading the registry back would be a circular require. Hoisted rather than
--- built per call: this runs once per entity per tick.
---@type table
local KITTY_CAPABILITY = { native_resolution = true }

--- Image ids this Neovim may allocate.
---
--- Ids are terminal-wide, not per-process, so two editors sharing one window
--- would otherwise overwrite each other's sprites. Deriving the base from the
--- pid makes a collision need two processes whose pids are congruent modulo
--- 32767 in the same terminal, rather than making it the default.
local IDS_PER_SESSION = 512

local id_base = nil
local next_offset = 0

local function allocate_id()
  if not id_base then
    id_base = (vim.fn.getpid() % 32767) * IDS_PER_SESSION + 1
  end
  if next_offset >= IDS_PER_SESSION then
    error(
      string.format(
        "distract.kitty: this session has transmitted %d images, its whole id range",
        IDS_PER_SESSION
      )
    )
  end
  local id = id_base + next_offset
  next_offset = next_offset + 1
  return id
end

--- The highlight group whose foreground names an image.
---
--- The terminal reads a placeholder's foreground as a 24-bit image id, never as
--- a colour, so this group is never seen. It does need `termguicolors`: without
--- it Neovim rounds the "colour" to the nearest palette entry and the id
--- arrives as a different number.
local function image_group(image_id)
  local group = "DistractKittyImage" .. image_id
  api.nvim_set_hl(0, group, { fg = protocol.image_colour(image_id) })
  return group
end

--- The float shows every cell of the frame, transparent ones included, which is
--- correct here in a way it is not for half-blocks: the placeholder cell has no
--- colour of its own and the image's own alpha decides what is painted.
local function placeholder_lines(cols, rows)
  local lines = {}
  for row = 0, rows - 1 do
    lines[row + 1] = protocol.cell_run(row, 0, cols - 1)
  end
  return lines
end

local function build_buffer(cols, rows, group)
  local lines = placeholder_lines(cols, rows)
  local buf = api.nvim_create_buf(false, true)
  api.nvim_buf_set_lines(buf, 0, -1, false, lines)

  for row = 0, rows - 1 do
    api.nvim_buf_set_extmark(buf, frame_ns, row, 0, {
      end_row = row,
      end_col = #lines[row + 1],
      hl_group = group,
      priority = 100,
    })
  end

  api.nvim_set_option_value("modifiable", false, { buf = buf })
  api.nvim_set_option_value("bufhidden", "hide", { buf = buf })
  return buf
end

--- The overlay's view: only the cells that have a pixel in them.
local function build_runs(spans, group)
  local rows = {}
  for row, row_spans in pairs(spans) do
    local runs = {}
    for _, span in ipairs(row_spans) do
      runs[#runs + 1] = {
        col = span[1],
        chunks = { { protocol.cell_run(row, span[1], span[2]), group } },
      }
    end
    rows[row] = runs
  end
  return rows
end

local placements = {}

--- The cell rectangle a sprite occupies once its depth is taken into account.
---
--- Parallax is what `z` buys on a backend that can scale, and kitty can: `c`
--- and `r` tell the terminal how many cells to resample the image into. The
--- engine already damps a distant entity's movement and measures its floor
--- against this same scaled footprint, so drawing it unscaled here would put
--- the two back out of step.
local function scaled_rect(frame, parallax)
  return math.max(1, math.floor(frame.cols * parallax + 0.5)),
    math.max(1, math.floor(frame.rows * parallax + 0.5))
end

--- Transmits a placement if it has not been sent, and returns how to draw it.
---
--- Keyed on the scaled cell rectangle as well as the frame: a virtual placement
--- fixes `c` and `r` at transmission, so the same art at two depths is two
--- images. Distinct rectangles are bounded by the sprite's own size, so this
--- does not grow without limit.
---
--- The transmission is the only I/O, and it happens once per key per session.
--- Everything after it is buffer and extmark work the redraw guard skips
--- entirely while an entity stands still.
local function prepare(asset_name, frame, cols, rows)
  local key = string.format("%s:%s:%d:%d", asset_name, frame.key, cols, rows)
  local entry = placements[key]
  if entry and api.nvim_buf_is_valid(entry.buf) then
    return entry
  end

  local image_id = entry and entry.image_id or allocate_id()
  local group = image_group(image_id)

  if not entry then
    writer.write_all(protocol.transmit({
      id = image_id,
      pixel_w = frame.pixel_w,
      pixel_h = frame.pixel_h,
      cols = cols,
      rows = rows,
      rgba = frame.rgba,
    }))
  end

  entry = {
    image_id = image_id,
    buf = build_buffer(cols, rows, group),
    runs = build_runs(frames.spans(frame, cols, rows), group),
    cols = cols,
    rows = rows,
  }
  placements[key] = entry
  return entry
end

--- Builds one entity's kitty surface.
---@param entity table
---@return DistractFrameSurface|nil
function M.surface(entity)
  local frame_count = #sprites.get_pixel_frames(entity.asset_name, KITTY_CAPABILITY)
  local frame_idx = renderer.resolve_pixel_frame(entity, frame_count)
  local flip_x = renderer.resolve_flip(entity)

  local frame = frames.describe(entity.asset_name, frame_idx, flip_x)
  if not frame then
    return nil
  end

  local cols, rows = scaled_rect(frame, entity.parallax or 1.0)
  local entry = prepare(entity.asset_name, frame, cols, rows)
  if not entry then
    return nil
  end

  return {
    key = entry.buf,
    buf = entry.buf,
    width = entry.cols,
    height = entry.rows,
    runs = function()
      return entry.runs
    end,
  }
end

--- Drops every transmitted image, freeing the terminal's memory for it.
---
--- Placeholder cells left on screen after this point name an image that no
--- longer exists and draw nothing, which is why the renderer's own windows go
--- first.
function M.reset()
  for _, entry in pairs(placements) do
    writer.write(protocol.delete(entry.image_id))
    if api.nvim_buf_is_valid(entry.buf) then
      api.nvim_buf_delete(entry.buf, { force = true })
    end
  end
  placements = {}
  frames.reset()
end

--- Image ids currently transmitted, for tests and diagnostics.
---@return integer[]
function M.transmitted_ids()
  local ids = {}
  for _, entry in pairs(placements) do
    ids[#ids + 1] = entry.image_id
  end
  table.sort(ids)
  return ids
end

return M
