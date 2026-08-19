require("tests.test_harness")

local engine = require("distract.engine")
local events = require("distract.events")
local renderer = require("distract.renderer")
local visibility = require("distract.visibility")

--- Fresh in-terminal engine with one cat, focused, and nothing hidden.
local function fresh_world()
  visibility.reset()
  require("distract").setup({ backend = "halfblock" })
  engine.clear()
  engine.spawn("cat")
  return engine.get_entities()[1]
end

--- Counts `renderer.draw` calls while `fn` runs.
local function count_draws(fn)
  local original = renderer.draw
  local draws = 0
  renderer.draw = function(...)
    draws = draws + 1
    return original(...)
  end
  local ok, err = pcall(fn)
  renderer.draw = original
  if not ok then
    error(err, 0)
  end
  return draws
end

describe("distract.visibility", function()
  it("is restricted to this instance by default", function()
    visibility.reset()
    assert.is_true(visibility.is_restricted_to_instance())
    assert.is_true(visibility.is_visible())
  end)

  it("hides on focus loss and shows again on focus gain", function()
    visibility.reset()
    assert.is_true(visibility.set_focus(false))
    assert.is_false(visibility.is_visible())
    assert.is_true(visibility.set_focus(true))
    assert.is_true(visibility.is_visible())
  end)

  it("reports no change when focus arrives twice", function()
    visibility.reset()
    visibility.set_focus(false)
    assert.is_false(visibility.set_focus(false))
  end)

  it("keeps drawing regardless when the restriction is turned off", function()
    visibility.reset()
    visibility.configure({ restrict_to_instance = false })
    -- Nothing to change: an unrestricted instance was already going to draw.
    assert.is_false(visibility.set_focus(false))
    assert.is_true(visibility.is_visible())
    assert.is_false(visibility.set_focus(true))
  end)

  it("leaves the restriction alone when setup says nothing about it", function()
    visibility.reset()
    visibility.configure({ fps = 30 })
    assert.is_true(visibility.is_restricted_to_instance())
  end)
end)

describe("distract.engine visibility", function()
  it("keeps simulating while hidden", function()
    local entity = fresh_world()
    entity.manifest.states[entity.current_state].physics.target_vx = 4
    local started_at = entity.x

    visibility.set_focus(false)
    engine.set_visible(false)
    for _ = 1, 5 do
      engine.step(0.05, { columns = 80, lines = 24 })
    end

    assert.is_true(
      math.abs(entity.x - started_at) > 0,
      "a hidden entity must keep moving, or a wrap in progress is stranded"
    )
    visibility.reset()
    engine.clear()
  end)

  it("does not draw while hidden, and draws again once shown", function()
    fresh_world()
    visibility.set_focus(false)

    local hidden_draws = count_draws(function()
      engine.tick()
      engine.tick()
    end)
    assert.are_equal(0, hidden_draws)

    visibility.set_focus(true)
    engine.set_visible(true)
    local shown_draws = count_draws(function()
      engine.tick()
    end)
    assert.is_true(shown_draws > 0, "a shown engine must repaint")

    visibility.reset()
    engine.clear()
  end)

  it("closes the surfaces when hidden, so nothing stays over another app", function()
    local entity = fresh_world()
    engine.tick()
    assert.is_not_nil(renderer.window_state(entity.id))

    engine.set_visible(false)
    assert.is_nil(renderer.window_state(entity.id))

    visibility.reset()
    engine.clear()
  end)
end)

describe("distract.events focus routing", function()
  it("routes a focus change to both backends without a running overlay", function()
    fresh_world()
    assert.has_no.errors(function()
      events.set_focus(false)
      events.set_focus(true)
    end)
    visibility.reset()
    engine.clear()
  end)

  it("does nothing when the focus change would not change what is drawn", function()
    fresh_world()
    visibility.configure({ restrict_to_instance = false })
    local draws = count_draws(function()
      events.set_focus(false)
      engine.tick()
    end)
    assert.is_true(draws > 0, "an unrestricted instance keeps drawing after focus loss")
    visibility.reset()
    engine.clear()
  end)
end)
