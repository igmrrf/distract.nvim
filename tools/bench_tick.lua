-- What one tick costs in the in-terminal engine, at the scale a particle system
-- would want.
--
-- `docs/ecosystem-roadmap.md` §2.5 gates ambient weather on this: the engine was built for three
-- entities and rain wants hundreds. `engine/tests/tick_budget.rs` answers it for
-- the overlay; this answers it for the backend most people actually run, where
-- stepping and drawing have very different costs — the step is arithmetic, the
-- draw is Neovim API calls.
--
--   nvim --headless --noplugin -u tests/minimal_init.lua -l tools/bench_tick.lua
--   nvim --headless --noplugin -u tests/minimal_init.lua -l tools/bench_tick.lua 500

local engine = require("distract.engine")
local renderer = require("distract.renderer")

local uv = vim.uv or vim.loop

local ENTITIES = tonumber(arg and arg[1]) or 200
local TICKS = 120
local FRAME_BUDGET_MS = 1000 / 30

local function quietly(fn)
  local notify = vim.notify
  vim.notify = function() end
  local ok, err = pcall(fn)
  vim.notify = notify
  if not ok then
    error(err, 0)
  end
  return ok
end

local function milliseconds(fn)
  local started = uv.hrtime()
  fn()
  return (uv.hrtime() - started) / 1e6
end

require("distract").setup({ backend = "halfblock" })
engine.clear()

quietly(function()
  for index = 1, ENTITIES do
    engine.spawn("cat", { x = (index % 40) * 2, y = 2 + (index % 6) })
  end
  -- Walking, not idling. The cat's `idle` state has no velocity, so an idle world
  -- never moves a sprite far enough to change its cell and the renderer's guard
  -- skips every entity -- which is the right behaviour and the wrong measurement.
  for _, entity in ipairs(engine.get_entities()) do
    engine.set_entity_state(entity, "walk")
  end
end)

local bounds = { columns = vim.o.columns, lines = vim.o.lines }

-- One untimed pass first: the first step resolves every asset and the first draw
-- builds every frame buffer, and attributing that to the steady-state cost would
-- answer a different question.
quietly(function()
  engine.step(1 / 30, bounds)
  renderer.draw(engine.get_entities(), "halfblock")
end)

local step_ms = milliseconds(function()
  quietly(function()
    for _ = 1, TICKS do
      engine.step(1 / 30, bounds)
    end
  end)
end) / TICKS

-- Stepped and drawn together, which is the only honest per-frame number: the
-- renderer skips an entity whose picture and placement have not changed, so a
-- loop of draws with no step between them measures the redraw guard rather than
-- the draw.
local frame_ms = milliseconds(function()
  quietly(function()
    for _ = 1, TICKS do
      engine.step(1 / 30, bounds)
      renderer.draw(engine.get_entities(), "halfblock")
    end
  end)
end) / TICKS

-- What a world that has settled costs, which is what the guard exists for.
local idle_ms = milliseconds(function()
  quietly(function()
    for _ = 1, TICKS do
      renderer.draw(engine.get_entities(), "halfblock")
    end
  end)
end) / TICKS

local function report(label, value)
  print(
    string.format(
      "%-16s%.3f ms (%.1f%% of a 30 FPS frame)",
      label,
      value,
      value / FRAME_BUDGET_MS * 100
    )
  )
end

print(string.format("%-16s%d", "entities", #engine.get_entities()))
report("step", step_ms)
report("step + draw", frame_ms)
report("idle redraw", idle_ms)

engine.clear()
vim.cmd("qall!")
