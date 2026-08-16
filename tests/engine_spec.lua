require("tests.test_harness")

local engine = require("distract.engine")
local renderer = require("distract.renderer")

--- Replaces renderer.draw for the duration of `fn`, restoring it afterwards.
local function with_broken_renderer(fn)
  local original = renderer.draw
  local calls = 0
  renderer.draw = function()
    calls = calls + 1
    error("simulated render failure")
  end
  local ok, err = pcall(fn)
  renderer.draw = original
  if not ok then
    error(err, 0)
  end
  return calls
end

--- Counts vim.notify calls at or above WARN while `fn` runs.
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

--- Fresh halfblock engine with a single cat spawned and running.
local function fresh_engine()
  require("distract").setup({ backend = "halfblock" })
  engine.clear()
  engine.spawn("cat")
end

describe("distract.engine render fault tolerance", function()
  it("does not propagate a renderer error out of tick", function()
    fresh_engine()
    with_broken_renderer(function()
      local ok, err = pcall(engine.tick)
      assert(ok, string.format("tick propagated renderer error: %s", tostring(err)))
    end)
    engine.clear()
  end)

  it("stops the engine after repeated render failures instead of looping forever", function()
    fresh_engine()
    assert.is_true(engine.is_running(), "engine should be running after spawn")

    with_broken_renderer(function()
      count_warnings(function()
        for _ = 1, 20 do
          pcall(engine.tick)
        end
      end)
    end)

    assert.is_false(engine.is_running(), "engine must stop itself once rendering fails repeatedly")
    engine.clear()
  end)

  it("reports the render failure to the user exactly once", function()
    fresh_engine()
    local warnings
    with_broken_renderer(function()
      warnings = count_warnings(function()
        for _ = 1, 20 do
          pcall(engine.tick)
        end
      end)
    end)
    assert.are_equal(1, warnings, "a render fault should notify once, not once per frame")
    engine.clear()
  end)

  it("keeps running when rendering recovers before the failure limit", function()
    fresh_engine()
    local original = renderer.draw
    local calls = 0
    renderer.draw = function(...)
      calls = calls + 1
      if calls <= 2 then
        error("transient failure")
      end
      return original(...)
    end
    for _ = 1, 6 do
      pcall(engine.tick)
    end
    renderer.draw = original

    assert.is_true(engine.is_running(), "a transient render error should not stop the engine")
    engine.clear()
  end)
end)

describe("distract.engine parity with the overlay", function()
  local engine = require("distract.engine")

  -- Each test registers its manifest under its own name. `engine.setup` merges
  -- config with `tbl_deep_extend("force", ...)`, so reusing one name would let
  -- a previous test's physics fields survive into the next one.
  local probe_counter = 0

  --- Spawns one entity under a manifest built for the test and returns it.
  local function only_entity(manifest, state, opts)
    engine.clear()
    probe_counter = probe_counter + 1
    local name = "probe_" .. probe_counter
    manifest.name = name
    engine.setup({ backend = "halfblock", assets = { [name] = manifest } })
    local orig = vim.notify
    vim.notify = function() end
    engine.spawn(name, opts or {})
    vim.notify = orig
    local entities = engine.get_entities()
    local e = entities[#entities]
    e.current_state = state
    return e
  end

  local function manifest_with(physics, transitions)
    return {
      initial_state = "run",
      states = {
        run = {
          animation = { frames = { 0 }, fps = 1.0, loop_anim = true },
          physics = physics,
          transitions = transitions,
        },
        parked = {
          animation = { frames = { 0 }, fps = 1.0, loop_anim = true },
          physics = { target_vx = 0.0 },
        },
      },
    }
  end

  it("wraps vertically, as the overlay always has", function()
    local m = manifest_with({ target_vx = 0.0, wrap_mode = "wrap" })
    local e = only_entity(m, "run")
    e.y = vim.o.lines + 50
    e.vy = 0
    engine.tick()
    assert(e.y < 0, string.format("entity below the screen did not wrap, y=%.1f", e.y))

    e.y = -500
    engine.tick()
    assert(e.y > 0, string.format("entity above the screen did not wrap, y=%.1f", e.y))
    engine.clear()
  end)

  it("bounces off the top and bottom, not just the sides", function()
    local m = manifest_with({ target_vx = 0.0, wrap_mode = "bounce" })
    local e = only_entity(m, "run")
    e.y = -5
    e.vy = -3
    engine.tick()
    assert(e.vy > 0, string.format("vertical bounce did not reverse vy, got %.2f", e.vy))
    engine.clear()
  end)

  it("fires on_edge_right when it turns around at the right edge", function()
    local m = manifest_with({ target_vx = 4.0, wrap_mode = "bounce" }, {
      on_edge_right = "parked",
    })
    local e = only_entity(m, "run")
    e.x = vim.o.columns
    e.heading_x = 1
    e.vx = 4
    engine.tick()
    assert.are_equal(
      "parked",
      e.current_state,
      "hitting the right edge must fire the manifest's on_edge_right"
    )
    engine.clear()
  end)

  it("integrates accel_x instead of ignoring it", function()
    local m = manifest_with({ target_vx = 0.0, accel_x = 0.5, wrap_mode = "none", friction = 0.05 })
    local e = only_entity(m, "run")
    e.vx = 0
    e.x = 10
    for _ = 1, 20 do
      engine.tick()
    end
    -- `target_vx` is zero, so friction alone can only hold vx at exactly zero.
    -- Any velocity at all is acceleration being integrated. The magnitude is
    -- wall-clock dependent -- these ticks are microseconds apart -- so it is
    -- deliberately not asserted.
    assert(e.vx > 0, string.format("accel_x built no velocity, vx=%.4f", e.vx))
    assert(e.x > 10, string.format("accel_x moved nothing, x=%.3f", e.x))
    engine.clear()
  end)

  it("holds an entity still when accel_x is absent", function()
    local m = manifest_with({ target_vx = 0.0, wrap_mode = "none", friction = 0.05 })
    local e = only_entity(m, "run")
    e.vx = 0
    e.x = 10
    for _ = 1, 20 do
      engine.tick()
    end
    assert.are_equal(0, e.vx, "no accel and no target means no velocity")
    engine.clear()
  end)

  it("integrates accel_y for an entity with no gravity", function()
    local m = manifest_with({ target_vx = 0.0, accel_y = -0.4, wrap_mode = "none" })
    local e = only_entity(m, "run")
    local start_y = e.y
    for _ = 1, 20 do
      engine.tick()
    end
    assert(
      e.y < start_y,
      string.format("accel_y did not lift the entity, %.1f -> %.1f", start_y, e.y)
    )
    engine.clear()
  end)
end)
