require("tests.test_harness")

local engine = require("distract.engine")
local events = require("distract.events")
local overlay_plugins = require("distract.overlay_plugins")
local plugins = require("distract.plugins")

--- Fresh in-terminal engine with one cat, and no plugins registered.
local function fresh_world()
  plugins.reset()
  require("distract").setup({ backend = "halfblock" })
  engine.clear()
  engine.spawn("cat")
  return engine.get_entities()[1]
end

--- Silences `vim.notify` and counts what it swallowed at or above WARN.
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

describe("distract.plugins registration", function()
  it("refuses a spec that declares a hook the engine does not dispatch", function()
    plugins.reset()
    local ok, err = pcall(plugins.register, "typo", { on_tik = function() end })
    assert.is_false(ok)
    assert.is_true(tostring(err):find("unknown hook 'on_tik'", 1, true) ~= nil)
  end)

  it("refuses a hook that is not callable", function()
    plugins.reset()
    local ok = pcall(plugins.register, "wrong", { on_tick = "not a function" })
    assert.is_false(ok)
  end)

  it("refuses a spec with no hooks at all", function()
    plugins.reset()
    local ok = pcall(plugins.register, "empty", {})
    assert.is_false(ok)
  end)

  it("refuses a duplicate name rather than shadowing the first registration", function()
    plugins.reset()
    plugins.register("first", { on_tick = function() end })
    local ok = pcall(plugins.register, "first", { on_tick = function() end })
    assert.is_false(ok)
    assert.are.same({ "first" }, plugins.names())
  end)

  it("dispatches in registration order", function()
    plugins.reset()
    local order = {}
    plugins.register("a", {
      on_editor_event = function()
        table.insert(order, "a")
      end,
    })
    plugins.register("b", {
      on_editor_event = function()
        table.insert(order, "b")
      end,
    })
    plugins.dispatch_editor_event("typing", {})
    assert.are.same({ "a", "b" }, order)
  end)

  it("runs on_teardown when a plugin is unregistered", function()
    plugins.reset()
    local torn_down = false
    plugins.register("bye", {
      on_teardown = function()
        torn_down = true
      end,
    })
    assert.is_true(plugins.unregister("bye"))
    assert.is_true(torn_down)
    assert.are.same({}, plugins.names())
  end)

  it("reports has_subscriber per hook, so nothing is subscribed to needlessly", function()
    plugins.reset()
    assert.is_false(plugins.has_subscriber("on_tick"))
    plugins.register("ticker", { on_tick = function() end })
    assert.is_true(plugins.has_subscriber("on_tick"))
    assert.is_false(plugins.has_subscriber("on_collision"))
  end)
end)

describe("distract.plugins failure policy", function()
  it("disables a plugin that raises, and reports it once", function()
    plugins.reset()
    local calls = 0
    plugins.register("bad", {
      on_editor_event = function()
        calls = calls + 1
        error("boom")
      end,
    })

    local warnings = count_warnings(function()
      for _ = 1, 5 do
        plugins.dispatch_editor_event("typing", {})
      end
    end)

    assert.are_equal(1, calls)
    assert.are_equal(1, warnings)
    assert.is_true(plugins.is_disabled("bad"))
  end)

  it("keeps dispatching to the plugins that did not raise", function()
    plugins.reset()
    local survived = 0
    plugins.register("bad", {
      on_editor_event = function()
        error("boom")
      end,
    })
    plugins.register("good", {
      on_editor_event = function()
        survived = survived + 1
      end,
    })

    count_warnings(function()
      plugins.dispatch_editor_event("typing", {})
      plugins.dispatch_editor_event("typing", {})
    end)

    assert.are_equal(2, survived)
  end)
end)

