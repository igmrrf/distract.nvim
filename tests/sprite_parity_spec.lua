-- Cross-engine sprite-art parity, Lua half.
--
-- Asserts the Lua sprite generators draw the same art as the overlay's, from
-- the goldens produced by `engine/tests/sprite_parity.rs`. This suite never
-- runs Rust and Rust never runs this -- they meet at the JSON in
-- `tests/fixtures/sprites/`.
--
-- The same art existing twice (`lua/distract/sprites/*.lua` and
-- `engine/src/sprites/*.rs`) with nothing comparing them is what made the
-- silhouette redo unsafe to start: three assets times two implementations is
-- six files free to drift the moment one is touched.
--
-- Regenerate the goldens after an intentional art change:
--   UPDATE_GOLDEN=1 cargo test --manifest-path engine/Cargo.toml --test sprite_parity

require("tests.test_harness")

local ASSETS = { "cat", "crab", "sun" }
local FIXTURE_DIR = "tests/fixtures/sprites"
local TRANSPARENT = "------"

-- The two ports cannot agree bit for bit: Lua computes in f64 and Rust in f32,
-- and `Canvas.set` floors its coordinates on both sides, so a coordinate
-- landing either side of an integer boundary throws a whole drawing step into
-- the adjacent pixel. A *tiny* precision difference therefore produces a
-- *large* colour difference, which is why a channel tolerance alone cannot
-- describe this and the neighbourhood rule below carries most of the weight.
local NEIGHBOURHOOD = 1

-- For the residual case the neighbourhood rule cannot reach: a difference
-- inside a smooth shading gradient, where the value drifts rather than moves.
-- No pixel needs this today. It is here because the neighbourhood rule is
-- structurally blind wherever an opaque layer is drawn over the difference, and
-- a harness that fails on precision drift is worse than no harness at all.
local CHANNEL_TOLERANCE = 24

-- Measured, not guessed. Regenerate the goldens, re-run, and update these
-- alongside any intentional art change.
--
-- `drifted` caps every pixel that differs at all, so a transcription error's
-- order-of-magnitude jump fails even when each individual pixel looks
-- explainable. `unexplained` caps the pixels no rule accounts for, and is the
-- number that matters.
--
-- sun's two unexplained pixels are both (7, 13), in `rising` frame 16 and
-- `setting` frame 21. `draw_horizon` cuts a gap at every `(x + row) % 7 == 0`
-- on its top row, identically on both sides; inside that one-pixel window the
-- overlay's f32 places the sun disc's lower edge and Lua's f64 does not. Every
-- adjacent pixel is covered by the opaque band, so the gold shade has nowhere
-- neighbouring to appear and the rule cannot see it. A third such pixel is a
-- real divergence and must fail.
local BUDGET = {
  cat = { drifted = 39, unexplained = 0 },
  crab = { drifted = 166, unexplained = 0 },
  sun = { drifted = 110, unexplained = 2 },
}

local function read_json(path)
  local fd = io.open(path, "r")
  assert(fd, "cannot open " .. path)
  local raw = fd:read("*a")
  fd:close()
  return vim.json.decode(raw)
end

