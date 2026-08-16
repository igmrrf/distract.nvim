require("tests.test_harness")

local backends = require("distract.backends")
local position = require("distract.position")

--- Runs `fn` with the given editor options, then puts them back.
local function with_options(options, fn)
  local saved = {}
  for name, value in pairs(options) do
    saved[name] = vim.o[name]
    vim.o[name] = value
  end
  local ok, err = pcall(fn)
  for name, value in pairs(saved) do
    vim.o[name] = value
  end
  if not ok then
    error(err, 0)
  end
end

--- A placement request with everything but the fields a test cares about.
local function request(overrides)
  return vim.tbl_deep_extend("force", {
    settings = position.settings(nil, nil),
    backend = backends.OVERLAY,
    locomotion = "grounded",
    floor_row = 20,
    sprite_h = 8,
    bounds = { columns = 100, lines = 30 },
    opts = {},
  }, overrides or {})
end

describe("distract.position screen floor", function()
  it("gives back the rows the editor is not already using", function()
    with_options({ cmdheight = 1, laststatus = 2 }, function()
      assert.are_equal(vim.o.lines - 2, position.screen_floor_row())
    end)
  end)

  it("gives back the rows a taller command line leaves", function()
    with_options({ cmdheight = 3, laststatus = 2 }, function()
      assert.are_equal(vim.o.lines - 4, position.screen_floor_row())
    end)
  end)

  it("reclaims the row a hidden statusline gives back", function()
    with_options({ cmdheight = 1, laststatus = 0 }, function()
      assert.are_equal(vim.o.lines - 1, position.screen_floor_row())
    end)
  end)
end)

describe("distract.position text floor", function()
  it("puts the floor on the row the last line starts on", function()
    local buf = vim.api.nvim_create_buf(false, true)
    vim.api.nvim_buf_set_lines(buf, 0, -1, false, { "one", "two", "three" })
    local previous = vim.api.nvim_get_current_buf()
    vim.api.nvim_set_current_buf(buf)

    local row = position.text_floor_row()

    vim.api.nvim_set_current_buf(previous)
    vim.api.nvim_buf_delete(buf, { force = true })

    assert.is_not_nil(row, "three visible lines have an addressable last row")
    assert.are_equal(2, row, "the third line sits on screen row 2, zero-based")
  end)

  it("falls back to the screen floor when the ground is unknown", function()
    -- `floor_row` is the only caller that has to keep working when a line
    -- cannot be mapped, which is the wrapped and folded case.
    assert.are_equal(position.screen_floor_row(), position.floor_row("screen"))
    assert.are_equal("number", type(position.floor_row("text")))
  end)
end)

describe("distract.position parallax", function()
  it("is exactly off until a configuration asks for it", function()
    assert.are_equal(1.0, position.parallax_factor(5, { per_unit = 0.0 }))
    assert.are_equal(1.0, position.parallax_factor(nil, { per_unit = 0.2 }))
    assert.are_equal(1.0, position.parallax_factor(0, { per_unit = 0.2 }))
  end)

  it("scales linearly with depth", function()
    assert.are_equal(1.4, position.parallax_factor(2, { per_unit = 0.2, min = 0.4, max = 1.6 }))
    assert.are_equal(0.6, position.parallax_factor(-2, { per_unit = 0.2, min = 0.4, max = 1.6 }))
  end)

  it("clamps rather than letting a sprite vanish or fill the screen", function()
    assert.are_equal(1.6, position.parallax_factor(100, { per_unit = 0.2, min = 0.4, max = 1.6 }))
    assert.are_equal(0.4, position.parallax_factor(-100, { per_unit = 0.2, min = 0.4, max = 1.6 }))
  end)

  it("collapses to one on a backend that cannot scale a sprite", function()
    local settings = position.settings({ parallax = { per_unit = 0.2 } }, nil)
    backends.reset_warnings()

    local warnings = 0
    local original = vim.notify
    vim.notify = function(_, level)
      if level and level >= vim.log.levels.WARN then
        warnings = warnings + 1
      end
    end
    local halfblock = position.parallax_for(2, settings, backends.HALFBLOCK)
    local halfblock_again = position.parallax_for(3, settings, backends.HALFBLOCK)
    vim.notify = original

    assert.are_equal(1.0, halfblock, "the half-block renderer honours order, not depth")
    assert.are_equal(1.0, halfblock_again)
    assert.are_equal(1, warnings, "a declared degradation is reported once, not per spawn")
    assert.are_equal(1.4, position.parallax_for(2, settings, backends.OVERLAY))
  end)
end)

