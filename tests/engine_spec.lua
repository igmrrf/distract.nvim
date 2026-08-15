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
  if not ok then error(err, 0) end
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
  if not ok then error(err, 0) end
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

    assert.is_false(engine.is_running(),
      "engine must stop itself once rendering fails repeatedly")
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
    assert.are_equal(1, warnings,
      "a render fault should notify once, not once per frame")
    engine.clear()
  end)

  it("keeps running when rendering recovers before the failure limit", function()
    fresh_engine()
    local original = renderer.draw
    local calls = 0
    renderer.draw = function(...)
      calls = calls + 1
      if calls <= 2 then error("transient failure") end
      return original(...)
    end
    for _ = 1, 6 do pcall(engine.tick) end
    renderer.draw = original

    assert.is_true(engine.is_running(),
      "a transient render error should not stop the engine")
    engine.clear()
  end)
end)
