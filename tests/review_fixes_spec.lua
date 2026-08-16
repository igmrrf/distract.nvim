--- Regressions for the findings in REVIEW.md.
---
--- Each test names the behaviour that was wrong, so a future change that
--- reintroduces it fails here rather than in someone's editor.

require("tests.test_harness")

describe("distract.events routing", function()
  local events = require("distract.events")
  local engine = require("distract.engine")

  after_each(function()
    events.teardown()
    engine.stop()
  end)

  it("routes idle to the in-terminal engine, not only to the overlay", function()
    -- `reset_idle_timer` used to call `external.send_event` directly, so the
    -- halfblock backend -- the default -- never saw an `idle` event and
    -- `idle_timeout_ms` was dead config for it.
    local seen = {}
    local original = engine.handle_editor_event
    engine.handle_editor_event = function(name)
      table.insert(seen, name)
    end
    engine.start()

    events.dispatch_event("idle")

    engine.handle_editor_event = original
    assert.are.same({ "idle" }, seen)
  end)

  it("throttles each event name independently", function()
    -- In insert mode TextChangedI ("typing") and CursorMovedI ("moving") both
    -- fire on every keystroke. A single shared flag was short-circuited by the
    -- alternating name, so every keystroke dispatched and the entity
    -- flip-flopped between walk_fast and walk.
    events.setup({ debounce_ms = 10000, idle_timeout_ms = 60000 })

    events.emit_debounced("typing")
    events.emit_debounced("moving")
    local first = events.throttle_state()
    assert.is_not_nil(first.typing)
    assert.is_not_nil(first.moving)

    -- A second burst inside the window must not refresh either deadline.
    events.emit_debounced("typing")
    events.emit_debounced("moving")
    local second = events.throttle_state()
    assert.are_equal(first.typing, second.typing)
    assert.are_equal(first.moving, second.moving)
  end)

  it("closes its timer on teardown instead of leaking a libuv handle", function()
    -- The timers were created at module load and only ever stopped, so every
    -- setup/teardown cycle leaked a handle -- and the test suite goes through
    -- many.
    local uv = vim.uv or vim.loop
    local function live_timers()
      local n = 0
      uv.walk(function(handle)
        if handle:get_type() == "timer" and not handle:is_closing() then
          n = n + 1
        end
      end)
      return n
    end

    events.setup({ idle_timeout_ms = 60000 })
    events.teardown()
    local baseline = live_timers()

    for _ = 1, 10 do
      events.setup({ idle_timeout_ms = 60000 })
      events.teardown()
    end

    assert.is_true(
      live_timers() <= baseline,
      string.format("timer handles grew from %d to %d over 10 cycles", baseline, live_timers())
    )
  end)
end)