describe("distract.plugins entity access", function()
  it("hands hooks a read-only entity", function()
    local entity = fresh_world()
    local seen = nil
    plugins.register("reader", {
      on_tick = function(view)
        seen = view
      end,
    })

    engine.step(0.05, { columns = 80, lines = 24 })

    assert.is_not_nil(seen)
    assert.are_equal(entity.id, seen.id)
    assert.are_equal(entity.asset_name, seen.asset_name)

    local ok, err = pcall(function()
      seen.vx = 99
    end)
    assert.is_false(ok)
    assert.is_true(tostring(err):find("read%-only") ~= nil)
    assert.are_not_equal(99, entity.vx)
    engine.clear()
    plugins.reset()
  end)

  it("exposes the world handle with the running backend named", function()
    fresh_world()
    local world = nil
    plugins.register("initer", {
      on_init = function(handle)
        world = handle
      end,
    })
    plugins.bind_world({ backend = "halfblock", entities = engine.get_entities })

    assert.is_not_nil(world)
    assert.are_equal("halfblock", world.backend)
    assert.are_equal(1, #world.entities())
    engine.clear()
    plugins.reset()
  end)
end)

describe("distract.plugins world commands", function()
  it("applies a requested state on the next step, not mid-frame", function()
    local entity = fresh_world()
    local requested = false
    plugins.register("stater", {
      on_tick = function(view)
        if not requested then
          requested = true
          plugins.world().request_state(view.id, "walk")
        end
      end,
    })
    plugins.bind_world({ backend = "halfblock", entities = engine.get_entities })

    engine.step(0.05, { columns = 80, lines = 24 })
    assert.are_equal("idle", entity.current_state)

    engine.step(0.05, { columns = 80, lines = 24 })
    assert.are_equal("walk", entity.current_state)
    engine.clear()
    plugins.reset()
  end)

  it("adds an impulse to the entity's velocity", function()
    local entity = fresh_world()
    plugins.bind_world({ backend = "halfblock", entities = engine.get_entities })
    entity.vx = 0
    plugins.world().apply_impulse(entity.id, 3, 0)

    engine.step(0.001, { columns = 80, lines = 24 })
    assert.is_true(entity.vx > 2, "impulse should have moved vx, got " .. tostring(entity.vx))
    engine.clear()
    plugins.reset()
  end)

  it("despawns on request", function()
    local entity = fresh_world()
    plugins.bind_world({ backend = "halfblock", entities = engine.get_entities })
    plugins.world().despawn(entity.id)

    engine.step(0.05, { columns = 80, lines = 24 })
    engine.step(0.05, { columns = 80, lines = 24 })
    assert.are_equal(0, #engine.get_entities())
    plugins.reset()
  end)

  it("refuses a command with the wrong argument types", function()
    fresh_world()
    plugins.bind_world({ backend = "halfblock", entities = engine.get_entities })
    local world = plugins.world()
    assert.is_false(pcall(world.request_state, "not an id", "walk"))
    assert.is_false(pcall(world.request_state, 1, ""))
    assert.is_false(pcall(world.apply_impulse, "not an id", 1, 1))
    assert.is_false(pcall(world.despawn, "not an id"))
    engine.clear()
    plugins.reset()
  end)

  it("marks the world dirty so a plugin's own layer is not suppressed", function()
    fresh_world()
    plugins.bind_world({ backend = "halfblock", entities = engine.get_entities })
    -- Consume whatever the spawn itself dirtied.
    plugins.consume_dirty()
    assert.is_false(plugins.consume_dirty())

    plugins.mark_dirty()
    assert.is_false(engine.is_quiescent(), "a dirty world is never quiescent")
    engine.clear()
    plugins.reset()
  end)
end)

describe("distract.plugins simulation hooks", function()
  it("reports a state transition with both states", function()
    local entity = fresh_world()
    local transitions = {}
    plugins.register("watcher", {
      on_state_change = function(view, from_state, to_state)
        table.insert(transitions, { id = view.id, from = from_state, to = to_state })
      end,
    })

    engine.set_entity_state(entity, "walk")
    assert.are_equal(1, #transitions)
    assert.are_equal("idle", transitions[1].from)
    assert.are_equal("walk", transitions[1].to)

    -- Setting the state it already has is not a transition.
    engine.set_entity_state(entity, "walk")
    assert.are_equal(1, #transitions)
    engine.clear()
    plugins.reset()
  end)

  it("reports the edge an entity bounced off", function()
    local entity = fresh_world()
    local edges = {}
    plugins.register("bouncer", {
      on_collision = function(_, collision)
        table.insert(edges, collision.edge)
      end,
    })

    entity.manifest.states[entity.current_state].physics.wrap_mode = "bounce"
    entity.x = -5
    engine.step(0.05, { columns = 80, lines = 24 })

    assert.are.same({ "left" }, edges)
    engine.clear()
    plugins.reset()
  end)

  it("passes editor events through with their context", function()
    plugins.reset()
    local seen = {}
    plugins.register("typist", {
      on_editor_event = function(event_name, context)
        table.insert(seen, { name = event_name, col = context.cursor_col })
      end,
    })

    events.dispatch_event("typing", { cursor_col = 12 })

    assert.are_equal(1, #seen)
    assert.are_equal("typing", seen[1].name)
    assert.are_equal(12, seen[1].col)
    plugins.reset()
  end)

  it("reports where each sprite was drawn, in cells", function()
    local entity = fresh_world()
    local layers = nil
    plugins.register("painter", {
      on_draw = function(reported)
        layers = reported
      end,
    })

    engine.tick()

    assert.is_not_nil(layers)
    assert.are_equal(1, #layers)
    assert.are_equal(entity.id, layers[1].id)
    assert.are_equal("cat", layers[1].asset_name)
    assert.is_true(layers[1].width > 0)
    assert.is_true(layers[1].height > 0)
    engine.clear()
    plugins.reset()
  end)
end)

describe("distract.overlay_plugins", function()
  local CELL = { width = 10, height = 20 }

  it("asks for nothing while no plugin subscribes", function()
    plugins.reset()
    overlay_plugins.reset()
    assert.is_nil(overlay_plugins.desired_snapshot_ms())
    assert.is_false(overlay_plugins.wants_world_events())
  end)

  it("asks for snapshots once a tick or draw hook exists", function()
    plugins.reset()
    plugins.register("ticker", { on_tick = function() end })
    assert.is_true(overlay_plugins.desired_snapshot_ms() > 0)
    plugins.reset()
  end)

  it("wants world events for a collision hook without wanting snapshots", function()
    plugins.reset()
    plugins.register("bumper", { on_collision = function() end })
    assert.is_nil(overlay_plugins.desired_snapshot_ms())
    assert.is_true(overlay_plugins.wants_world_events())
    plugins.reset()
  end)

  it("converts the overlay's pixels into cells before a hook sees them", function()
    plugins.reset()
    overlay_plugins.reset()
    local seen = nil
    plugins.register("converter", {
      on_tick = function(view, dt)
        seen = { x = view.x, y = view.y, vx = view.vx, dt = dt }
      end,
    })

    overlay_plugins.on_snapshot({
      entities = {
        { id = 1, asset_name = "cat", state = "walk", x = 100, y = 40, vx = 20, vy = 0 },
      },
      dt = 0.1,
    }, CELL)

    assert.is_not_nil(seen)
    assert.are_equal(10, seen.x)
    assert.are_equal(2, seen.y)
    assert.are_equal(2, seen.vx)
    assert.are_equal(0.1, seen.dt)
    plugins.reset()
    overlay_plugins.reset()
  end)

  it("dispatches world events against the entity the snapshot described", function()
    plugins.reset()
    overlay_plugins.reset()
    local transitions, edges = {}, {}
    plugins.register("observer", {
      on_state_change = function(view, from_state, to_state)
        table.insert(transitions, { id = view.id, from = from_state, to = to_state })
      end,
      on_collision = function(_, collision)
        table.insert(edges, collision.edge)
      end,
    })

    overlay_plugins.on_snapshot({
      entities = { { id = 4, asset_name = "cat", state = "idle", x = 0, y = 0, vx = 0, vy = 0 } },
      dt = 0.1,
    }, CELL)
    overlay_plugins.on_world_events({
      events = {
        { event = "state_change", id = 4, from = "idle", to = "walk" },
        { event = "collision", id = 4, edge = "right" },
        -- An entity no snapshot has described yet: dropped rather than passed on
        -- with no position.
        { event = "collision", id = 99, edge = "left" },
      },
    })

    assert.are_equal(1, #transitions)
    assert.are_equal("walk", transitions[1].to)
    assert.are.same({ "right" }, edges)
    plugins.reset()
    overlay_plugins.reset()
  end)

  it("warns when the engine had to drop events", function()
    plugins.reset()
    overlay_plugins.reset()
    local warnings = count_warnings(function()
      overlay_plugins.on_world_events({ events = {}, dropped = 7 })
    end)
    assert.are_equal(1, warnings)
    plugins.reset()
  end)

  it("forwards each queued command as its own IPC message", function()
    plugins.reset()
    overlay_plugins.reset()
    plugins.bind_world({ backend = "overlay", entities = overlay_plugins.entities })
    local world = plugins.world()
    world.request_state(1, "jump")
    world.apply_impulse(2, -1.5, 0.5)
    world.despawn(3)

    local sent = {}
    overlay_plugins.flush_commands(function(command)
      table.insert(sent, command)
    end)

    assert.are_equal(3, #sent)
    assert.are_equal("SetState", sent[1].command)
    assert.are_equal("jump", sent[1].state)
    assert.are_equal("Impulse", sent[2].command)
    assert.are_equal(-1.5, sent[2].vx)
    assert.are_equal("Despawn", sent[3].command)
    assert.are_equal(3, sent[3].id)

    -- Drained: a second flush sends nothing.
    local again = {}
    overlay_plugins.flush_commands(function(command)
      table.insert(again, command)
    end)
    assert.are_equal(0, #again)
    plugins.reset()
    overlay_plugins.reset()
  end)
end)
