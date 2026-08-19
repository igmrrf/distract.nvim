--- A GIF decoder with no dependencies and no external process.
---
--- GIF87a and GIF89a, LZW, interlacing, global and local palettes, the
--- transparency index and disposal methods 0-3. What comes out is what the rest
--- of the plugin already draws: a list of frames, each a 1-based `[row][col]`
--- matrix of `{r, g, b}` or `false`, in the same shape `distract.sprites.*`
--- produce, so a GIF asset reaches the half-block and kitty backends through
--- exactly the code path a procedural one does.

local parser = require("distract.gif.parser")
local resample = require("distract.resample")

local M = {}

--- Bounds, in the spirit of every other boundary in this plugin: a malformed or
--- hostile file fails with a message rather than exhausting memory.
---
--- `MAX_CANVAS_DIM` and `MAX_FRAMES` mirror `engine/src/asset.rs`, so a file
--- the overlay refuses is refused here too and for the same reason.
M.MAX_CANVAS_DIM = 4096
M.MAX_FRAMES = 512

--- The largest sprite this module will materialise when no target size is
--- given. A GIF authored at screen size has to say what it should be drawn at;
--- silently building a 1600-cell-wide sprite is not a smaller failure than
--- refusing to.
M.MAX_SPRITE_CELLS = 65536

---@class DistractGifFrame
---@field pixels table<integer, table<integer, integer[]|false>> 1-based `[row][col]`
---@field delay_ms integer the frame's own display time, 0 when the file omits one

---@class DistractGif
---@field width integer sprite pixels, after resampling
---@field height integer sprite pixels, after resampling
---@field frames DistractGifFrame[]

local DISPOSAL_RESTORE_BACKGROUND = 2
local DISPOSAL_RESTORE_PREVIOUS = 3

local function new_canvas(cell_count)
  local canvas = { red = {}, green = {}, blue = {}, opaque = {} }
  for index = 1, cell_count do
    canvas.red[index] = 0
    canvas.green[index] = 0
    canvas.blue[index] = 0
    canvas.opaque[index] = false
  end
  return canvas
end

local function snapshot(canvas, cell_count)
  local copy = { red = {}, green = {}, blue = {}, opaque = {} }
  for index = 1, cell_count do
    copy.red[index] = canvas.red[index]
    copy.green[index] = canvas.green[index]
    copy.blue[index] = canvas.blue[index]
    copy.opaque[index] = canvas.opaque[index]
  end
  return copy
end

local function restore(canvas, saved, cell_count)
  for index = 1, cell_count do
    canvas.red[index] = saved.red[index]
    canvas.green[index] = saved.green[index]
    canvas.blue[index] = saved.blue[index]
    canvas.opaque[index] = saved.opaque[index]
  end
end

--- Clears the rectangle an image occupied, which is what disposal method 2 asks
--- for. The background *colour* is deliberately not painted: an entity is drawn
--- over editor text, so "restore to background" has to mean "let the editor
--- through" rather than "paint the author's backdrop".
local function clear_rect(canvas, image, screen_width)
  for row = image.top, image.top + image.height - 1 do
    local base = row * screen_width
    for column = image.left, image.left + image.width - 1 do
      canvas.opaque[base + column + 1] = false
    end
  end
end

local function draw(canvas, image, screen)
  local palette = image.palette
  local transparent_index = image.transparent_index

  for row = 0, image.height - 1 do
    local screen_row = image.top + row
    if screen_row >= 0 and screen_row < screen.height then
      local source_base = row * image.width
      local target_base = screen_row * screen.width
      for column = 0, image.width - 1 do
        local screen_column = image.left + column
        local index = image.indices[source_base + column + 1]
        if screen_column >= 0 and screen_column < screen.width and index ~= transparent_index then
          local colour = palette[index + 1]
          if colour then
            local target = target_base + screen_column + 1
            canvas.red[target] = colour[1]
            canvas.green[target] = colour[2]
            canvas.blue[target] = colour[3]
            canvas.opaque[target] = true
          end
        end
      end
    end
  end
end

