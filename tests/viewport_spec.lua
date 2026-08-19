require("tests.test_harness")

local placement = require("distract.placement")
local viewport = require("distract.viewport")

--- Opens a scratch float and closes it after `fn`, whatever happens.
local function with_float(config, fn)
  local buf = vim.api.nvim_create_buf(false, true)
  local win = vim.api.nvim_open_win(
    buf,
    false,
    vim.tbl_extend("force", {
      relative = "editor",
      width = 10,
      height = 4,
      row = 2,
      col = 3,
      style = "minimal",
      focusable = false,
      noautocmd = true,
    }, config or {})
  )
  local ok, err = pcall(fn, win)
  pcall(vim.api.nvim_win_close, win, true)
  pcall(vim.api.nvim_buf_delete, buf, { force = true })
  if not ok then
    error(err, 0)
  end
end

describe("distract.viewport configuration", function()
  it("defaults to the editor grid, which is what every release so far used", function()
    viewport.reset()
    assert.are_equal(viewport.EDITOR, viewport.scope())
    local rect = viewport.rect()
    assert.are_equal(0, rect.row)
    assert.are_equal(0, rect.col)
    assert.are_equal(vim.o.columns, rect.width)
    assert.are_equal(vim.o.lines, rect.height)
  end)

  it("refuses a scope it cannot resolve rather than silently using the editor", function()
    viewport.reset()
    local ok, err = pcall(viewport.configure, { scope = "monitor" })
    assert.is_false(ok)
    assert.is_true(tostring(err):find("positioning.scope", 1, true) ~= nil)
    assert.are_equal(viewport.EDITOR, viewport.scope())
  end)

  it("replaces the excluded filetype list rather than merging into it", function()
    viewport.reset()
    viewport.configure({ exclude_filetypes = { "qf" } })
    -- Merging would leave the defaults at the indices a shorter list does not
    -- cover, so 'help' would still be excluded after the user replaced the list.
    viewport.configure({ scope = viewport.EDITOR })
    local blocked_filetypes = nil
    with_float({}, function(win)
      vim.api.nvim_set_option_value("filetype", "help", { buf = vim.api.nvim_win_get_buf(win) })
      -- The float itself blocks by floating, so exclude_floating is turned off
      -- to isolate the filetype rule.
      viewport.configure({ exclude_floating = false })
      blocked_filetypes = #viewport.blocking_rects()
    end)
    assert.are_equal(0, blocked_filetypes, "'help' must not block once the list was replaced")
    viewport.reset()
  end)

  it("stacks sprites below LSP hovers by default", function()
    viewport.reset()
    assert.are_equal(40, viewport.z_index_offset())
    viewport.configure({ z_index_offset = 5 })
    assert.are_equal(5, viewport.z_index_offset())
    viewport.reset()
  end)

  it("reports bounds with an origin, which is what the engines clamp against", function()
    viewport.reset()
    local bounds = viewport.bounds()
    assert.are_equal(vim.o.columns, bounds.columns)
    assert.are_equal(vim.o.lines, bounds.lines)
    assert.are_equal(0, bounds.col)
    assert.are_equal(0, bounds.row)
  end)
end)

describe("distract.viewport scopes", function()
  it("measures the current window for the window scope", function()
    viewport.reset()
    viewport.configure({ scope = viewport.WINDOW })
    local win = vim.api.nvim_get_current_win()
    local rect = viewport.rect()
    assert.are_equal(vim.api.nvim_win_get_width(win), rect.width)
    assert.are_equal(vim.api.nvim_win_get_height(win), rect.height)
    viewport.reset()
  end)

  it("takes the gutter off the buffer scope, so nothing draws over the numbers", function()
    viewport.reset()
    local original = vim.wo.number
    vim.wo.number = true
    vim.wo.numberwidth = 6

    viewport.configure({ scope = viewport.WINDOW })
    local window_rect = viewport.rect()
    viewport.configure({ scope = viewport.BUFFER })
    local buffer_rect = viewport.rect()

    assert.is_true(
      buffer_rect.col > window_rect.col,
      "the buffer scope starts after the gutter, not at the window's edge"
    )
    assert.is_true(buffer_rect.width < window_rect.width)

    vim.wo.number = original
    viewport.reset()
  end)
end)

