-- What the 3D render mode costs in the in-terminal engine.
--
-- The claim the design rests on is that a voxel pet costs a table lookup per draw
-- once its frames are rasterised, and that the rasterising itself is bounded. This
-- measures both, because "measure rather than assume" is what `tick_budget.rs` and
-- `bench_tick.lua` already established for the particle question.
--
--   nvim --headless --noplugin -u tests/minimal_init.lua -l tools/bench_render3d.lua
--   nvim --headless --noplugin -u tests/minimal_init.lua -l tools/bench_render3d.lua 200 gudong

local distract = require("distract")
local engine = require("distract.engine")
local raster3d = require("distract.raster3d")
local render = require("distract.render")
local renderer = require("distract.renderer")
local sprites = require("distract.terminal_sprites")

local uv = vim.uv or vim.loop

local ENTITIES = tonumber(arg and arg[1]) or 200
local ASSET = (arg and arg[2]) or "cat"
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

local function report(label, value, budget)
  if budget then
    print(
      string.format(
        "  %-22s%8.3f ms (%.1f%% of a 30 FPS frame)",
        label,
        value,
        value / budget * 100
      )
    )
  else
    print(string.format("  %-22s%8.3f ms", label, value))
  end
end

--- Rasterises every frame of an asset from cold, which is what a session pays the
--- first time each pose is seen.
local function measure_meshing()
  sprites.configure_render(render.settings({ mode = "3d" }))
  quietly(function()
    sprites.bind_manifest(ASSET, distract.config.assets[ASSET])
  end)
  local frame_count = #sprites.get_pixel_frames(ASSET, { native_resolution = false })

  raster3d.reset()
  local cold_ms = milliseconds(function()
    quietly(function()
      raster3d.matrix(ASSET, 1, false)
    end)
  end)

  raster3d.reset()
  local all_ms = milliseconds(function()
    quietly(function()
      for frame = 1, frame_count do
        raster3d.matrix(ASSET, frame, false)
      end
    end)
  end)

  local warm_ms = milliseconds(function()
    for _ = 1, 1000 do
      raster3d.matrix(ASSET, 1, false)
    end
  end) / 1000

  print(
    string.format(
      "%s: %d frames, model fitted to %d wide",
      ASSET,
      frame_count,
      sprites.get_dimensions(ASSET)
    )
  )
  report("first frame, cold", cold_ms)
  report("every frame, cold", all_ms)
  report("cached frame", warm_ms)
  print("")
  return frame_count
end

--- Steady-state step and draw at scale, in one mode.
local function measure_mode(mode)
  sprites.configure_render(render.settings({ mode = mode }))
  engine.clear()
  quietly(function()
    for index = 1, ENTITIES do
      engine.spawn(ASSET, { x = (index % 40) * 2, y = 2 + (index % 6) })
    end
    -- Walking, not idling: the renderer's guard skips an entity whose picture and
    -- placement have not changed, which is right behaviour and the wrong
    -- measurement.
    for _, entity in ipairs(engine.get_entities()) do
      engine.set_entity_state(entity, "walk")
    end
  end)

  local bounds = { columns = vim.o.columns, lines = vim.o.lines }
  -- One untimed pass: the first step resolves every asset and the first draw
  -- rasterises and buffers every frame.
  quietly(function()
    engine.step(1 / 30, bounds)
    renderer.draw(engine.get_entities(), "halfblock")
  end)

  local frame_ms = milliseconds(function()
    quietly(function()
      for _ = 1, TICKS do
        engine.step(1 / 30, bounds)
        renderer.draw(engine.get_entities(), "halfblock")
      end
    end)
  end) / TICKS

  local idle_ms = milliseconds(function()
    quietly(function()
      for _ = 1, TICKS do
        renderer.draw(engine.get_entities(), "halfblock")
      end
    end)
  end) / TICKS

  print(string.format("%s, %d entities", mode, #engine.get_entities()))
  report("step + draw", frame_ms, FRAME_BUDGET_MS)
  report("idle redraw", idle_ms, FRAME_BUDGET_MS)
  print("")
  engine.clear()
end

distract.setup({ backend = "halfblock" })
measure_meshing()
measure_mode("2d")
measure_mode("3d")

vim.cmd("qall!")
