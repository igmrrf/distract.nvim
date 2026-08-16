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

  engine.clear()
  engine.setup({
    backend = "halfblock",
    assets = {
      [name] = {
        name = name,
        initial_state = "idle",
        states = {
          idle = {
            animation = { frames = { 0 }, fps = 8.0, loop_anim = true },
            physics = fixture.physics,
            -- Any transition firing mid-run would swap in another state's
            -- physics and the trajectory would stop describing the fixture.
            transitions = {},
          },
        },
      },
    },
  })

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
  e.frame_idx = 0
  e.frame_timer = 0
  e.state_time = 0

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
