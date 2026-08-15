require("tests.test_harness")

local sprites = require("distract.terminal_sprites")

local ASSETS = { "cat", "crab", "sun" }

--- Declared pixel width of an asset's canvas.
local function cols_of(name)
  local w = select(1, sprites.get_dimensions(name))
  return w
end

--- Number of pixel cells in a matrix row, counting transparent cells.
--- Uses an explicit bound because a row containing nils would report length 0.
local function row_len(row, cols)
  local n = 0
  for i = 1, cols + 8 do
    if row[i] ~= nil then
      n = i
    end
  end
  return n
end

describe("distract.terminal_sprites pixel matrices", function()
  it("every frame row of every asset declares the full canvas width", function()
    for _, name in ipairs(ASSETS) do
      local cols = cols_of(name)
      for frame_no, matrix in ipairs(sprites.get_pixel_frames(name)) do
        for row_no, row in ipairs(matrix) do
          local len = row_len(row, cols)
          assert(
            len == cols,
            string.format(
              "%s frame %d row %d has %d cells, expected %d",
              name,
              frame_no,
              row_no,
              len,
              cols
            )
          )
        end
      end
    end
  end)

  it("every frame declares the same number of rows", function()
    for _, name in ipairs(ASSETS) do
      local expected = nil
      for frame_no, matrix in ipairs(sprites.get_pixel_frames(name)) do
        expected = expected or #matrix
        assert(
          #matrix == expected,
          string.format("%s frame %d has %d rows, expected %d", name, frame_no, #matrix, expected)
        )
      end
    end
  end)
end)

describe("distract.terminal_sprites half-block rendering", function()
  it("renders every line at the full sprite width in display cells", function()
    for _, name in ipairs(ASSETS) do
      local COLS = cols_of(name)
      for frame_no, matrix in ipairs(sprites.get_pixel_frames(name)) do
        local lines = sprites.render_halfblock_frame(matrix)
        for row_no, line in ipairs(lines) do
          local w = vim.fn.strdisplaywidth(line)
          assert(
            w == COLS,
            string.format(
              "%s frame %d line %d is %d display cells wide, expected %d",
              name,
              frame_no,
              row_no,
              w,
              COLS
            )
          )
        end
      end
    end
  end)

  it("reports sprite dimensions in display cells, not bytes", function()
    for _, name in ipairs(ASSETS) do
      local COLS = cols_of(name)
      for frame_no, matrix in ipairs(sprites.get_pixel_frames(name)) do
        local lines, _, w, h = sprites.render_halfblock_frame(matrix)
        assert(
          w == COLS,
          string.format(
            "%s frame %d reported width %s, expected %d",
            name,
            frame_no,
            tostring(w),
            COLS
          )
        )
        assert(
          h == #lines,
          string.format(
            "%s frame %d reported height %s, expected %d",
            name,
            frame_no,
            tostring(h),
            #lines
          )
        )
        assert(w > 0 and h > 0, "sprite dimensions must be positive")
      end
    end
  end)

  it("emits highlight columns as byte offsets that land on a block character", function()
    for _, name in ipairs(ASSETS) do
      for frame_no, matrix in ipairs(sprites.get_pixel_frames(name)) do
        local lines, highlights = sprites.render_halfblock_frame(matrix)
        for _, hl in ipairs(highlights) do
          local line = lines[hl.row + 1]
          assert.is_not_nil(
            line,
            string.format("%s frame %d: highlight row %d out of range", name, frame_no, hl.row)
          )
          local char = line:sub(hl.col + 1, hl.col + hl.len)
          assert(
            char == "\u{2580}" or char == "\u{2584}",
            string.format(
              "%s frame %d: highlight at byte col %d spans %q, expected a half-block character",
              name,
              frame_no,
              hl.col,
              char
            )
          )
        end
      end
    end
  end)

  it("emits an end column that does not split a multi-byte character", function()
    local matrix = sprites.get_pixel_frames("sun")[1]
    local lines, highlights = sprites.render_halfblock_frame(matrix)
    for _, hl in ipairs(highlights) do
      local line = lines[hl.row + 1]
      assert(
        hl.col + hl.len <= #line,
        string.format("highlight end byte %d exceeds line length %d", hl.col + hl.len, #line)
      )
      assert(
        hl.len == 3,
        string.format("half-block characters are 3 bytes, highlight reports len %d", hl.len)
      )
    end
  end)

  it("colours every block glyph it emits, leaving only spaces uncoloured", function()
    for _, name in ipairs(ASSETS) do
      for frame_no, matrix in ipairs(sprites.get_pixel_frames(name)) do
        local lines, highlights = sprites.render_halfblock_frame(matrix)

        local coloured = {}
        for _, hl in ipairs(highlights) do
          coloured[hl.row .. ":" .. hl.col] = true
        end

        for row_no, line in ipairs(lines) do
          local byte_col = 0
          while byte_col < #line do
            local char_len = vim.str_utf_end(line, byte_col + 1) + 1
            local char = line:sub(byte_col + 1, byte_col + char_len)
            if char ~= " " then
              assert(
                coloured[(row_no - 1) .. ":" .. byte_col],
                string.format(
                  "%s frame %d row %d byte %d: glyph %q has no highlight",
                  name,
                  frame_no,
                  row_no,
                  byte_col,
                  char
                )
              )
            end
            byte_col = byte_col + char_len
          end
        end
      end
    end
  end)

  it("colours every opaque pixel of a fully populated frame", function()
    local COLS = 16
    local row = {}
    for i = 1, COLS do
      row[i] = { 255, 0, 0 }
    end
    local lines, highlights = sprites.render_halfblock_frame({ row, row })
    assert(#lines == 1, "two pixel rows collapse into one half-block row")
    assert(
      #highlights == COLS,
      string.format("expected %d highlights for a full row, got %d", COLS, #highlights)
    )
    local seen = {}
    for _, hl in ipairs(highlights) do
      seen[hl.col] = true
    end
    for cell = 0, COLS - 1 do
      assert(seen[cell * 3], string.format("no highlight at byte offset %d", cell * 3))
    end
  end)
end)
