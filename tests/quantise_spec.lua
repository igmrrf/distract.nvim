require("tests.test_harness")

local quantise = require("distract.quantise")

--- Distinct colours in a matrix, as `"r,g,b"` keys.
local function palette_of(rows)
  local seen, count = {}, 0
  for _, row in ipairs(rows) do
    for _, pixel in ipairs(row) do
      if pixel then
        local key = table.concat(pixel, ",")
        if not seen[key] then
          seen[key] = true
          count = count + 1
        end
      end
    end
  end
  return seen, count
end

--- A one-row matrix of `count` evenly spaced greys.
local function gradient(count)
  local row = {}
  for index = 1, count do
    local level = math.floor((index - 1) * 255 / (count - 1))
    row[index] = { level, level, level }
  end
  return { row }
end

describe("distract.quantise", function()
  it("leaves art that already fits the cap alone", function()
    local rows = { { { 10, 20, 30 }, { 40, 50, 60 }, false } }
    local reduced = quantise.reduce(rows, 8)

    assert.are.same({ 10, 20, 30 }, reduced[1][1])
    assert.are.same({ 40, 50, 60 }, reduced[1][2])
    assert.are_equal(false, reduced[1][3])
  end)

  it("brings a wide palette down to the cap", function()
    local reduced = quantise.reduce(gradient(64), 8)
    local _, count = palette_of(reduced)

    assert(count <= 8, "expected at most 8 colours, got " .. count)
    assert(count > 1, "expected the art to keep more than one colour")
  end)

  it("keeps every transparent cell transparent", function()
    local rows = { { { 1, 2, 3 }, false, { 250, 250, 250 } }, { false, false, { 9, 9, 9 } } }
    local reduced = quantise.reduce(rows, 2)

    assert.are_equal(false, reduced[1][2])
    assert.are_equal(false, reduced[2][1])
    assert.are_equal(false, reduced[2][2])
  end)

  it("keeps the matrix the same shape", function()
    local reduced = quantise.reduce(gradient(16), 4)

    assert.are_equal(1, #reduced)
    assert.are_equal(16, #reduced[1])
  end)

  it("splits two clusters into one colour each", function()
    local rows = {
      {
        { 0, 0, 0 },
        { 10, 10, 10 },
        { 240, 240, 240 },
        { 250, 250, 250 },
      },
    }
    local reduced = quantise.reduce(rows, 2)

    assert.are.same({ 5, 5, 5 }, reduced[1][1])
    assert.are.same({ 5, 5, 5 }, reduced[1][2])
    assert.are.same({ 245, 245, 245 }, reduced[1][3])
    assert.are.same({ 245, 245, 245 }, reduced[1][4])
  end)

  it("collapses to a single average at a cap of one", function()
    local rows = { { { 0, 0, 0 }, { 100, 100, 100 }, { 200, 200, 200 } } }
    local reduced = quantise.reduce(rows, 1)
    local _, count = palette_of(reduced)

    assert.are_equal(1, count)
    assert.are.same({ 100, 100, 100 }, reduced[1][1])
  end)

  it("is deterministic, since `pairs` order is not", function()
    local first = quantise.reduce(gradient(64), 5)
    local second = quantise.reduce(gradient(64), 5)

    for index = 1, 64 do
      assert.are.same(first[1][index], second[1][index])
    end
  end)

  it("refuses a cap below one rather than returning empty art", function()
    local ok = pcall(quantise.reduce, gradient(4), 0)
    assert.is_false(ok)
  end)
end)
