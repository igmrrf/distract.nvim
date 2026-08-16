--- Kitty graphics protocol escape sequences.
---
--- Pure string building: nothing here touches the terminal, reads editor state,
--- or keeps a cache. That is what lets a headless test assert on chunk
--- boundaries, base64 payloads and diacritic encoding without a tty.
---
--- Every command carries `q=2`, which suppresses the terminal's reply. Without
--- it kitty answers each transmission with an `OK` on stdin, and Neovim reads
--- stdin as keystrokes.

local M = {}

local diacritics = require("distract.kitty.diacritics")

local APC = "\27_G"
local ST = "\27\\"

--- The codepoint a cell carries to say "part of an image goes here".
M.PLACEHOLDER = "\u{10EEEE}"

--- Largest base64 payload one escape may carry, per the protocol.
M.CHUNK_BYTES = 4096

--- Highest image id a two-diacritic placeholder can name.
---
--- A third diacritic would carry bits 24..31. Staying under 2^24 keeps every
--- cell three codepoints instead of four, and 16.7 million ids is more than a
--- session will ever allocate.
M.MAX_IMAGE_ID = 0xFFFFFF

--- Image id kitty is asked about when probing for protocol support.
M.PROBE_IMAGE_ID = 31

local function command(keys, payload)
  return APC .. keys .. ";" .. (payload or "") .. ST
end

--- The 24-bit colour that names an image inside a placeholder cell.
---
--- A placeholder's foreground colour *is* the image id -- the terminal reads it
--- as a number, not as a colour, and never paints it. Neovim can only set a
--- cell's foreground through a highlight group, so this is the hex the group
--- carries.
---@param image_id integer
---@return string hex colour, `#rrggbb`
function M.image_colour(image_id)
  if type(image_id) ~= "number" or image_id < 1 or image_id > M.MAX_IMAGE_ID then
    error(
      string.format(
        "distract.kitty: image id %s is outside 1..%d",
        tostring(image_id),
        M.MAX_IMAGE_ID
      )
    )
  end
  return string.format("#%06x", image_id)
end

--- One placeholder cell: the reserved codepoint plus its row and column.
---
--- The image id is not encoded here. It rides in the cell's foreground colour
--- for ids below 2^24, which is every id this backend allocates.
---@param row integer 0-based row within the image
---@param col integer 0-based column within the image
---@return string utf8
function M.cell(row, col)
  return M.PLACEHOLDER .. diacritics.char(row) .. diacritics.char(col)
end

--- A whole row of placeholder cells, `from_col` through `to_col` inclusive.
---@param row integer 0-based row within the image
---@param from_col integer 0-based, inclusive
---@param to_col integer 0-based, inclusive
---@return string utf8
function M.cell_run(row, from_col, to_col)
  local parts = {}
  for col = from_col, to_col do
    parts[#parts + 1] = M.cell(row, col)
  end
  return table.concat(parts)
end

--- What a transmission needs to know about the picture it is sending.
---@class DistractKittyImage
---@field id integer
---@field pixel_w integer
---@field pixel_h integer
---@field cols integer terminal cells the image occupies horizontally
---@field rows integer terminal cells it occupies vertically
---@field rgba string raw `pixel_w * pixel_h * 4` bytes

--- Transmits an image and creates a virtual placement for it.
---
--- `a=T,U=1` is transmit-and-display where the display is placeholder-driven:
--- the terminal holds the image and draws it wherever placeholder cells name
--- it, rather than at the cursor. `f=32` is raw RGBA, so there is no PNG
--- encoder and no zlib in the plugin.
---
--- Returns the escapes in order. They must be written back to back and to the
--- same terminal: a chunked transmission is one command split across several
--- sequences, and anything interleaved between them aborts it.
---@param image DistractKittyImage
---@return string[]
function M.transmit(image)
  local payload = vim.base64.encode(image.rgba)
  local first = string.format(
    "a=T,U=1,q=2,i=%d,f=32,s=%d,v=%d,c=%d,r=%d",
    image.id,
    image.pixel_w,
    image.pixel_h,
    image.cols,
    image.rows
  )

  local escapes = {}
  local offset = 1
  local total = #payload

  repeat
    local chunk = payload:sub(offset, offset + M.CHUNK_BYTES - 1)
    offset = offset + M.CHUNK_BYTES
    local more = offset <= total and 1 or 0
    if #escapes == 0 then
      escapes[1] = command(first .. ",m=" .. more, chunk)
    else
      escapes[#escapes + 1] = command("q=2,m=" .. more, chunk)
    end
  until offset > total

  return escapes
end

--- Deletes an image and frees the memory the terminal holds for it.
---
--- `d=I` rather than `d=i`: the lowercase form removes the placements and
--- leaves the pixels resident for the life of the terminal, which is the leak
--- the cache reset exists to prevent.
---@param image_id integer
---@return string
function M.delete(image_id)
  return command(string.format("a=d,d=I,i=%d,q=2", image_id))
end

--- Asks the terminal whether it speaks the protocol at all.
---
--- A one-pixel image is transmitted with `a=q`, which validates the command and
--- answers without storing anything. `q` is deliberately absent: this is the one
--- command whose reply is the entire point.
---@return string
function M.probe()
  return command(string.format("i=%d,s=1,v=1,a=q,t=d,f=24", M.PROBE_IMAGE_ID), "AAAA")
end

--- Whether a terminal reply is this module's probe answering yes.
---@param sequence string
---@return boolean
function M.is_probe_ok(sequence)
  if type(sequence) ~= "string" then
    return false
  end
  return sequence:find("_Gi=" .. M.PROBE_IMAGE_ID .. ";OK", 1, true) ~= nil
end

return M