local function split(text, separator)
  local pieces = {}
  for piece in string.gmatch(text, "([^" .. separator .. "]+)") do
    pieces[#pieces + 1] = piece
  end
  return pieces
end

local function encode_pixel(pixel)
  if not pixel then
    return TRANSPARENT
  end
  return string.format("%02x%02x%02x", pixel[1], pixel[2], pixel[3])
end

--- The golden's frames as `grid[frame][y][x]`, in the same 1-based indexing the
--- Lua matrices use.
local function decode_golden(golden)
  local grid = {}
  for frame_index, frame in ipairs(golden.frames) do
    grid[frame_index] = {}
    for y, row in ipairs(split(frame, ";")) do
      grid[frame_index][y] = split(row, ",")
    end
  end
  return grid
end

--- The Lua generator's frames in the same shape and encoding.
local function encode_lua(sprite_module)
  local grid = {}
  for frame_index, matrix in ipairs(sprite_module.frames()) do
    grid[frame_index] = {}
    for y = 1, sprite_module.height do
      local row = {}
      for x = 1, sprite_module.width do
        row[x] = encode_pixel(matrix[y][x])
      end
      grid[frame_index][y] = row
    end
  end
  return grid
end

local function appears_nearby(grid, frame_index, x, y, value)
  for dy = -NEIGHBOURHOOD, NEIGHBOURHOOD do
    local row = grid[frame_index][y + dy]
    if row then
      for dx = -NEIGHBOURHOOD, NEIGHBOURHOOD do
        if row[x + dx] == value then
          return true
        end
      end
    end
  end
  return false
end

local function channel_distance(want, got)
  if want == TRANSPARENT or got == TRANSPARENT then
    return math.huge
  end
  local widest = 0
  for channel = 0, 2 do
    local first = channel * 2 + 1
    local difference =
      math.abs(tonumber(want:sub(first, first + 1), 16) - tonumber(got:sub(first, first + 1), 16))
    if difference > widest then
      widest = difference
    end
  end
  return widest
end

--- Whether a differing pixel is accounted for by a known precision effect.
local function is_explained(want_grid, got_grid, frame_index, x, y)
  local want = want_grid[frame_index][y][x]
  local got = got_grid[frame_index][y][x]
  if appears_nearby(got_grid, frame_index, x, y, want) then
    return true
  end
  if appears_nearby(want_grid, frame_index, x, y, got) then
    return true
  end
  return channel_distance(want, got) <= CHANNEL_TOLERANCE
end

--- Every differing pixel in one asset, split by whether a rule accounts for it.
local function compare(golden, sprite_module)
  local want_grid = decode_golden(golden)
  local got_grid = encode_lua(sprite_module)
  local drifted, unexplained = 0, {}

  for frame_index = 1, #golden.frames do
    for y = 1, golden.height do
      for x = 1, golden.width do
        local want = want_grid[frame_index][y][x]
        local got = got_grid[frame_index][y][x]
        if want ~= got then
          drifted = drifted + 1
          if not is_explained(want_grid, got_grid, frame_index, x, y) then
            unexplained[#unexplained + 1] = string.format(
              "frame %d pixel (%d, %d): overlay draws %s, Lua draws %s",
              frame_index - 1,
              x,
              y,
              want,
              got
            )
          end
        end
      end
    end
  end

  return drifted, unexplained
end

describe("distract sprite art parity with the overlay engine", function()
  it("has a golden for every asset", function()
    for _, name in ipairs(ASSETS) do
      local golden = FIXTURE_DIR .. "/" .. name .. ".golden.json"
      assert(
        vim.fn.filereadable(golden) == 1,
        string.format(
          "no golden for %s. Generate with UPDATE_GOLDEN=1 cargo test "
            .. "--manifest-path engine/Cargo.toml --test sprite_parity",
          name
        )
      )
    end
  end)

  for _, name in ipairs(ASSETS) do
    describe(name, function()
      local golden = read_json(FIXTURE_DIR .. "/" .. name .. ".golden.json")
      local sprite_module = require("distract.sprites." .. name)

      it("draws the same canvas size as the overlay", function()
        assert.are_equal(golden.width, sprite_module.width)
        assert.are_equal(golden.height, sprite_module.height)
      end)

      it("draws the same number of frames as the overlay", function()
        assert.are_equal(#golden.frames, #sprite_module.frames())
      end)

      it("maps every state to the same frames as the overlay", function()
        assert.are.same(golden.layout, sprite_module.layout)
      end)

      it("reproduces the overlay's pixels within the measured drift", function()
        local budget = BUDGET[name]
        local drifted, unexplained = compare(golden, sprite_module)

        assert(
          #unexplained <= budget.unexplained,
          string.format(
            "%s: %d pixels differ for no known reason, budget is %d.\n  %s",
            name,
            #unexplained,
            budget.unexplained,
            table.concat(unexplained, "\n  ")
          )
        )
        assert(
          drifted <= budget.drifted,
          string.format(
            "%s: %d pixels drifted from the overlay, budget is %d. A jump this "
              .. "size is a transcription error, not f32 against f64",
            name,
            drifted,
            budget.drifted
          )
        )
      end)
    end)
  end
end)
