require("tests.test_harness")

-- What a spawn produces, pinned field by field.
--
-- The physics-parity fixtures cover the *step* exhaustively and barely touch the
-- spawn: they set an explicit x and y and then zero the randomised fields, so
-- almost everything `M.spawn` decides -- anchoring, the floor, parallax, draw
-- order, which fields the initial state's physics seeds, and the desynchronisation
-- that stops two pets moving in lockstep -- had no test at all.
--
-- This is the characterization suite for extracting entity construction out of
-- `engine.lua`. It asserts current behaviour rather than desired behaviour: if one
-- of these fails after a refactor, the refactor changed something.

local distract = require("distract")
local engine = require("distract.engine")
local position = require("distract.position")
local viewport = require("distract.viewport")

local function quietly(fn)
  local notify = vim.notify
  vim.notify = function() end
  local ok, result = pcall(fn)
  vim.notify = notify
  if not ok then
    error(result, 0)
  end
  return result
end

--- Spawns into a clean world and returns the entity, with the randomised fields
--- left alone so they can be asserted on separately.
local function spawn(asset_name, opts)
  viewport.reset()
  distract.setup({ backend = "halfblock" })
  engine.clear()
  engine.set_ground_row(nil)
  engine.set_obstacles({})
  local id = quietly(function()
    return engine.spawn(asset_name, opts)
  end)
  return engine.get_entities()[#engine.get_entities()], id
end

describe("spawn identity and bookkeeping", function()
  it("returns the id it assigned, and allocates them monotonically", function()
    local first, first_id = spawn("cat")
    assert.are_equal(first.id, first_id)

    local second_id = quietly(function()
      return engine.spawn("cat")
    end)
    assert.is_true(second_id > first_id, "ids must not be reused within a session")
    engine.clear()
  end)

  it("refuses a manifest its own capabilities forbid, and allocates nothing", function()
    viewport.reset()
    distract.setup({ backend = "halfblock" })
    engine.clear()
    distract.register_asset("refused_probe", {
      manifest = {
        name = "refused_probe",
        initial_state = "orbiting",
        locomotion = "grounded",
        capabilities = { locomotion = { "grounded" } },
        states = {
          orbiting = {
            animation = { frames = { 0 }, fps = 4, loop_anim = true },
            physics = { path_type = "orbital", locomotion = "omnidirectional" },
          },
        },
      },
    })

    local id = quietly(function()
      return engine.spawn("refused_probe")
    end)
    assert.is_nil(id, "a refused spawn returns nil")
    assert.are_equal(0, #engine.get_entities())
    engine.clear()
  end)

  it("falls back to the cat's behaviour for an unknown asset, under the asked-for name", function()
    local entity = spawn("no_such_asset_at_all")
    assert.are_equal("no_such_asset_at_all", entity.asset_name)
    assert.are_equal("cat", entity.manifest.name)
    engine.clear()
  end)

  it("starts the engine if it was not already running", function()
    viewport.reset()
    distract.setup({ backend = "halfblock" })
    engine.clear()
    engine.stop()
    assert.is_false(engine.is_running())
    quietly(function()
      engine.spawn("cat")
    end)
    assert.is_true(engine.is_running())
    engine.clear()
  end)
end)

describe("spawn reaches a registered manifest", function()
  -- Regression. The backends hold a snapshot of `config` taken when they were set
  -- up, so a manifest registered afterwards never reached them: the spawn fell
  -- through to `require("distract.manifests." .. name)`, failed, and drew the cat
  -- under the asked-for name. `register_asset`'s own contract is that this cannot
  -- happen, and only the art half of it held.
  it("uses a manifest registered after setup, rather than falling back to the cat", function()
    viewport.reset()
    distract.setup({ backend = "halfblock" })
    engine.clear()

    distract.register_asset("registered_after_setup", {
      manifest = {
        name = "registered_after_setup",
        initial_state = "hovering",
        states = {
          hovering = {
            animation = { frames = { 0 }, fps = 4, loop_anim = true },
            physics = { target_vx = 0, wrap_mode = "clamp" },
          },
        },
      },
    })

    local entity = quietly(function()
      engine.spawn("registered_after_setup", { x = 4, y = 4 })
      return engine.get_entities()[1]
    end)

    assert.are_equal("registered_after_setup", entity.manifest.name)
    assert.are_equal("hovering", entity.current_state)
    engine.clear()
  end)
end)

describe("spawn placement", function()
  it("honours an explicit position exactly", function()
    local entity = spawn("cat", { x = 17, y = 5 })
    assert.are_equal(17, entity.x)
    assert.are_equal(5, entity.y)
    -- A path primitive anchors on where the entity started, both axes.
    assert.are_equal(17, entity.base_x)
    assert.are_equal(5, entity.base_y)
    engine.clear()
  end)

  it("stands a bottom-anchored asset on the floor it was pushed", function()
    viewport.reset()
    distract.setup({ backend = "halfblock" })
    engine.clear()
    engine.set_ground_row(30)
    local entity = quietly(function()
      engine.spawn("cat")
      return engine.get_entities()[1]
    end)

    -- The floor is the exclusive bottom edge, so an entity of height h has its
    -- top-left at floor - h.
    local sprite_h = 8
    assert.are_equal(30 - sprite_h, entity.y)
    assert.are_equal(entity.y, entity.ground_y)
    engine.set_ground_row(nil)
    engine.clear()
  end)

  it("centres in the bounds when nothing anchors it", function()
    viewport.reset()
    distract.setup({
      backend = "halfblock",
      position = vim.tbl_extend("force", vim.deepcopy(position.DEFAULTS), { anchor = "free" }),
    })
    engine.clear()
    engine.set_ground_row(nil)
    local entity = quietly(function()
      engine.spawn("cat")
      return engine.get_entities()[1]
    end)

    assert.are_equal(math.floor(vim.o.columns / 2), entity.x)
    assert.are_equal(math.floor(vim.o.lines / 2), entity.y)
    distract.config.position = vim.deepcopy(position.DEFAULTS)
    engine.clear()
  end)

  it("takes its facing from flip_x, and heads that way", function()
    local facing_left = spawn("cat", { x = 10, y = 5, flip_x = true })
    assert.is_true(facing_left.flip_x)
    assert.are_equal(-1, facing_left.heading_x)
    engine.clear()

    local facing_right = spawn("cat", { x = 10, y = 5 })
    assert.is_false(facing_right.flip_x)
    assert.are_equal(1, facing_right.heading_x)
    engine.clear()
  end)
end)

describe("spawn depth", function()
  it("uses the manifest's z_index when the spawn names no depth", function()
    local entity = spawn("cat", { x = 4, y = 4 })
    assert.are_equal(distract.config.assets.cat.z_index or 10, entity.z_index)
    assert.are_equal(0, entity.z)
    engine.clear()
  end)

  it("lets a spawned z override the manifest's draw order, rounded", function()
    local entity = spawn("cat", { x = 4, y = 4, z = 2.6 })
    assert.are_equal(3, entity.z_index)
    assert.are_equal(2.6, entity.z)
    engine.clear()
  end)

  it("flattens parallax to 1 on a backend that cannot scale a sprite", function()
    local entity = spawn("cat", { x = 4, y = 4, z = 3 })
    assert.are_equal(1, entity.parallax, "halfblock cannot scale, so depth damps nothing")
    engine.clear()
  end)
end)

describe("spawn initial state", function()
  it("seeds velocity from the initial state's physics, signed by the heading", function()
    local entity = spawn("cat_walking", { x = 4, y = 4 })
    local physics = entity.manifest.states[entity.current_state].physics

    assert.are_equal("walk", entity.current_state)
    assert.are_equal((physics.target_vx or 0) * entity.heading_x, entity.target_vx)
    assert.are_equal(entity.target_vx, entity.vx)
    assert.are_equal(physics.target_vy or 0, entity.target_vy)
    assert.are_equal(entity.target_vy, entity.vy)
    engine.clear()
  end)

  it("starts unlocked, active, and with a fresh state clock", function()
    local entity = spawn("cat", { x = 4, y = 4 })
    assert.is_false(entity.is_locked)
    assert.is_true(entity.is_active)
    assert.are_equal(0, entity.state_time)
    assert.is_false(entity.animation_finished)
    assert.is_nil(entity.action_timer)
    assert.is_nil(entity.action_duration)
    assert.is_nil(entity.return_state)
    engine.clear()
  end)

  it("desynchronises from anything already alive", function()
    -- Two entities sharing a frame index, a frame timer and a path phase read as
    -- a chorus line rather than as two animals. Asserted as a property of the
    -- fields rather than as an exact value, because the source is math.random.
    viewport.reset()
    distract.setup({ backend = "halfblock" })
    engine.clear()

    local frame_indices, phases = {}, {}
    quietly(function()
      for _ = 1, 12 do
        engine.spawn("cat")
      end
    end)
    for _, entity in ipairs(engine.get_entities()) do
      frame_indices[entity.frame_idx] = true
      table.insert(phases, entity.path_phase)
      assert.is_true(entity.frame_idx >= 1, "a Lua frame index is 1-based")
      assert.is_true(entity.frame_timer >= 0 and entity.frame_timer < 0.1)
      assert.is_true(entity.path_phase >= 0 and entity.path_phase < 2 * math.pi)
    end

    local distinct_phases = {}
    for _, phase in ipairs(phases) do
      distinct_phases[tostring(phase)] = true
    end
    assert.is_true(
      vim.tbl_count(distinct_phases) > 1,
      "twelve entities must not all share one path phase"
    )
    engine.clear()
  end)

  it("lets a state's own ground_y override the floor it was pushed", function()
    viewport.reset()
    distract.setup({ backend = "halfblock" })
    engine.clear()
    engine.set_ground_row(30)
    distract.register_asset("grounded_probe", {
      manifest = {
        name = "grounded_probe",
        initial_state = "idle",
        states = {
          idle = {
            animation = { frames = { 0 }, fps = 4, loop_anim = true },
            physics = { gravity = 0.5, ground_y = 12 },
          },
        },
      },
    })

    local entity = quietly(function()
      engine.spawn("grounded_probe", { x = 4, y = 2 })
      return engine.get_entities()[1]
    end)
    assert.are_equal(12, entity.ground_y)
    engine.set_ground_row(nil)
    engine.clear()
  end)
end)