local function target_size(screen, opts)
  local width = opts.target_width or screen.width
  local height = opts.target_height or screen.height
  if width < 1 or height < 1 then
    return nil, "a GIF sprite must be at least one pixel in each direction"
  end
  if width * height > M.MAX_SPRITE_CELLS then
    return nil,
      string.format(
        "a %dx%d GIF sprite is over the %d-pixel budget; declare "
          .. "`spritesheet.frame_width` and `spritesheet.frame_height` to draw it smaller",
        width,
        height,
        M.MAX_SPRITE_CELLS
      )
  end
  return { width = width, height = height }
end

local function check_canvas(screen)
  if screen.width < 1 or screen.height < 1 then
    return "GIF declares an empty canvas"
  end
  if screen.width > M.MAX_CANVAS_DIM or screen.height > M.MAX_CANVAS_DIM then
    return string.format(
      "GIF canvas is %dx%d, over the %d-pixel limit",
      screen.width,
      screen.height,
      M.MAX_CANVAS_DIM
    )
  end
  return nil
end

--- Composites every image onto a running canvas and resamples each result.
---
--- Disposal relates one frame to the next, so it is applied here rather than in
--- the parser: the method on image N says what to do to the canvas *after* it
--- has been shown, which only the frame that follows it can act on.
--- Draws every image onto the shared canvas in order, honouring disposal, and
--- resamples each composed frame to the target size.
---
--- `opts.on_frame` is called once per composed frame. It exists so a caller
--- running inside a coroutine can yield here: composing a 32-frame 1920x1080
--- GIF costs ~220ms, which is thirteen dropped frames if it happens in one go
--- on the main loop.
---@param images table[]
---@param screen table
---@param opts table `{ target = table, on_frame = fun(index: integer)|nil }`
local function compose(images, screen, opts)
  local target, on_frame = opts.target, opts.on_frame
  local cell_count = screen.width * screen.height
  local canvas = new_canvas(cell_count)
  local frames = {}
  local pending_disposal = nil
  local saved = nil

  for _, image in ipairs(images) do
    if pending_disposal then
      if pending_disposal.method == DISPOSAL_RESTORE_BACKGROUND then
        clear_rect(canvas, pending_disposal.image, screen.width)
      elseif pending_disposal.method == DISPOSAL_RESTORE_PREVIOUS and saved then
        restore(canvas, saved, cell_count)
      end
    end

    saved = image.disposal == DISPOSAL_RESTORE_PREVIOUS and snapshot(canvas, cell_count) or saved
    draw(canvas, image, screen)

    frames[#frames + 1] = {
      pixels = resample.to_matrix(canvas, screen, target),
      delay_ms = image.delay_ms,
    }
    pending_disposal = { method = image.disposal, image = image }
    if on_frame then
      on_frame(#frames)
    end
  end

  return frames
end

--- Decodes a GIF held in memory.
---@param bytes string
---@param opts table|nil `{ target_width = <sprite pixels>, target_height = <sprite pixels> }`
---@return DistractGif|nil gif, string|nil error_message
function M.decode_bytes(bytes, opts)
  opts = opts or {}
  if type(bytes) ~= "string" then
    return nil, "distract.gif: expected GIF bytes"
  end

  local screen, header_err = parser.read_header(bytes)
  if not screen then
    return nil, header_err
  end

  local canvas_err = check_canvas(screen)
  if canvas_err then
    return nil, canvas_err
  end

  local target, target_err = target_size(screen, opts)
  if not target then
    return nil, target_err
  end

  local images, images_err =
    parser.read_images(bytes, screen, { max_frames = M.MAX_FRAMES, on_frame = opts.on_frame })
  if not images then
    return nil, images_err
  end
  if #images == 0 then
    return nil, "GIF contains no frames"
  end

  return {
    width = target.width,
    height = target.height,
    frames = compose(images, screen, { target = target, on_frame = opts.on_frame }),
  }
end

--- Decodes a GIF from disk.
---@param path string
---@param opts table|nil see `decode_bytes`
---@return DistractGif|nil gif, string|nil error_message
function M.decode(path, opts)
  if type(path) ~= "string" or path == "" then
    return nil, "distract.gif: expected a file path"
  end

  local handle, open_err = io.open(path, "rb")
  if not handle then
    return nil, string.format("could not open '%s': %s", path, tostring(open_err))
  end

  local bytes = handle:read("*a")
  handle:close()
  if not bytes then
    return nil, string.format("could not read '%s'", path)
  end

  return M.decode_bytes(bytes, opts)
end

return M