describe("distract.position anchors", function()
  it("puts what gravity binds on the floor and lets the rest drift", function()
    assert.are_equal(position.BOTTOM, position.effective_anchor(position.AUTO, nil, "grounded"))
    assert.are_equal(position.BOTTOM, position.effective_anchor(position.AUTO, nil, "ballistic"))
    assert.are_equal(
      position.FREE,
      position.effective_anchor(position.AUTO, nil, "omnidirectional")
    )
  end)

  it("leaves an anchor that was asked for alone", function()
    assert.are_equal(position.TOP, position.effective_anchor(position.TOP, nil, "grounded"))
  end)

  it("prefers what the asset declares over what its locomotion implies", function()
    assert.are_equal(
      position.TOP,
      position.effective_anchor(position.AUTO, position.TOP, "omnidirectional")
    )
    assert.are_equal(
      position.FREE,
      position.effective_anchor(position.AUTO, position.AUTO, "omnidirectional")
    )
  end)

  it("lets a spawn override what the asset declares", function()
    assert.are_equal(
      position.BOTTOM,
      position.effective_anchor(position.BOTTOM, position.TOP, "omnidirectional")
    )
  end)

  it("reads the anchor an asset declares, and refuses one it made up", function()
    assert.is_nil(position.manifest_anchor({ name = "probe" }))
    assert.are_equal(position.TOP, position.manifest_anchor({ name = "sun", anchor = "top" }))
    assert.is_false(pcall(position.manifest_anchor, { name = "probe", anchor = "botom" }))
  end)

  it("puts the sun in the sky rather than the middle of the screen", function()
    local sun = require("distract.manifests.sun")
    assert.are_equal(position.TOP, position.manifest_anchor(sun))
  end)
end)

describe("distract.position placement", function()
  it("stands a bottom-anchored spawn on the floor", function()
    local placed = request({ settings = position.settings({ anchor = "bottom" }, nil) })
    local result = position.placement(placed)
    assert.are_equal(12, result.y, "20 - 8 puts the feet on the floor")
    assert.are_equal(12, result.ground_y)
  end)

  it("starts a top-anchored spawn at row zero but keeps its floor", function()
    local result = position.placement(request({
      settings = position.settings({ anchor = "top" }, nil),
    }))
    assert.are_equal(0, result.y)
    assert.are_equal(12, result.ground_y, "the anchor says where it starts, not where it lands")
  end)

  it("centres a free spawn", function()
    local result = position.placement(request({
      settings = position.settings({ anchor = "free" }, nil),
    }))
    assert.are_equal(50, result.x)
    assert.are_equal(15, result.y)
  end)

  it("reads x, y and z out of a table anchor", function()
    local result = position.placement(request({
      settings = position.settings({ anchor = { x = 7, y = 3, z = 2 } }, nil),
    }))
    assert.are_equal(7, result.x)
    assert.are_equal(3, result.y)
    assert.are_equal(2, result.z)
  end)

  it("lets a spawn override the anchor it would have used", function()
    local result = position.placement(request({
      settings = position.settings({ anchor = "bottom" }, nil),
      opts = { x = 4, y = 5 },
    }))
    assert.are_equal(4, result.x)
    assert.are_equal(5, result.y)
  end)

  it("raises the floor for a sprite that depth has shrunk", function()
    local result = position.placement(request({
      settings = position.settings({
        anchor = "bottom",
        parallax = { per_unit = -0.25, min = 0.4, max = 1.6 },
      }, nil),
      opts = { z = 2 },
    }))
    assert.are_equal(0.5, result.parallax)
    assert.are_equal(16, result.ground_y, "a half-height sprite still stands on the same floor")
  end)

  it("centres a bottom anchor when no floor has been measured", function()
    local unmeasured = request({ settings = position.settings({ anchor = "bottom" }, nil) })
    unmeasured.floor_row = nil
    local result = position.placement(unmeasured)
    assert.are_equal(15, result.y, "nothing to stand on means the old centred spawn")
    assert.is_nil(result.ground_y)
  end)
end)