describe("distract.viewport occlusion", function()
  it("treats an open float as something a sprite must not cover", function()
    viewport.reset()
    with_float({ row = 2, col = 3, width = 10, height = 4 }, function()
      local blocked = viewport.blocking_rects()
      assert.is_true(#blocked > 0)
      assert.is_true(viewport.is_blocked({ row = 3, col = 4, width = 2, height = 2 }, blocked))
      assert.is_false(viewport.is_blocked({ row = 20, col = 40, width = 2, height = 2 }, blocked))
    end)
    viewport.reset()
  end)

  it("ignores the windows the caller says are its own", function()
    viewport.reset()
    with_float({}, function(win)
      assert.is_true(#viewport.blocking_rects() > 0)
      assert.are_equal(0, #viewport.blocking_rects({ [win] = true }))
    end)
    viewport.reset()
  end)

  it("blocks nothing at all in the absolute scope", function()
    viewport.reset()
    viewport.configure({ scope = viewport.ABSOLUTE })
    with_float({}, function()
      assert.are_equal(0, #viewport.blocking_rects())
    end)
    viewport.reset()
  end)

  it("reports no overlap for rects that only touch edges", function()
    local left = { row = 0, col = 0, width = 4, height = 4 }
    local right = { row = 0, col = 4, width = 4, height = 4 }
    assert.is_false(viewport.overlaps(left, right))
    assert.is_true(viewport.overlaps(left, { row = 3, col = 3, width = 4, height = 4 }))
  end)
end)

describe("distract.renderer occlusion", function()
  local engine = require("distract.engine")
  local renderer = require("distract.renderer")

  it("skips the frame for a sprite that would cover a float", function()
    viewport.reset()
    require("distract").setup({ backend = "halfblock" })
    engine.clear()
    engine.spawn("cat", { x = 3, y = 2 })
    local entity = engine.get_entities()[1]

    engine.tick()
    assert.is_not_nil(renderer.window_state(entity.id), "the sprite draws with nothing in the way")

    with_float({ row = 0, col = 0, width = 40, height = 12 }, function()
      engine.tick()
      assert.is_nil(
        renderer.window_state(entity.id),
        "a sprite over a float is worse than no sprite"
      )
    end)

    engine.clear()
    viewport.reset()
  end)
end)

describe("distract.renderer toroidal wrap", function()
  local engine = require("distract.engine")
  local renderer = require("distract.renderer")

  --- A walking cat placed so its footprint straddles the right edge.
  local function cat_at_the_seam()
    viewport.reset()
    require("distract").setup({ backend = "halfblock" })
    engine.clear()
    engine.spawn("cat")
    local entity = engine.get_entities()[1]
    -- `idle` clamps and `walk` wraps, so the state has to be the wrapping one.
    engine.set_entity_state(entity, "walk")
    entity.x = vim.o.columns - 8
    entity.y = 2
    engine.tick()
    return entity
  end

  it("draws the departing half at the opposite edge, in the same frame", function()
    local entity = cat_at_the_seam()
    local state = renderer.window_state(entity.id)

    assert.is_not_nil(state)
    assert.are_equal(2, #state.slices, "a sprite crossing the seam is drawn in two pieces")

    local at_seam, wrapped = state.slices[1], state.slices[2]
    assert.are_equal(vim.o.columns - 8, at_seam.col)
    assert.are_equal(0, at_seam.src_col)
    assert.are_equal(0, wrapped.col)
    assert.is_true(wrapped.src_col > 0, "the wrapped piece shows a later part of the sprite")
    assert.are_equal(at_seam.width + wrapped.width, 24, "between them they draw the whole sprite")

    engine.clear()
    viewport.reset()
  end)

  it("gives back the extra float once the sprite is clear of the seam", function()
    local entity = cat_at_the_seam()
    assert.are_equal(2, #renderer.window_state(entity.id).slices)

    entity.x = 10
    engine.tick()

    local state = renderer.window_state(entity.id)
    assert.are_equal(1, #state.slices)
    assert.are_equal(10, state.col)

    engine.clear()
    viewport.reset()
  end)

  it("never draws onto buffer text while it is sliced", function()
    local entity = cat_at_the_seam()
    assert.are_equal(0, renderer.window_state(entity.id).overlay_limit)
    engine.clear()
    viewport.reset()
  end)
end)

describe("distract.placement", function()
  local BOUNDS = { columns = 40, lines = 20, col = 10, row = 5 }

  it("clamps a surface inside a scoped rectangle", function()
    local geom = placement.resolve({ x = 0, y = 0, width = 8, height = 4, bounds = BOUNDS })
    assert.are_equal(10, geom.col)
    assert.are_equal(5, geom.row)

    local far = placement.resolve({ x = 500, y = 500, width = 8, height = 4, bounds = BOUNDS })
    assert.are_equal(10 + 40 - 8, far.col)
    assert.are_equal(5 + 20 - 4 - 1, far.row)
  end)

  it("keeps a surface larger than the rectangle to a legal size", function()
    local geom = placement.resolve({ x = 0, y = 0, width = 400, height = 400, bounds = BOUNDS })
    assert.are_equal(40, geom.width)
    assert.are_equal(19, geom.height)
  end)

  it("draws a wrapping surface in one piece while it is fully inside", function()
    local geom = placement.resolve({
      x = 20,
      y = 8,
      width = 8,
      height = 4,
      bounds = BOUNDS,
      wrap = true,
    })
    assert.are_equal(1, #geom.slices)
    assert.are_equal(20, geom.slices[1].col)
    assert.are_equal(0, geom.slices[1].src_col)
  end)

  it("draws the departing half of a wrapping surface at the opposite edge", function()
    -- The rectangle spans columns 10..49. At x = 45 the last 4 columns of the
    -- surface are past the right edge and reappear at column 10.
    local geom = placement.resolve({
      x = 45,
      y = 8,
      width = 8,
      height = 4,
      bounds = BOUNDS,
      wrap = true,
    })
    assert.are_equal(2, #geom.slices)

    local visible, wrapped = geom.slices[1], geom.slices[2]
    assert.are_equal(45, visible.col)
    assert.are_equal(5, visible.width)
    assert.are_equal(0, visible.src_col)

    assert.are_equal(10, wrapped.col)
    assert.are_equal(3, wrapped.width)
    assert.are_equal(5, wrapped.src_col)
    assert.are_equal(visible.width + wrapped.width, 8)
  end)

  it("draws a surface still off the near edge as the same two pieces", function()
    -- Columns 10..49, so x = 7 is three columns off the left edge. On a circle
    -- that is the same picture as x = 47: three columns at the far edge, five
    -- wrapped round to the near one.
    local geom = placement.resolve({
      x = 7,
      y = 8,
      width = 8,
      height = 4,
      bounds = BOUNDS,
      wrap = true,
    })
    assert.are_equal(2, #geom.slices)

    local at_far_edge, wrapped = geom.slices[1], geom.slices[2]
    assert.are_equal(47, at_far_edge.col)
    assert.are_equal(3, at_far_edge.width)
    assert.are_equal(0, at_far_edge.src_col)

    assert.are_equal(10, wrapped.col)
    assert.are_equal(5, wrapped.width)
    assert.are_equal(3, wrapped.src_col)

    local same_as_wrapped_position =
      placement.resolve({ x = 47, y = 8, width = 8, height = 4, bounds = BOUNDS, wrap = true })
    assert.are.same(geom.slices, same_as_wrapped_position.slices)
  end)

  it("draws four slices for a surface leaving a corner", function()
    -- Rows 5..23, columns 10..49. Off the right edge and off the bottom at once.
    local geom = placement.resolve({
      x = 46,
      y = 21,
      width = 8,
      height = 4,
      bounds = BOUNDS,
      wrap = true,
    })
    assert.are_equal(4, #geom.slices)

    local covered = 0
    local corners = {}
    for _, slice in ipairs(geom.slices) do
      covered = covered + slice.width * slice.height
      table.insert(corners, string.format("%d,%d", slice.row, slice.col))
    end
    -- Every one of the surface's cells is drawn exactly once, somewhere.
    assert.are_equal(8 * 4, covered)
    assert.are.same({ "21,46", "21,10", "5,46", "5,10" }, corners)
  end)

  it("never draws onto buffer text while a surface is sliced", function()
    local geom = placement.resolve({
      x = 46,
      y = 21,
      width = 8,
      height = 4,
      bounds = BOUNDS,
      wrap = true,
    })
    assert.are_equal(0, geom.overlay_limit)
  end)

  it("reports no slice for a rectangle with no room in it", function()
    local geom = placement.resolve({
      x = 0,
      y = 0,
      width = 8,
      height = 4,
      bounds = { columns = 0, lines = 1, col = 0, row = 0 },
      wrap = true,
    })
    assert.are_equal(0, #geom.slices)
  end)

  it("splits the surface between buffer text and a float", function()
    local geom = placement.resolve({ x = 12, y = 6, width = 8, height = 4, bounds = BOUNDS })
    assert.are_equal(geom.height, geom.overlay_limit + geom.float_height)
    assert.are_equal(geom.row + geom.overlay_limit, geom.float_row)
  end)
end)
