--- Voxel meshing: one sprite frame to one 3D model.
---
--- Mirrors `engine/src/voxel.rs` exactly, including the order faces and corners
--- are emitted in, because `tests/voxel_parity_spec.lua` asserts the two produce
--- byte-identical meshes from the same grid. A model that differs between the
--- backends is a pet that changes shape when the overlay opens.
---
--- Model space: one voxel is one unit, x right, y **down** to match the rest of
--- the engine, and the model is centred on x and z with `y = 0` at its top. A yaw
--- therefore turns the model about its own vertical axis without moving it.
---
--- A pixel becomes one box of the full slab depth rather than a stack of cubes:
--- the interior layers of a stack are never visible, and collapsing them is what
--- keeps a wide pet's model small enough to rasterise in Lua.

local M = {}

--- Widest voxel grid a frame is extruded at.
---
--- Matches `voxel::DEFAULT_MAX_WIDTH`. Art wider than this is resampled first,
--- exactly as `sprite_sources.TERMINAL_SPRITE_MAX_WIDTH` already fits art for the
--- half-block renderer.
M.DEFAULT_MAX_WIDTH = 48
--- Slab thickness, in voxels. Matches `voxel::DEFAULT_DEPTH`.
M.DEFAULT_DEPTH = 4

--- Unit normals, in the order `voxel::Face` declares them.
local FACE_FRONT = "front"
local FACE_BACK = "back"
local FACE_LEFT = "left"
local FACE_RIGHT = "right"
local FACE_TOP = "top"
local FACE_BOTTOM = "bottom"

local NORMALS = {
  [FACE_FRONT] = { 0, 0, 1 },
  [FACE_BACK] = { 0, 0, -1 },
  [FACE_LEFT] = { -1, 0, 0 },
  [FACE_RIGHT] = { 1, 0, 0 },
  [FACE_TOP] = { 0, -1, 0 },
  [FACE_BOTTOM] = { 0, 1, 0 },
}

--- The four corners of one face of the box spanning `min`..`max`.
---
--- The winding matches `Face::corners` in Rust corner for corner. It is not
--- arbitrary: the parity golden records vertices in emission order.
local function corners(face, min, max)
  local left, right = min[1], max[1]
  local top, bottom = min[2], max[2]
  local back, front = min[3], max[3]

  if face == FACE_FRONT then
    return {
      { left, top, front },
      { left, bottom, front },
      { right, bottom, front },
      { right, top, front },
    }
  elseif face == FACE_BACK then
    return {
      { right, top, back },
      { right, bottom, back },
      { left, bottom, back },
      { left, top, back },
    }
  elseif face == FACE_LEFT then
    return {
      { left, top, back },
      { left, bottom, back },
      { left, bottom, front },
      { left, top, front },
    }
  elseif face == FACE_RIGHT then
    return {
      { right, top, front },
      { right, bottom, front },
      { right, bottom, back },
      { right, top, back },
    }
  elseif face == FACE_TOP then
    return {
      { left, top, back },
      { left, top, front },
      { right, top, front },
      { right, top, back },
    }
  end
  return {
    { left, bottom, front },
    { left, bottom, back },
    { right, bottom, back },
    { right, bottom, front },
  }
end

--- A frame's opaque pixels on the voxel grid.
---
--- Nearest neighbour rather than the area average `resample.lua` uses for
--- sprites: voxel occupancy is a binary decision, and an area average puts a
--- partly covered pixel either side of a coverage threshold where f32 and f64
--- fall on opposite sides. The integer arithmetic here is what Rust runs too.
---@param matrix table[] `matrix[row][col]` of `{r, g, b}` or a falsy value
---@param max_width integer
---@return table grid `{ cols, rows, pixels }`, `pixels` indexed row-major, 1-based
function M.fit(matrix, max_width)
  local source_rows = #matrix
  local source_cols = source_rows > 0 and #matrix[1] or 0
  if source_cols == 0 or source_rows == 0 then
    return { cols = 0, rows = 0, pixels = {} }
  end

  local limit = math.max(1, math.floor(max_width or M.DEFAULT_MAX_WIDTH))
  local cols = math.max(1, math.min(source_cols, limit))
  local rows = source_cols == cols and source_rows
    or math.max(1, math.floor(source_rows * cols / source_cols))

  local pixels = {}
  for row = 0, rows - 1 do
    local source_row = math.min(math.floor(row * source_rows / rows), source_rows - 1)
    for col = 0, cols - 1 do
      local source_col = math.min(math.floor(col * source_cols / cols), source_cols - 1)
      local pixel = matrix[source_row + 1][source_col + 1]
      if pixel then
        pixels[row * cols + col + 1] = { pixel[1], pixel[2], pixel[3] }
      end
    end
  end

  return { cols = cols, rows = rows, pixels = pixels }
