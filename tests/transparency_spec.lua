require("tests.test_harness")

local renderer = require("distract.renderer")
local sprites = require("distract.terminal_sprites")

--- A cat entity placed at a given screen cell.
local function cat_at(x, y)
  return {
    id = 1,
    asset_name = "cat",
    x = x,
    y = y,
    frame_idx = 1,
    current_state = "idle",
    z_index = 10,
    flip_x = false,
    manifest = require("distract.manifests.cat"),
  }
end

--- Fills the current buffer and drops any cached layout.
local function with_buffer(lines)
  renderer.clear_all()
  renderer.invalidate_screen_map()
  vim.api.nvim_buf_set_lines(0, 0, -1, false, lines)
  vim.cmd("redraw")
end

local function long_file(n)
  local lines = {}
  for i = 1, n do
    lines[i] = string.rep("ABCDEFGHIJ", 4)
  end
  return lines
end

describe("distract sprite frame runs", function()
  it("describes a row as runs of drawn cells, skipping transparent ones", function()
    local rows, w, h = sprites.get_frame_runs("cat", 1, false)
    assert.is_not_nil(rows)
    assert(w > 0 and h > 0)

    -- Every chunk must be non-empty and carry a highlight; a chunk of spaces
    -- would occlude the code underneath, which is the whole thing being fixed.
    local total_runs = 0
    for r = 0, h - 1 do
      for _, run in ipairs(rows[r] or {}) do
        total_runs = total_runs + 1
        assert(run.col >= 0, "run column must be within the sprite")
        for _, chunk in ipairs(run.chunks) do
          assert(#chunk[1] > 0, "empty chunk")
          assert(chunk[1]:find(" ", 1, true) == nil, "a run must not contain a blank cell")
          assert.is_not_nil(chunk[2], "every chunk needs a highlight group")
        end
      end
    end
    assert(total_runs > 0, "a cat frame should produce runs")
  end)

  it("merges adjacent cells that share a highlight into one chunk", function()
    local rows, _, h = sprites.get_frame_runs("cat", 1, false)
    local cells, chunks = 0, 0
    for r = 0, h - 1 do
      for _, run in ipairs(rows[r] or {}) do
        for _, chunk in ipairs(run.chunks) do
          chunks = chunks + 1
          cells = cells + vim.fn.strchars(chunk[1])
        end
      end
    end
    assert(chunks <= cells, "merging cannot produce more chunks than cells")
  end)
end)

describe("distract in-terminal transparency", function()
  after_each(function()
    renderer.clear_all()
  end)

  it("draws onto buffer text without blanking a single cell of it", function()
    with_buffer(long_file(vim.o.lines + 5))

    local function row_text(r)
      local cells = {}
      for c = 1, 45 do
        cells[c] = vim.fn.screenstring(r, c)
      end
      return table.concat(cells)
    end

    local top = 6
    local before = {}
    for r = top, top + 7 do
      before[r] = row_text(r)
    end

    renderer.draw({ cat_at(10, top - 1) }, "halfblock")
    vim.cmd("redraw")

    local destroyed, sprite_cells = 0, 0
    for r = top, top + 7 do
      local after = row_text(r)
      for c = 1, 45 do
        local b, a = before[r]:sub(c, c), after:sub(c, c)
        if b:match("[A-J]") and a == " " then
          destroyed = destroyed + 1
        end
        if a ~= " " and not a:match("^[A-J~]$") then
          sprite_cells = sprite_cells + 1
        end
      end
    end

    assert(sprite_cells > 0, "the sprite must actually be drawn")
    assert.are_equal(0, destroyed, "a sprite over code must not blank a single character of it")
  end)

  it("uses no float at all when every row lands on buffer text", function()
    with_buffer(long_file(vim.o.lines + 5))
    renderer.draw({ cat_at(10, 5) }, "halfblock")

    local st = renderer.window_state(1)
    assert.is_not_nil(st)
    assert.are_equal(0, st.float_height, "no rows should need a float")
    assert.is_nil(st.win, "a fully overlaid sprite must not open a window")
    assert(st.overlay_marks > 0, "it should have placed overlay extmarks")
  end)

  it("falls back to a float for rows past the end of the buffer", function()
    with_buffer({ "AAAA", "BBBB", "CCCC" })
    renderer.draw({ cat_at(10, 12) }, "halfblock")

    local st = renderer.window_state(1)
    assert.are_equal(0, st.overlay_limit, "no row sits over text down there")
    assert.are_equal(st.height, st.float_height, "the whole sprite needs the float")
    assert.is_not_nil(st.win)
    assert.are.same(
      { st.float_row, st.col },
      vim.api.nvim_win_get_position(st.win),
      "the float must sit where the entity is"
    )
  end)

  it("splits between both surfaces when the sprite straddles the last line", function()
    with_buffer({ "AAAA", "BBBB", "CCCC" })
    renderer.draw({ cat_at(10, 1) }, "halfblock")

    local st = renderer.window_state(1)
    assert(st.overlay_limit > 0, "rows over the text should be overlaid")
    assert(st.float_height > 0, "rows past the text should use the float")
    assert.are_equal(st.height, st.overlay_limit + st.float_height)
    assert.are_equal(st.row + st.overlay_limit, st.float_row)

    -- The float shows the tail of the frame, not the top of it.
    local view = vim.api.nvim_win_call(st.win, function()
      return vim.fn.winsaveview()
    end)
    assert.are_equal(
      st.overlay_limit + 1,
      view.topline,
      "the float must be scrolled to the first row it is responsible for"
    )
  end)

  it("removes its overlay extmarks when the entity goes away", function()
    with_buffer(long_file(vim.o.lines + 5))
    renderer.draw({ cat_at(10, 5) }, "halfblock")

    local ns = renderer.overlay_namespace()
    local buf = vim.api.nvim_get_current_buf()
    assert(#vim.api.nvim_buf_get_extmarks(buf, ns, 0, -1, {}) > 0)

    renderer.draw({}, "halfblock")
    assert.are_equal(
      0,
      #vim.api.nvim_buf_get_extmarks(buf, ns, 0, -1, {}),
      "a despawned entity must leave nothing behind in the user's buffer"
    )
  end)

  it("costs no API calls while nothing moves", function()
    with_buffer(long_file(vim.o.lines + 5))
    local e = cat_at(10, 5)
    renderer.draw({ e }, "halfblock")

    local calls = 0
    local o_ext = vim.api.nvim_buf_set_extmark
    local o_del = vim.api.nvim_buf_del_extmark
    local o_cfg = vim.api.nvim_win_set_config
    vim.api.nvim_buf_set_extmark = function(...)
      calls = calls + 1
      return o_ext(...)
    end
    vim.api.nvim_buf_del_extmark = function(...)
      calls = calls + 1
      return o_del(...)
    end
    vim.api.nvim_win_set_config = function(...)
      calls = calls + 1
      return o_cfg(...)
    end

    for _ = 1, 10 do
      renderer.draw({ e }, "halfblock")
    end

    vim.api.nvim_buf_set_extmark = o_ext
    vim.api.nvim_buf_del_extmark = o_del
    vim.api.nvim_win_set_config = o_cfg

    assert.are_equal(0, calls, "a stationary sprite must not touch the API at all")
  end)

  it("redraws the overlay when the view scrolls under it", function()
    with_buffer(long_file(vim.o.lines + 40))
    local e = cat_at(10, 5)
    renderer.draw({ e }, "halfblock")
    local first = renderer.window_state(1)

    vim.cmd("normal! 20\26E")
    vim.cmd("redraw")
    renderer.draw({ e }, "halfblock")
    local second = renderer.window_state(1)

    assert.is_not_nil(second)
    assert(second.overlay_marks > 0, "the sprite must still be drawn after a scroll")
    local _ = first
  end)
end)

describe("distract colourscheme recovery", function()
  it("rebuilds sprite highlights after :hi clear deletes them", function()
    local sprites_mod = require("distract.terminal_sprites")
    local group = sprites_mod.get_hl_group({ 10, 20, 30 }, nil)
    assert.is_not_nil(vim.api.nvim_get_hl(0, { name = group }).fg)

    -- What `:colorscheme` does.
    vim.cmd("hi clear")
    sprites_mod.reset_highlights()

    local again = sprites_mod.get_hl_group({ 10, 20, 30 }, nil)
    assert.are_equal(group, again)
    assert.is_not_nil(
      vim.api.nvim_get_hl(0, { name = again }).fg,
      "a sprite colour must be re-declared after the colourscheme cleared it"
    )
  end)

  it("registers a ColorScheme autocmd that does the cleanup", function()
    require("distract").setup({ backend = "halfblock" })
    local autocmds = vim.api.nvim_get_autocmds({ group = "Distract", event = "ColorScheme" })
    assert(#autocmds > 0, "a colourscheme change must be handled")
  end)
end)
