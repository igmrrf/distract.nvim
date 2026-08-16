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

-- `tick` derives `dt` from `uv.hrtime()` and reads `vim.o.columns`/`lines`
-- inline, so a test could only ever assert on direction, never on distance.
-- `step` is the same simulation with both injected, which is what makes the
-- cross-engine parity goldens possible.
describe("distract.engine deterministic step", function()
  local probe_counter = 0

  local function only_entity(physics, opts)
    engine.clear()
    probe_counter = probe_counter + 1
    local name = "stepprobe_" .. probe_counter
    engine.setup({
      backend = "halfblock",
      assets = {
        [name] = {
          name = name,
          initial_state = "run",
          states = {
            run = {
              animation = { frames = { 0 }, fps = 1.0, loop_anim = true },
              physics = physics,
            },
          },
        },
      },
    })
    local orig = vim.notify
    vim.notify = function() end
    engine.spawn(name, opts or {})
    vim.notify = orig
    local entities = engine.get_entities()
    return entities[#entities]
  end

  it("advances an entity by an exact distance for an injected dt", function()
    -- vx is seeded to target_vx at spawn and lerps toward the same value, so
    -- displacement is vx * (dt * 60) * 1.0 cells = 1.0 * 30 * 1.0 = 30.
    local e = only_entity(
      { target_vx = 1.0, friction = 5.0, gravity = 0.0, wrap_mode = "none" },
      { x = 40, y = 10 }
    )
    engine.step(0.5, { columns = 200, lines = 50 })
    assert(
      math.abs(e.x - 70) < 1e-3,
      string.format("expected x to land on exactly 70, got %.6f", e.x)
    )
    engine.clear()
  end)

  it("reports a still, single-frame, pathless entity as quiescent", function()
    local e = only_entity(
      { target_vx = 0.0, gravity = 0.0, wrap_mode = "none" },
      { x = 40, y = 10 }
    )
    e.vx, e.vy = 0, 0
    assert.is_true(engine.is_quiescent(), "a motionless entity has nothing left to draw")
    engine.clear()
  end)

  it("is not quiescent while an entity is still moving", function()
    local e = only_entity(
      { target_vx = 1.0, friction = 5.0, gravity = 0.0, wrap_mode = "none" },
      { x = 40, y = 10 }
    )
    assert(math.abs(e.vx) > 0.001, "fixture should be moving")
    assert.is_false(engine.is_quiescent(), "a moving entity still needs redrawing")
    engine.clear()
  end)

  it("stops redrawing once every entity has settled", function()
    local e = only_entity(
      { target_vx = 0.0, gravity = 0.0, wrap_mode = "none" },
      { x = 40, y = 10 }
    )
    e.vx, e.vy = 0, 0

    local original = renderer.draw
    local draws = 0
    renderer.draw = function(...)
      draws = draws + 1
      return original(...)
    end
    for _ = 1, 10 do
      engine.tick()
    end
    renderer.draw = original

    -- One final frame so the settled pose is actually on screen, then nothing.
    assert.are_equal(1, draws, "a screen of sleeping entities must not redraw every tick")
    engine.clear()
  end)

  it("takes screen bounds from its argument rather than from vim.o", function()
    -- 45 + 30 = 75, past the injected 50-column screen but inside the 80 a
    -- headless editor reports, so only a step that honours `bounds` wraps.
    local e = only_entity(
      { target_vx = 1.0, friction = 5.0, gravity = 0.0, wrap_mode = "wrap" },
      { x = 45, y = 10 }
    )
    engine.step(0.5, { columns = 50, lines = 50 })
    assert(
      e.x < 0,
      string.format("entity should have wrapped off the injected 50-column screen, x = %.2f", e.x)
    )
    engine.clear()
  end)
end)

-- `path_type` and `locomotion` are open-ended strings, so a manifest could ask
-- for a grounded orbit -- which both engines silently skip, because a path that
-- writes x fights a floor. Declared capabilities turn that into a message.
describe("distract.engine capability gating", function()
  local probe_counter = 0

  --- Registers `manifest` under a fresh name and returns spawn's result.
  ---
  --- Reports the ERROR-level notifications alongside it: a refusal that says
  --- nothing is barely better than the silence it replaced.
  local function try_spawn(manifest)
    engine.clear()
    probe_counter = probe_counter + 1
    local name = "capprobe_" .. probe_counter
    manifest.name = name
    engine.setup({ backend = "halfblock", assets = { [name] = manifest } })

    local original = vim.notify
    local errors = {}
    vim.notify = function(message, level)
      if level and level >= vim.log.levels.ERROR then
        errors[#errors + 1] = message
      end
    end
    local id = engine.spawn(name)
    vim.notify = original
    return id, errors
  end

  local function one_state(physics, extra)
    local manifest = {
      initial_state = "only",
      states = {
        only = {
          animation = { frames = { 0 }, fps = 1.0, loop_anim = true },
          physics = physics,
          transitions = {},
        },
      },
    }
    for key, value in pairs(extra or {}) do
      manifest[key] = value
    end
    return manifest
  end

  it("refuses a state that breaks the asset's declared locomotion", function()
    local id, errors = try_spawn(one_state({ locomotion = "ballistic", gravity = 0.3 }, {
      capabilities = { locomotion = { "grounded" } },
    }))
    assert.is_nil(id, "a violating manifest must not produce an entity")
    assert.are_equal(0, #engine.get_entities(), "a refused spawn must leave nothing behind")
    assert(#errors > 0, "the refusal must be reported, not silent")
    assert(
      errors[1]:find("only") and errors[1]:find("ballistic"),
      "the message must name the offending state and class, got: " .. tostring(errors[1])
    )
    engine.clear()
  end)

  it("refuses an exotic path on a grounded state", function()
    local id = try_spawn(one_state({ path_type = "orbital" }, { locomotion = "grounded" }))
    assert.is_nil(id, "a grounded orbit cannot be drawn, so it must not spawn")
    engine.clear()
  end)

  it("allows sine and linear paths on the ground", function()
    for _, path in ipairs({ "linear", "sine" }) do
      local id = try_spawn(one_state({ path_type = path }, { locomotion = "grounded" }))
      assert.is_not_nil(id, path .. " moves y at most, so it does not need omnidirectional")
      engine.clear()
    end
  end)

  it("refuses omnidirectional locomotion under gravity", function()
    local id = try_spawn(one_state({ locomotion = "omnidirectional", gravity = 0.4 }))
    assert.is_nil(id, "gravity brings a floor, so the state would clamp to one it denies")
    engine.clear()
  end)

  it("refuses an unknown locomotion name", function()
    local id = try_spawn(one_state({ locomotion = "hovering" }))
    assert.is_nil(id)
    engine.clear()
  end)

  it("still spawns a manifest that declares no capabilities", function()
    local id = try_spawn(one_state({ locomotion = "omnidirectional", path_type = "orbital" }))
    assert.is_not_nil(id, "an undeclared manifest must keep loading as it always has")
    engine.clear()
  end)

  it("lets every built-in satisfy the capabilities it declares", function()
    for _, name in ipairs({ "cat", "crab", "sun" }) do
      local manifest = require("distract.manifests." .. name)
      assert.is_not_nil(
        manifest.capabilities and manifest.capabilities.locomotion,
        name .. " should declare what it can do, or the gate proves nothing"
      )
      assert.is_nil(engine.validate_capabilities(manifest), name .. " violates its own declaration")
    end
  end)
end)

-- The cat's jump returns through the animation's `on_finish`, so today it lands
-- when the art happens to run out rather than when it reaches the ground.
describe("distract.engine locomotion", function()
  local probe_counter = 0

  --- Spawns one entity running `physics` with `transitions`, and returns it.
  local function jumper(physics, transitions)
    engine.clear()
    probe_counter = probe_counter + 1
    local name = "jumper_" .. probe_counter
    engine.setup({
      backend = "halfblock",
      assets = {
        [name] = {
          name = name,
          initial_state = "flying",
          states = {
            flying = {
              animation = { frames = { 0 }, fps = 1.0, loop_anim = true },
              physics = physics,
              transitions = transitions,
            },
            landed = {
              animation = { frames = { 0 }, fps = 1.0, loop_anim = true },
              physics = { target_vx = 0.0, wrap_mode = "none" },
              transitions = {},
            },
          },
        },
      },
    })
    local orig = vim.notify
    vim.notify = function() end
    engine.spawn(name, { x = 100, y = 20 })
    vim.notify = orig
    local all = engine.get_entities()
    return all[#all]
  end

  --- Physics that falls onto a floor 20 cells below the spawn point.
  local function falling(locomotion)
    return {
      gravity = 0.6,
      ground_y = 40,
      wrap_mode = "none",
      locomotion = locomotion,
    }
  end

  local function run(steps)
    for _ = 1, steps do
      engine.step(1 / 60, { columns = 200, lines = 100 })
    end
  end

  it("changes state when a ballistic entity touches down", function()
    local e = jumper(falling("ballistic"), { on_land = "landed" })
    run(120)
    assert.are_equal(
      "landed",
      e.current_state,
      "a ballistic entity that reached its floor must fire on_land"
    )
    engine.clear()
  end)

  it("does not fire on_land again while the entity rests on the floor", function()
    -- Gravity re-accelerates a resting entity every tick and the clamp catches
    -- it again, so a landing test written against the clamp alone fires forever.
    local e = jumper(falling("ballistic"), { on_land = "landed" })
    e.y = 40
    e.vy = 0
    e.current_state = "flying"
    run(30)
    assert.are_equal("flying", e.current_state, "sitting on the ground is not a landing")
    engine.clear()
  end)

  it("ignores on_land for a grounded entity", function()
    local e = jumper(falling("grounded"), { on_land = "landed" })
    run(120)
    assert.are_equal(
      "flying",
      e.current_state,
      "on_land belongs to ballistic locomotion, not to every floor"
    )
    engine.clear()
  end)

  it("derives an omitted locomotion from gravity", function()
    -- No manifest in the wild sets `locomotion`, so the derived value is what
    -- every existing asset actually runs under.
    assert.are_equal("grounded", engine.effective_locomotion(falling(nil)))
    assert.are_equal("omnidirectional", engine.effective_locomotion({}))
  end)
end)

-- `sine` was the only path either engine understood, and it moved y alone. The
-- manifest schema has advertised `path_type` as open-ended since the start, so
-- anything else a manifest asked for was silently ignored on both backends.
describe("distract.engine path primitives", function()
  local probe_counter = 0

  --- Spawns one entity running `physics`, phase zeroed, and returns it.
  ---
  --- Amplitudes are in sprite pixels: one cell wide on x, half a cell on y.
  local function path_entity(physics)
    engine.clear()
    probe_counter = probe_counter + 1
    local name = "pathprobe_" .. probe_counter
    engine.setup({
      backend = "halfblock",
      assets = {
        [name] = {
          name = name,
          initial_state = "drift",
          states = {
            drift = {
              animation = { frames = { 0 }, fps = 1.0, loop_anim = true },
              physics = physics,
              transitions = {},
            },
          },
        },
      },
    })
    local orig = vim.notify
    vim.notify = function() end
    engine.spawn(name, { x = 100, y = 20 })
    vim.notify = orig
    local all = engine.get_entities()
    local e = all[#all]
    -- Spawn desynchronises entities with a random phase, which is right for
    -- two suns on screen and fatal for an analytic assertion.
    e.path_phase = 0
    return e
  end

  -- `freq = 0` pins the phase where the test put it, so each assertion is the
  -- path equation evaluated by hand rather than wherever the integrator
  -- happened to arrive.

  it("moves an orbital path along x as well as y", function()
    local e = path_entity({
      wrap_mode = "none",
      path_type = "orbital",
      path_params = { freq = 0.0, amp_x = 12.0, amp_y = 5.0 },
    })
    engine.step(0.1, { columns = 200, lines = 50 })
    assert(
      math.abs(e.x - 112) < 1e-4,
      string.format("orbital must move x: cos(0) * 12 from base_x 100, got %.4f", e.x)
    )
    assert(
      math.abs(e.y - 20) < 1e-4,
      string.format("orbital y at phase 0 sits on base_y, got %.4f", e.y)
    )
    engine.clear()
  end)

  it("offsets a lissajous path's x axis by its phase_delta", function()
    local e = path_entity({
      wrap_mode = "none",
      path_type = "lissajous",
      path_params = { freq = 0.0, amp_x = 10.0, amp_y = 4.0, phase_delta = math.pi / 2 },
    })
    engine.step(0.1, { columns = 200, lines = 50 })
    assert(
      math.abs(e.x - 110) < 1e-4,
      string.format("sin(0 + pi/2) * 10 from base_x 100, got %.4f", e.x)
    )
    assert(
      math.abs(e.y - 20) < 1e-4,
      string.format("phase_delta is an x-axis offset only, got y %.4f", e.y)
    )
    engine.clear()
  end)

  it("starts a bezier path on its first control point", function()
    local e = path_entity({
      wrap_mode = "none",
      path_type = "bezier",
      path_params = {
        freq = 0.0,
        points = { { 10, 20 }, { 30, 0 }, { 50, 40 }, { 70, 5 } },
      },
    })
    engine.step(0.1, { columns = 200, lines = 50 })
    assert(
      math.abs(e.x - 110) < 1e-4,
      string.format("a cubic at t=0 is its first control point, got x %.4f", e.x)
    )
    -- 20 sprite pixels is 10 cells: the y axis is half-height.
    assert(
      math.abs(e.y - 30) < 1e-4,
      string.format("control points are relative to the spawn position, got y %.4f", e.y)
    )
    engine.clear()
  end)

  it("reads the legacy sine fields as amp_y and freq_y", function()
    local legacy = path_entity({
      wrap_mode = "none",
      path_type = "sine",
      path_amplitude = 15.0,
      path_frequency = 2.0,
    })
    for _ = 1, 20 do
      engine.step(0.05, { columns = 200, lines = 50 })
    end
    local legacy_y = legacy.y

    local modern = path_entity({
      wrap_mode = "none",
      path_type = "sine",
      path_params = { amp_y = 15.0, freq_y = 2.0 },
    })
    for _ = 1, 20 do
      engine.step(0.05, { columns = 200, lines = 50 })
    end

    assert(
      math.abs(legacy_y - 20) > 1e-3,
      "the fixture has to actually be moving for this to mean anything"
    )
    assert(
      math.abs(legacy_y - modern.y) < 1e-9,
      string.format(
        "path_amplitude/path_frequency must alias amp_y/freq_y exactly, got %.9f vs %.9f",
        legacy_y,
        modern.y
      )
    )
    engine.clear()
  end)

  it("does not keep the world awake for a linear path alone", function()
    local e = path_entity({ wrap_mode = "none", path_type = "linear" })
    e.vx, e.vy = 0, 0
    assert.is_true(
      engine.is_quiescent(),
      "`linear` overrides no position, so it produces no new pictures"
    )
    engine.clear()
  end)
end)