end

--- Whether a grid cell is solid. Off-grid is empty, which is what puts a face on
--- the silhouette.
---@param grid table
---@param col integer 0-based
---@param row integer 0-based
---@return boolean
function M.is_opaque(grid, col, row)
  if col < 0 or row < 0 or col >= grid.cols or row >= grid.rows then
    return false
  end
  return grid.pixels[row * grid.cols + col + 1] ~= nil
end

--- Which of a voxel's faces something else is not already hiding.
---
--- Front and back always show: the slab is one box deep, so nothing is behind
--- either of them.
local function exposed_faces(grid, col, row)
  local faces = { FACE_FRONT, FACE_BACK }
  if not M.is_opaque(grid, col - 1, row) then
    table.insert(faces, FACE_LEFT)
  end
  if not M.is_opaque(grid, col + 1, row) then
    table.insert(faces, FACE_RIGHT)
  end
  if not M.is_opaque(grid, col, row - 1) then
    table.insert(faces, FACE_TOP)
  end
  if not M.is_opaque(grid, col, row + 1) then
    table.insert(faces, FACE_BOTTOM)
  end
  return faces
end

local function push_quad(mesh, face, min, max, colour)
  local base = #mesh.vertices
  local normal = NORMALS[face]
  for _, position in ipairs(corners(face, min, max)) do
    table.insert(mesh.vertices, {
      position = position,
      normal = { normal[1], normal[2], normal[3] },
      colour = colour,
    })
  end
  for _, offset in ipairs({ 0, 1, 2, 0, 2, 3 }) do
    table.insert(mesh.indices, base + offset)
  end
end

--- Extrudes an already-fitted grid.
---@param grid table from `M.fit`
---@param depth integer slab thickness in voxels
---@return table mesh `{ vertices, indices, extent }`
function M.build_from_grid(grid, depth)
  depth = math.max(1, math.floor(depth or M.DEFAULT_DEPTH))
  local half_width = grid.cols * 0.5
  local half_depth = depth * 0.5
  local mesh = { vertices = {}, indices = {}, extent = { grid.cols, grid.rows, depth } }

  for row = 0, grid.rows - 1 do
    for col = 0, grid.cols - 1 do
      local colour = grid.pixels[row * grid.cols + col + 1]
      if colour then
        local min = { col - half_width, row, -half_depth }
        local max = { min[1] + 1, min[2] + 1, half_depth }
        for _, face in ipairs(exposed_faces(grid, col, row)) do
          push_quad(mesh, face, min, max, colour)
        end
      end
    end
  end

  return mesh
end

--- Extrudes one sprite frame into a mesh.
---@param matrix table[] `matrix[row][col]` of `{r, g, b}` or a falsy value
---@param options table|nil `{ max_width, depth }`
---@return table mesh `{ vertices, indices, extent }`
function M.build(matrix, options)
  options = options or {}
  local grid = M.fit(matrix, options.max_width or M.DEFAULT_MAX_WIDTH)
  return M.build_from_grid(grid, options.depth or M.DEFAULT_DEPTH)
end

--- How many quads a mesh holds. Two triangles, six indices, one quad.
---@param mesh table
---@return integer
function M.quad_count(mesh)
  return #mesh.indices / 6
end

return M
