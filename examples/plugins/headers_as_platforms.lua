-- Solid ground made out of the code you are looking at.
--
-- Every function header, class or struct on screen becomes a platform a grounded
-- pet walks along, and every closed fold becomes one too. Walk off the end of one
-- and the pet drops to the next surface under it.
--
-- This is `docs/ecosystem-roadmap.md` §2.4 in the smallest form that works. A real
-- `distract-physics` would use a Tree-sitter query rather than a pattern; the
-- surface it registers through is exactly the one used here.
--
-- The provider is called on a debounced cadence -- editing, scrolling, window
-- changes -- and never per tick per entity, which is the whole reason the contract
-- takes a function rather than a list.

local distract = require("distract")

--- Patterns that look like the start of a block worth standing on.
---
--- Deliberately crude: the point of the example is the registration surface, and a
--- language-accurate answer is what Tree-sitter is for.
local HEADER_PATTERNS = {
  "^%s*function%s",
  "^%s*local%s+function%s",
  "^%s*def%s",
  "^%s*class%s",
  "^%s*struct%s",
  "^%s*fn%s",
  "^%s*pub%s+fn%s",
  "^%s*impl%s",
  "^%-%-%-+%s*$",
  "^===+%s*$",
}

--- The most rows scanned, so a very tall window cannot make this expensive.
local MAX_ROWS = 200

local function looks_like_a_header(line)
  for _, pattern in ipairs(HEADER_PATTERNS) do
    if line:match(pattern) then
      return true
    end
  end
  return false
end

--- The screen row a buffer line is displayed on, or nil when it is not visible.
local function screen_row_of(win_id, lnum)
  local position = vim.fn.screenpos(win_id, lnum, 1)
  if position.row == 0 then
    return nil
  end
  -- `screenpos` is 1-based and the obstacle vocabulary is 0-based screen cells,
  -- the same convention the floor is measured in.
  return position.row - 1
end

distract.register_obstacle_provider(function(win_id, buf_id)
  local rects = {}

  local first_visible = vim.fn.line("w0", win_id)
  local last_visible = math.min(vim.fn.line("w$", win_id), first_visible + MAX_ROWS)
  local info = vim.fn.getwininfo(win_id)[1]
  if not info then
    return rects
  end

  local text_left = info.wincol - 1 + (info.textoff or 0)
  local text_width = math.max(1, info.width - (info.textoff or 0))

  for lnum = first_visible, last_visible do
    local line = vim.api.nvim_buf_get_lines(buf_id, lnum - 1, lnum, false)[1]
    if line and looks_like_a_header(line) then
      local row = screen_row_of(win_id, lnum)
      if row then
        -- The platform spans the indented text, not the whole window: a pet
        -- should walk the length of the signature it is standing on.
        local indent = #(line:match("^%s*") or "")
        table.insert(rects, {
          x = text_left + indent,
          y = row,
          width = math.max(4, math.min(text_width - indent, #line - indent)),
          height = 1,
          type = "solid_platform",
        })
      end
    end
  end

  -- A closed fold is a solid line of text, so it is solid ground.
  local lnum = first_visible
  while lnum <= last_visible do
    local fold_end = vim.fn.foldclosedend(lnum)
    if fold_end > 0 then
      local row = screen_row_of(win_id, lnum)
      if row then
        table.insert(rects, {
          x = text_left,
          y = row,
          width = text_width,
          height = 1,
          type = "solid_platform",
        })
      end
      lnum = fold_end + 1
    else
      lnum = lnum + 1
    end
  end

  return rects
end)