describe("distract.engine simulation", function()
  local engine = require("distract.engine")

  after_each(function()
    engine.stop()
  end)

  it("uses each asset's real width for boundary checks", function()
    -- `sprite_w` was hardcoded to 16 while cat and crab are 24 cells wide, so
    -- wrap and bounce fired in the wrong place for both.
    local sprites = require("distract.terminal_sprites")
    local cat_w = sprites.get_dimensions("cat")
    local sun_w = sprites.get_dimensions("sun")
    assert.are_equal(24, cat_w)
    assert.are_equal(16, sun_w)
    assert.are_not_equal(cat_w, sun_w)
  end)

  it("wraps an entity whose velocity decayed while it was off screen", function()
    -- The old gate was `vx > 0 and x > columns`. With vx lerped to zero the
    -- entity sat off-screen forever, invisible, and never despawned.
    engine.spawn("cat", { x = 5, y = 5 })
    local e = engine.get_entities()[1]
    e.current_state = "walk"
    e.x = vim.o.columns + 500
    e.vx = 0
    e.target_vx = 0
    e.heading_x = 0

    engine.tick()
    assert.is_true(e.x <= vim.o.columns, "entity stayed off screen at x=" .. tostring(e.x))
  end)

  it("clears entities without stopping, matching the overlay backend", function()
    -- `:DistractClear` used to mean "clear and stop" here and "clear" there.
    engine.spawn("cat", { x = 1, y = 1 })
    assert.is_true(engine.is_running())

    engine.clear()
    assert.are_equal(0, #engine.get_entities())
    assert.is_true(engine.is_running(), "clear must not stop the engine")
  end)

  it("desynchronises entities spawned together", function()
    engine.spawn("cat", { x = 1, y = 1 })
    engine.spawn("cat", { x = 1, y = 1 })
    engine.spawn("cat", { x = 1, y = 1 })
    engine.spawn("cat", { x = 1, y = 1 })
    engine.spawn("cat", { x = 1, y = 1 })
    engine.spawn("cat", { x = 1, y = 1 })

    local phases = {}
    for _, e in ipairs(engine.get_entities()) do
      phases[tostring(math.floor(e.path_phase * 1000))] = true
    end
    local distinct = 0
    for _ in pairs(phases) do
      distinct = distinct + 1
    end
    assert.is_true(distinct > 1, "all entities share one path phase")
  end)

  it("ignores a custom action with no target_state instead of nilling the state", function()
    -- The Rust side is safe here because serde requires the field. This side
    -- had no schema, so `current_state = nil` broke the next tick's lookup.
    engine.spawn("cat", { x = 1, y = 1 })
    local e = engine.get_entities()[1]
    e.manifest = vim.deepcopy(e.manifest)
    e.manifest.custom_actions.broken = { duration_ms = 100 }
    local state_before = e.current_state

    engine.trigger_action("broken", e.id)

    assert.are_equal(state_before, e.current_state)
    assert.is_not_nil(e.current_state)
    assert.has_no.errors(function()
      engine.tick()
    end)
  end)

  it("ignores a custom action targeting a state the manifest does not define", function()
    engine.spawn("cat", { x = 1, y = 1 })
    local e = engine.get_entities()[1]
    e.manifest = vim.deepcopy(e.manifest)
    e.manifest.custom_actions.nowhere = { target_state = "no_such_state" }
    local state_before = e.current_state

    engine.trigger_action("nowhere", e.id)
    assert.are_equal(state_before, e.current_state)
  end)

  it("turns an entity toward the cursor when it reacts", function()
    engine.spawn("cat", { x = 40, y = 5 })
    local e = engine.get_entities()[1]
    e.heading_x = 1
    e.is_locked = false
    e.current_state = "idle"

    engine.handle_editor_event("moving", { cursor_col = 2, cursor_row = 1 })

    assert.are_equal("walk", e.current_state)
    assert.are_equal(-1, e.heading_x)
    assert.is_true(e.flip_x)
  end)
end)

describe("distract.renderer redraw guarding", function()
  local renderer = require("distract.renderer")
  local engine = require("distract.engine")

  after_each(function()
    renderer.clear_all()
    engine.stop()
  end)

  it("caches a frame's rendered lines instead of rebuilding them each draw", function()
    local sprites = require("distract.terminal_sprites")
    sprites.reset_cache()
    local a = { sprites.get_rendered_frame("cat", 1) }
    local b = { sprites.get_rendered_frame("cat", 1) }
    -- Same table identity means the second call did no work.
    assert.are_equal(a[1], b[1])
    assert.are_equal(a[2], b[2])
  end)

  it("does not reconfigure a window that has not moved", function()
    -- `nvim_win_set_config` forces a redraw, so calling it every tick per
    -- entity cost a redraw per entity per tick even for a sleeping pet.
    local entity = {
      id = 1,
      asset_name = "cat",
      x = 4,
      y = 4,
      frame_idx = 1,
      current_state = "idle",
      z_index = 10,
      manifest = require("distract.manifests.cat"),
    }

    renderer.draw({ entity }, "halfblock")
    local first = renderer.window_state(1)
    assert.is_not_nil(first)

    local calls = 0
    local original = vim.api.nvim_win_set_config
    vim.api.nvim_win_set_config = function(...)
      calls = calls + 1
      return original(...)
    end

    renderer.draw({ entity }, "halfblock")
    renderer.draw({ entity }, "halfblock")
    vim.api.nvim_win_set_config = original

    assert.are_equal(0, calls, "a stationary entity must cost no window reconfigures")

    -- Moving it must still reposition.
    entity.x = 20
    vim.api.nvim_win_set_config = function(...)
      calls = calls + 1
      return original(...)
    end
    renderer.draw({ entity }, "halfblock")
    vim.api.nvim_win_set_config = original
    assert.are_equal(1, calls, "a moved entity must reposition exactly once")
  end)
end)

describe("distract.external process handling", function()
  local external = require("distract.external")

  it("does not start the engine just to answer a query", function()
    -- `send_command` used to call `start()`, so :DistractClear or
    -- :DistractStatus after :DistractStop respawned the overlay process.
    external.stop()
    assert.is_false(external.is_running())
    assert.is_false(external.send_command({ command = "GetStatus" }))
    assert.is_false(external.is_running())
  end)

  it("looks where a downloaded release binary would be installed", function()
    -- The release workflow publishes per-platform archives; nothing looked
    -- anywhere they could be installed, so they were unreachable.
    local candidates = external.binary_candidates()
    local found_bin_dir = false
    for _, path in ipairs(candidates) do
      if path:match("/engine/bin/distract%-engine") then
        found_bin_dir = true
      end
    end
    assert.is_true(found_bin_dir, "engine/bin is not searched")
    assert.is_true(#candidates >= 3)
  end)

  it("builds asynchronously rather than freezing the editor", function()
    -- The old path was `vim.fn.system("cargo build --release ...")`, which made
    -- Neovim unresponsive for the length of a cold Rust build.
    local cmd = external.build_command()
    assert.are_equal("cargo", cmd[1])
    assert.are_equal("build", cmd[2])
    assert.is_function(external.build)
  end)

  it("exposes the cell size as configuration with a documented default", function()
    external.setup({})
    local w, h = external.cell_size()
    assert.are_equal(10, w)
    assert.are_equal(20, h)

    external.setup({ cell_width = 16, cell_height = 36 })
    w, h = external.cell_size()
    assert.are_equal(16, w)
    assert.are_equal(36, h)

    external.setup({})
    external.set_reported_cell_size(34, 15)
    w, h = external.cell_size()
    assert.are_equal(15, w)
    assert.are_equal(34, h)
  end)
end)

describe("distract.init lifecycle", function()
  local distract = require("distract")

  it("registers VimLeavePre in a cleared group so setup does not accumulate", function()
    for _ = 1, 4 do
      distract.setup({ backend = "halfblock" })
    end
    local autocmds = vim.api.nvim_get_autocmds({ group = "Distract", event = "VimLeavePre" })
    assert.are_equal(1, #autocmds)
  end)

  it("does not advertise a backend it cannot provide", function()
    local backends = distract.get_available_backends()
    assert.are.same({ "halfblock", "overlay" }, backends)
  end)

  it("lists built-in assets without materialising their manifests", function()
    -- `pairs` only sees assets already loaded, so the built-ins have to be
    -- enumerated explicitly for the lazy table to stay invisible to callers.
    local names = distract.get_asset_names()
    local seen = {}
    for _, n in ipairs(names) do
      seen[n] = true
    end
    for _, builtin in ipairs({ "cat", "crab", "sun" }) do
      assert.is_true(seen[builtin] == true, "missing built-in asset " .. builtin)
    end
  end)
end)

describe("startup cost", function()
  it("does not rasterise sprites merely to load the plugin", function()
    -- Requiring the plugin used to draw all 79 frames of orb-shaded pixel art
    -- on every Neovim start, spawn or not.
    for name, _ in pairs(package.loaded) do
      if name:match("^distract") then
        package.loaded[name] = nil
      end
    end

    local start = vim.loop.hrtime()
    require("distract")
    local ms = (vim.loop.hrtime() - start) / 1e6

    assert.is_true(ms < 5, string.format("require('distract') took %.2f ms", ms))
  end)
end)

describe("distract.renderer frame buffer reuse", function()
  local renderer = require("distract.renderer")
  local sprites = require("distract.terminal_sprites")
  local engine = require("distract.engine")

  after_each(function()
    renderer.clear_all()
    engine.stop()
  end)

  local function cat_entity(id, frame_idx)
    return {
      id = id,
      asset_name = "cat",
      x = 4,
      y = 4,
      frame_idx = frame_idx,
      current_state = "walk",
      z_index = 10,
      manifest = require("distract.manifests.cat"),
    }
  end

  it("advances a frame with one window call, not one per coloured cell", function()
    -- The old path rewrote every line and re-set an extmark per coloured cell
    -- on every frame change: ~90 API calls per entity per frame.
    local e = cat_entity(1, 1)
    local steps = #e.manifest.states.walk.animation.frames

    -- Build every frame buffer once. That first lap is the whole cost; what is
    -- measured below is what an animation costs from then on.
    for step = 1, steps do
      e.frame_idx = step
      renderer.draw({ e }, "halfblock")
    end

    local extmarks, set_lines, set_buf = 0, 0, 0
    local o_ext = vim.api.nvim_buf_set_extmark
    local o_lines = vim.api.nvim_buf_set_lines
    local o_setbuf = vim.api.nvim_win_set_buf
    vim.api.nvim_buf_set_extmark = function(...)
      extmarks = extmarks + 1
      return o_ext(...)
    end
    vim.api.nvim_buf_set_lines = function(...)
      set_lines = set_lines + 1
      return o_lines(...)
    end
    vim.api.nvim_win_set_buf = function(...)
      set_buf = set_buf + 1
      return o_setbuf(...)
    end

    for _ = 1, 3 do
      for step = 1, steps do
        e.frame_idx = step
        renderer.draw({ e }, "halfblock")
      end
    end

    vim.api.nvim_buf_set_extmark = o_ext
    vim.api.nvim_buf_set_lines = o_lines
    vim.api.nvim_win_set_buf = o_setbuf

    assert.are_equal(0, extmarks, "a warm frame must not re-set a single extmark")
    assert.are_equal(0, set_lines, "a warm frame must not rewrite its lines")
    assert(set_buf > 0, "advancing the animation should swap the window's buffer")
  end)

  it("shares one buffer between entities showing the same frame", function()
    local a, b = cat_entity(1, 1), cat_entity(2, 1)
    b.x = 40
    renderer.draw({ a, b }, "halfblock")

    local sa = renderer.window_state(1)
    local sb = renderer.window_state(2)
    assert.is_not_nil(sa)
    assert.is_not_nil(sb)
    assert.are_equal(sa.buf, sb.buf, "identical frames should share one buffer")
  end)

  it("keeps the shared frame buffer alive when one entity's window closes", function()
    local e = cat_entity(1, 1)
    renderer.draw({ e }, "halfblock")
    local buf = renderer.window_state(1).buf

    renderer.close_window(1)
    assert.is_true(
      vim.api.nvim_buf_is_valid(buf),
      "closing one window must not delete a buffer other entities may be showing"
    )
  end)

  it("rebuilds a frame buffer that was wiped out from under it", function()
    local buf = sprites.get_frame_buffer("cat", 1, false)
    vim.api.nvim_buf_delete(buf, { force = true })
    local again = sprites.get_frame_buffer("cat", 1, false)
    assert.is_true(vim.api.nvim_buf_is_valid(again))
  end)

  it("gives sprite windows a background that is not NormalFloat's", function()
    local group = renderer.background_group()
    local hl = vim.api.nvim_get_hl(0, { name = group, link = false })
    assert.is_nil(hl.bg, "a sprite window must not paint a background rectangle")
  end)
end)
