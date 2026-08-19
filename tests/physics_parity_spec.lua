-- Cross-engine physics parity, Lua half.
--
-- Asserts the Lua engine reproduces the same trajectories as the Rust engine,
-- from the same fixtures, in the same units. The goldens are produced by
-- `engine/tests/physics_parity.rs`; this suite never runs Rust and Rust never
-- runs this, so neither needs the other's toolchain -- they meet at the files
-- in `tests/fixtures/physics/`.
--
-- Both file headers have long claimed "one manifest describes one behaviour on
-- both backends". Three divergences (`wrap`, `bounce`, `animation.flip_x`) had
-- to be found by reading. This is what makes the claim testable.
--
-- Frame timing is part of the same contract. `frame_duration_seconds` exists in
-- both engines with the same precedence rule and had no fixture guarding it. A
-- fixture carrying an `animation` block exercises it. The compared value is the
-- atlas frame each engine would draw, not `frame_idx`: Lua indexes
-- `animation.frames` from 1 and Rust from 0, so the raw index diverges by
-- convention while the drawn frame must not.
--
-- The middle branch of that rule -- the delays a source file carries -- needs
-- real imported art, because a procedural probe has none. A fixture may
-- therefore carry a `spritesheet` block naming a GIF relative to the repository
-- root, decoded by `distract.gif` here and by the `image` crate there.
--
-- Regenerate the goldens after an intentional behaviour change:
--   UPDATE_GOLDEN=1 cargo test --manifest-path engine/Cargo.toml --test physics_parity

require("tests.test_harness")

local engine = require("distract.engine")

local FIXTURE_DIR = "tests/fixtures/physics"

-- Rust computes in f32 and Lua in f64, so the trajectories converge rather
-- than coincide. Real divergence bugs -- a missing axis, an unread field, a
-- boundary applied in the wrong place -- are order-of-cells, thousands of
-- times larger than this.
local TOLERANCE = 1e-3

local probe_counter = 0

local function read_json(path)
  local fd = io.open(path, "r")
  assert(fd, "cannot open " .. path)
  local raw = fd:read("*a")
  fd:close()
  return vim.json.decode(raw)
end

--- Silences the spawn/clear notifications the engine emits by design.
local function quietly(fn)
  local original = vim.notify
  vim.notify = function() end
  local ok, err = pcall(fn)
  vim.notify = original
  if not ok then
    error(err, 0)
  end
end

