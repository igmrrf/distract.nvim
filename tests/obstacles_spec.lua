require("tests.test_harness")

local engine = require("distract.engine")
local events = require("distract.events")
local obstacles = require("distract.obstacles")
local plugins = require("distract.plugins")

local function platform(x, y, width)
  return { x = x, y = y, width = width, height = 1, type = obstacles.SOLID_PLATFORM }
end

local function hazard(x, y, width, height)
  return { x = x, y = y, width = width, height = height, type = obstacles.HAZARD }
end

local function footprint(left, top)
  return { left = left, top = top, width = 20, height = 10 }
end

--- Counts `vim.notify` calls at or above WARN while `fn` runs.
local function count_warnings(fn)
  local original = vim.notify
  local warnings = 0
  vim.notify = function(_, level)
    if level and level >= vim.log.levels.WARN then
      warnings = warnings + 1
    end
  end
  local ok, err = pcall(fn)
  vim.notify = original
  if not ok then
    error(err, 0)
  end
  return warnings
end

describe("distract.obstacles providers", function()
  it("refuses a provider that is not callable", function()
    obstacles.reset()
    assert.is_false(pcall(obstacles.register_provider, "not a function"))
  end)

  it("collects from every registered provider", function()
    obstacles.reset()
    obstacles.register_provider(function()
      return { platform(0, 10, 20) }
    end)
    obstacles.register_provider(function()
      return { hazard(30, 10, 2, 4) }
    end)

    local rects = obstacles.collect()
    assert.are_equal(2, #rects)
    assert.are_equal(obstacles.SOLID_PLATFORM, rects[1].type)
    assert.are_equal(obstacles.HAZARD, rects[2].type)
    obstacles.reset()
  end)

  it("passes the current window and buffer to the provider", function()
    obstacles.reset()
    local seen = nil
    obstacles.register_provider(function(win_id, buf_id)
      seen = { win = win_id, buf = buf_id }
      return {}
    end)
    obstacles.collect()

    assert.is_not_nil(seen)
    assert.are_equal(vim.api.nvim_get_current_win(), seen.win)
    assert.are_equal(vim.api.nvim_get_current_buf(), seen.buf)
    obstacles.reset()
  end)

  it("skips a provider that raises and keeps the others", function()
    obstacles.reset()
    obstacles.register_provider(function()
      error("query failed")
    end)
    obstacles.register_provider(function()
      return { platform(0, 10, 20) }
    end)

    local rects
    local warnings = count_warnings(function()
      rects = obstacles.collect()
    end)

    assert.are_equal(1, warnings)
    assert.are_equal(1, #rects, "one broken provider must not remove everyone else's ground")
    obstacles.reset()
  end)

  it("refuses a malformed rectangle rather than letting it reach the physics", function()
    obstacles.reset()
    obstacles.register_provider(function()
      return {
        { x = 0, y = 10, width = 20, type = obstacles.SOLID_PLATFORM },
        { x = 0, y = 10, width = 0, height = 1, type = obstacles.SOLID_PLATFORM },
        { x = 0, y = 10, width = 20, height = 1, type = "trampoline" },
        platform(0, 10, 20),
      }
    end)

    local rects
    local warnings = count_warnings(function()
      rects = obstacles.collect()
    end)

    assert.are_equal(3, warnings)
    assert.are_equal(1, #rects)
    obstacles.reset()
  end)

  it("caps the list and says so once", function()
    obstacles.reset()
    obstacles.register_provider(function()
      local many = {}
      for index = 1, 200 do
        table.insert(many, platform(index, 10, 2))
      end
      return many
    end)

    local first, second
    local warnings = count_warnings(function()
      first = #obstacles.collect()
      second = #obstacles.collect()
    end)

    assert.are_equal(128, first)
    assert.are_equal(128, second)
    assert.are_equal(1, warnings, "the cap is reported once, not on every collection")
    obstacles.reset()
  end)

  it("unregisters by id", function()
    obstacles.reset()
    local id = obstacles.register_provider(function()
      return { platform(0, 10, 20) }
    end)
    assert.are_equal(1, obstacles.provider_count())
    assert.is_true(obstacles.unregister_provider(id))
    assert.are_equal(0, obstacles.provider_count())
    assert.is_false(obstacles.unregister_provider(id))
    obstacles.reset()
  end)
end)

describe("distract.obstacles geometry", function()
  it("catches a falling entity on the platform it crossed", function()
    local rects = { platform(0, 100, 200) }
    assert.are_equal(100, obstacles.crossed_platform(rects, footprint(10, 95), 85))
  end)

  it("keeps catching an entity already resting on a platform", function()
    local rects = { platform(0, 100, 200) }
    assert.are_equal(100, obstacles.crossed_platform(rects, footprint(10, 90.5), 100))
  end)

  it("lets an entity through a platform from below", function()
    local rects = { platform(0, 100, 200) }
    assert.is_nil(obstacles.crossed_platform(rects, footprint(10, 80), 105))
  end)

  it("ignores a platform the entity is not over", function()
    local rects = { platform(500, 100, 200) }
    assert.is_nil(obstacles.crossed_platform(rects, footprint(10, 95), 85))
  end)

  it("lands on the first platform reached when several are crossed at once", function()
    local rects = { platform(0, 160, 200), platform(0, 120, 200), platform(0, 200, 200) }
    assert.are_equal(120, obstacles.crossed_platform(rects, footprint(10, 250), 100))
  end)

  it("stands a grounded entity on the highest platform under its feet", function()
    local rects = { platform(0, 300, 400), platform(0, 250, 400) }
    assert.are_equal(250, obstacles.standing_surface(rects, footprint(10, 240), 400))
  end)

  it("returns a grounded entity to the floor past the end of a platform", function()
    local rects = { platform(0, 250, 100) }
    assert.are_equal(400, obstacles.standing_surface(rects, footprint(300, 240), 400))
  end)

  it("treats a platform above the entity's head as scenery", function()
    local rects = { platform(0, 50, 400) }
    assert.are_equal(400, obstacles.standing_surface(rects, footprint(10, 240), 400))
  end)

  it("returns an entity the way it came from a hazard", function()
    local rects = { hazard(100, 0, 20, 400) }
    local from_left = obstacles.deflection(rects, footprint(95, 100), 1)
    assert.are_equal(80, from_left.x)
    assert.are_equal(-1, from_left.heading_x)

    local from_right = obstacles.deflection(rects, footprint(105, 100), -1)
    assert.are_equal(120, from_right.x)
    assert.are_equal(1, from_right.heading_x)
  end)

  it("deflects nothing the entity is not touching", function()
    local rects = { hazard(100, 0, 20, 40) }
    assert.is_nil(obstacles.deflection(rects, footprint(95, 300), 1))
    assert.is_nil(obstacles.deflection(rects, footprint(400, 10), 1))
  end)

  it("never lets a platform deflect or a hazard support", function()
    assert.is_nil(obstacles.deflection({ platform(100, 100, 20) }, footprint(95, 95), 1))
    assert.are_equal(
      400,
      obstacles.standing_surface({ hazard(100, 100, 20, 20) }, footprint(95, 50), 400)
    )
  end)
end)

describe("distract.obstacles in the running engine", function()
  it("reports an obstacle collision to a plugin", function()
    obstacles.reset()
    plugins.reset()
    require("distract").setup({ backend = "halfblock" })
    engine.clear()
    engine.spawn("cat")
    local entity = engine.get_entities()[1]

    local edges = {}
    plugins.register("bumper", {
      on_collision = function(_, collision)
        table.insert(edges, collision.edge)
      end,
    })

    -- A hazard exactly where the cat is standing.
    engine.set_obstacles({ hazard(entity.x + 2, entity.y - 1, 3, 20) })
    engine.step(0.05, { columns = 120, lines = 40 })

    assert.are.same({ "obstacle" }, edges)
    engine.clear()
    engine.set_obstacles({})
    plugins.reset()
    obstacles.reset()
  end)

  it("does nothing at all when no provider is registered", function()
    obstacles.reset()
    assert.has_no.errors(function()
      events.sync_obstacles()
    end)
    assert.are_equal(0, #obstacles.rects())
  end)

  it("pushes what it collected into the engine", function()
    obstacles.reset()
    require("distract").setup({ backend = "halfblock" })
    obstacles.register_provider(function()
      return { platform(0, 20, 40) }
    end)

    events.sync_obstacles()
    assert.are_equal(1, #obstacles.rects())

    obstacles.reset()
    engine.set_obstacles({})
  end)
end)
