require("tests.test_harness")

--- Cross-engine voxel-meshing parity, Lua half.
---
--- `engine/tests/voxel_parity.rs` writes the goldens in `tests/fixtures/voxels/`
--- and asserts Rust still reproduces them. This asserts `lua/distract/voxel.lua`
--- reproduces them too, so the model a pet has in the terminal is the model it has
--- on the overlay.
---
--- **Exact, with no tolerance at all.** Unlike sprite and physics parity, nothing
--- here goes through a float computation whose width matters: a voxel coordinate
--- is a whole number or an exact half, a normal is one unit on one axis, and a
--- colour is a source byte copied through. Any difference is a real divergence,
--- and a tolerance would only hide one.
---
--- Each fixture carries its own source grid rather than meshing an asset's art,
--- because sprite art is only equal across the engines within a measured drift —
--- meshing each engine's own cat would compare two things already allowed to
--- differ.

local voxel = require("distract.voxel")

local FIXTURE_DIR = "tests/fixtures/voxels"

--- The goldens on disk, by name.
local function fixture_names()
  local names = {}
  for _, path in ipairs(vim.fn.glob(FIXTURE_DIR .. "/*.golden.json", false, true)) do
    table.insert(names, vim.fn.fnamemodify(path, ":t:r:r"))
  end
  table.sort(names)
  return names
end

local function read_golden(name)
  local path = string.format("%s/%s.golden.json", FIXTURE_DIR, name)
  local lines = vim.fn.readfile(path)
  return vim.json.decode(table.concat(lines, "\n"))
end

--- `"c82828"` as `{ 200, 40, 40 }`.
local function decode_colour(hex)
  return {
    tonumber(hex:sub(1, 2), 16),
    tonumber(hex:sub(3, 4), 16),
    tonumber(hex:sub(5, 6), 16),
  }
end

--- A golden's flat source list as the `matrix[row][col]` the mesher takes.
local function source_matrix(golden)
  local matrix = {}
  for row = 1, golden.source_rows do
    local pixels = {}
    for col = 1, golden.source_cols do
      local entry = golden.source[(row - 1) * golden.source_cols + col]
      -- `vim.json.decode` turns a JSON null into `vim.NIL`, which is truthy.
      pixels[col] = (entry ~= nil and entry ~= vim.NIL) and decode_colour(entry) or false
    end
    matrix[row] = pixels
  end
  return matrix
end

--- The same encoding `voxel_parity.rs` writes: whole coordinates as integers,
--- halves as they are, so the two sides produce identical strings.
local function coordinate(value)
  if value % 1 == 0 then
    return string.format("%d", value)
  end
  return tostring(value)
end

local function encode_vertex(vertex)
  local position = {}
  for axis = 1, 3 do
    position[axis] = coordinate(vertex.position[axis])
  end
  local normal = {}
  for axis = 1, 3 do
    normal[axis] = string.format("%d", vertex.normal[axis])
  end
  return string.format(
    "%s|%s|%02x%02x%02x",
    table.concat(position, ","),
    table.concat(normal, ","),
    vertex.colour[1],
    vertex.colour[2],
    vertex.colour[3]
  )
end

describe("voxel parity fixtures", function()
  it("finds the goldens the Rust harness writes", function()
    local names = fixture_names()
    assert.is_true(#names > 0, "no voxel goldens; run the Rust harness with UPDATE_GOLDEN=1")
  end)

  it("declares a source grid on every fixture", function()
    for _, name in ipairs(fixture_names()) do
      local golden = read_golden(name)
      assert.are_equal(
        golden.source_cols * golden.source_rows,
        #golden.source,
        name .. ": the source list does not fill the declared grid"
      )
    end
  end)
end)

for _, name in ipairs(fixture_names()) do
  describe("[" .. name .. "]", function()
    local golden = read_golden(name)
    local mesh = voxel.build(source_matrix(golden), {
      max_width = golden.max_width,
      depth = golden.depth,
    })

    it("fits the source to the same voxel grid", function()
      assert.are_equal(golden.extent[1], mesh.extent[1], "columns")
      assert.are_equal(golden.extent[2], mesh.extent[2], "rows")
      assert.are_equal(golden.extent[3], mesh.extent[3], "depth")
    end)

    it("emits the same number of faces", function()
      assert.are_equal(#golden.vertices, #mesh.vertices, "vertex count")
      assert.are_equal(#golden.indices, #mesh.indices, "index count")
    end)

    it("emits the same vertices, in the same order", function()
      for index, expected in ipairs(golden.vertices) do
        assert.are_equal(expected, encode_vertex(mesh.vertices[index]), "vertex " .. index)
      end
    end)

    it("addresses those vertices the same way", function()
      for index, expected in ipairs(golden.indices) do
        assert.are_equal(expected, mesh.indices[index], "index " .. index)
      end
    end)
  end)
end