--- Runs one fixture through the Lua engine and returns its trajectory in cells.
---
--- Mirrors `run()` in physics_parity.rs step for step. The asset name is left
--- unregistered on purpose: `sprite_cell_size` then falls back to the cat's
--- 24x16 art, which is exactly the manifest the Rust side starts from, so the
--- sprite dimensions that boundary handling depends on match by construction.
local function run(fixture)
  probe_counter = probe_counter + 1
  local name = "parity_probe_" .. probe_counter

  -- Absent on every physics fixture: a multi-frame loop makes the world
  -- permanently non-quiescent and would mask a disagreement in that rule.
  -- Present only on the fixtures whose subject *is* frame timing.
  local probe_animation = { frames = { 0 }, fps = 8.0, loop_anim = true }
  if fixture.animation then
    probe_animation = {
      frames = fixture.animation.frames,
      fps = fixture.animation.fps,
      loop_anim = fixture.animation.loop_anim,
    }
  end

  -- Present only on the fixture whose subject is the per-frame delays a source
  -- file carries. A procedural probe has none, so that branch of
  -- `frame_duration_seconds` cannot be reached without one. The path is
  -- resolved against the plugin root by `asset_path`, which is the same
  -- repository-relative string the Rust runner joins onto its own root.
  local probe_spritesheet = nil
  if fixture.spritesheet then
    probe_spritesheet = {
      path = fixture.spritesheet.path,
      frame_width = fixture.spritesheet.frame_width,
      frame_height = fixture.spritesheet.frame_height,
    }
  end

  engine.clear()
  engine.setup({
    backend = "halfblock",
    assets = {
      [name] = {
        name = name,
        initial_state = "idle",
        spritesheet = probe_spritesheet,
        states = {
          idle = {
            animation = probe_animation,
            physics = fixture.physics,
            -- Empty for almost every fixture: a transition firing mid-run swaps
            -- in another state's physics and the trajectory stops describing
            -- the fixture. `on_land` is the exception, since its whole subject
            -- is *when* the state changes.
            transitions = fixture.transitions or {},
          },
          -- A landing target for `on_land`, defined to match
          -- `StateDefinition::default()` on the Rust side field for field, so
          -- the state a fixture lands in has the same animation, physics and
          -- quiescence whichever engine ran it.
          landed = {
            animation = { frames = { 0 }, fps = 8.0, loop_anim = true },
            physics = {},
            transitions = {},
          },
        },
      },
    },
  })

  -- Set on every fixture, nil included: the floor is engine state that would
  -- otherwise carry over from whichever fixture ran before this one.
  engine.set_ground_row(nil)
  engine.set_ground_row(fixture.ground_row)

  quietly(function()
    engine.spawn(name, {
      x = fixture.spawn.x,
      y = fixture.spawn.y,
      flip_x = fixture.spawn.flip_x,
    })
  end)

  local entities = engine.get_entities()
  local e = entities[#entities]

  -- Spawn deliberately desynchronises entities from one another with random
  -- frame and phase offsets -- right for two cats on screen, fatal for a
  -- reproducible trajectory. Zeroed on both sides.
  e.path_phase = 0
  -- 1 is Lua's first frame, matching what `spawn` would have picked. Rust's
  -- first frame is 0; the recorded sheet index reconciles the two.
  e.frame_idx = 1
  e.frame_timer = 0
  e.state_time = 0
  -- Applied here for the same reason: a fixture describes what the *engine* is
  -- given, not the `position` config and backend capabilities that produced it.
  -- The half-block backend would otherwise flatten every parallax to 1.
  if fixture.spawn.parallax then
    e.parallax = fixture.spawn.parallax
  end

  local frames_by_state = {}
  local manifest_states = e.manifest.states
  for state_name, state_def in pairs(manifest_states) do
    frames_by_state[state_name] = state_def.animation.frames
  end

  --- The atlas frame the renderer would draw, resolved exactly as
  --- `renderer.lua` resolves it.
  local function drawn_sheet_index()
    local frames = frames_by_state[e.current_state]
    assert(frames and #frames > 0, "the recorded state declares no frames")
    local position = ((math.max(1, e.frame_idx or 1) - 1) % #frames) + 1
    return frames[position]
  end

  local bounds = { columns = fixture.bounds.columns, lines = fixture.bounds.lines }
  local trajectory = {}
  for _ = 1, fixture.steps do
    quietly(function()
      engine.step(fixture.dt, bounds)
    end)
    trajectory[#trajectory + 1] = {
      x = e.x,
      y = e.y,
      vx = e.vx,
      vy = e.vy,
      flip_x = e.flip_x,
      state = e.current_state,
      quiescent = engine.is_quiescent(),
      sheet_index = drawn_sheet_index(),
      animation_finished = e.animation_finished == true,
    }
  end

  return trajectory
end

--- Fixture names, excluding the golden files that sit beside them.
local function fixture_names()
  local names = {}
  for _, path in ipairs(vim.fn.glob(FIXTURE_DIR .. "/*.json", false, true)) do
    if not path:match("%.golden%.json$") then
      names[#names + 1] = vim.fn.fnamemodify(path, ":t:r")
    end
  end
  table.sort(names)
  return names
end

describe("distract physics parity with the overlay engine", function()
  local names = fixture_names()

  it("has fixtures and goldens to compare", function()
    assert(#names > 0, "no fixtures found in " .. FIXTURE_DIR)
    for _, name in ipairs(names) do
      local golden = FIXTURE_DIR .. "/" .. name .. ".golden.json"
      assert(
        vim.fn.filereadable(golden) == 1,
        string.format(
          "no golden for %s. Generate with UPDATE_GOLDEN=1 cargo test "
            .. "--manifest-path engine/Cargo.toml --test physics_parity",
          name
        )
      )
    end
  end)

  for _, name in ipairs(names) do
    it("matches the overlay trajectory for " .. name, function()
      local fixture = read_json(FIXTURE_DIR .. "/" .. name .. ".json")
      local expected = read_json(FIXTURE_DIR .. "/" .. name .. ".golden.json")
      local actual = run(fixture)

      assert.are_equal(
        #expected,
        #actual,
        string.format("%s: golden has %d steps, run produced %d", name, #expected, #actual)
      )

      -- Reports the *first* divergence rather than the last: once two
      -- integrators part company every later step is wrong too, and the step
      -- it happened on is the only one that says why.
      for i = 1, #expected do
        local want, got = expected[i], actual[i]
        for _, field in ipairs({ "x", "y", "vx", "vy" }) do
          local diff = math.abs(want[field] - got[field])
          assert(
            diff <= TOLERANCE,
            string.format(
              "%s step %d: %s diverged from the overlay engine, "
                .. "expected %.6f, got %.6f (delta %.6f cells)",
              name,
              i,
              field,
              want[field],
              got[field],
              diff
            )
          )
        end
        assert(
          want.quiescent == got.quiescent,
          string.format(
            "%s step %d: quiescence diverged, expected %s, got %s",
            name,
            i,
            tostring(want.quiescent),
            tostring(got.quiescent)
          )
        )
        assert(
          want.flip_x == got.flip_x,
          string.format(
            "%s step %d: flip_x diverged, expected %s, got %s",
            name,
            i,
            tostring(want.flip_x),
            tostring(got.flip_x)
          )
        )
        assert(
          want.sheet_index == got.sheet_index,
          string.format(
            "%s step %d: the drawn frame diverged, expected %s, got %s",
            name,
            i,
            tostring(want.sheet_index),
            tostring(got.sheet_index)
          )
        )
        assert(
          want.animation_finished == got.animation_finished,
          string.format(
            "%s step %d: animation_finished diverged, expected %s, got %s",
            name,
            i,
            tostring(want.animation_finished),
            tostring(got.animation_finished)
          )
        )
        assert(
          want.state == got.state,
          string.format(
            "%s step %d: state diverged, expected %s, got %s",
            name,
            i,
            tostring(want.state),
            tostring(got.state)
          )
        )
      end

      engine.clear()
    end)
  end
end)
