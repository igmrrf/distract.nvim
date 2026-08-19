--- Software rasteriser for voxel models, for the in-terminal backends.
---
--- The overlay draws models on the GPU; this draws the *same* models — the ones
--- `distract.voxel` builds and `tests/voxel_parity_spec.lua` pins to the overlay's
--- — into the sprite canvas the half-block and kitty renderers already draw. A
--- backend that could not do 3D would fork the manifest contract, which is exactly
--- what the plan refused.
---
--- **The projection here is orthographic, and the overlay's is perspective.** That
--- is deliberate, not an omission. A whole model spans about thirty sprite pixels,
--- so a perspective divide across it moves nothing by a whole pixel — and an
--- orthographic projection makes the result independent of where on screen the pet
--- is, which is the only thing that makes the `(asset, frame, facing)` cache below
--- valid. Depth still decides which face is visible; it just does not scale.
---
--- **The canvas keeps the sprite's own size.** One asset has one cell footprint,
--- and physics, wrapping and floor anchoring all measure against it, so a model is
--- fitted into the sprite's canvas rather than given a canvas of its own. A yawed
--- model is at most 1.4% wider than its canvas at the worst angle, so the extreme
--- column can lose a sub-pixel sliver; changing the footprint to keep it would
--- change what the whole engine measures.

local M = {}

local render = require("distract.render")
local sources = require("distract.sprite_sources")
local voxel = require("distract.voxel")

--- Frames the terminal backends draw are one sprite pixel per canvas cell.
local TERMINAL_CAPABILITY = { native_resolution = false }

local settings = render.DEFAULTS
--- `asset|frame|facing` -> rasterised matrix. Invalidated whenever the settings
--- or the art change, because both change every pixel.
local cache = {}

--- Applies the render settings every rasterised frame is drawn under.
---@param new_settings table validated `render` settings
function M.configure(new_settings)
  settings = new_settings or render.DEFAULTS
  cache = {}
end

--- Drops rasterised frames, for one asset or for all of them.
---@param asset_name string|nil
function M.reset(asset_name)
  if not asset_name then
    cache = {}
    return
  end
  for key in pairs(cache) do
    if key:sub(1, #asset_name + 1) == asset_name .. "|" then
      cache[key] = nil
    end
  end
end

--- The turn a model is drawn at, in radians.
---
--- Facing is a yaw rather than a mirror, matching the overlay: mirroring would
--- swap which side the light falls on, so a pet turning round would appear to move
--- the sun.
---@param flip_x boolean
---@return number
function M.yaw_for(flip_x)
  local base = math.rad(settings.yaw_degrees or 0)
  if flip_x then
    return math.pi - base
  end
  return base
end

--- How bright a face pointing this way is, 0..1.
---
--- The same term the fragment shader runs: an ambient floor plus a Lambertian
--- response to one directional light.
local function shade_for(normal)
  local direction = render.light_direction(settings)
  local lambert = -(normal[1] * direction[1] + normal[2] * direction[2] + normal[3] * direction[3])
  if lambert < 0 then
    lambert = 0
  end
  local ambient = settings.light and settings.light.ambient or 0
  return ambient + (1 - ambient) * lambert
end

local function lit_colour(colour, shade)
  local lit = {}
  for channel = 1, 3 do
    local value = math.floor(colour[channel] * shade + 0.5)
    lit[channel] = math.max(0, math.min(255, value))
  end
  return lit
end

--- Yaws a model-space point about the model's own vertical axis.
local function yaw_point(point, sine, cosine)
  return {
    point[1] * cosine + point[3] * sine,
    point[2],
    -point[1] * sine + point[3] * cosine,
  }
end

--- The mesh's vertices, yawed and moved into canvas coordinates.
---
--- Canvas x runs 0..cols with the model centred, y runs 0..rows from the top, and
--- the third component is the view depth: larger is nearer the viewer.
local function project(mesh, yaw)
  local sine, cosine = math.sin(yaw), math.cos(yaw)
  local half_width = mesh.extent[1] * 0.5
  local projected = {}
  for index, vertex in ipairs(mesh.vertices) do
    local turned = yaw_point(vertex.position, sine, cosine)
    projected[index] = { turned[1] + half_width, turned[2], turned[3] }
  end
  return projected
end

--- Fills one triangle into the canvas, keeping whichever fragment is nearest.
---
--- Barycentric coverage at the cell's centre. A cell is one sprite pixel, so
--- there is nothing finer to sample and no antialiasing to do: the half-block
--- renderer has one colour per cell either way.
local function fill_triangle(canvas, corners, colour)
  local ax, ay, az = corners[1][1], corners[1][2], corners[1][3]
  local bx, by, bz = corners[2][1], corners[2][2], corners[2][3]
  local cx, cy, cz = corners[3][1], corners[3][2], corners[3][3]

  local area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
  if area == 0 then
    return
  end

  local left = math.max(1, math.floor(math.min(ax, bx, cx)) + 1)
  local right = math.min(canvas.cols, math.ceil(math.max(ax, bx, cx)))
  local top = math.max(1, math.floor(math.min(ay, by, cy)) + 1)
  local bottom = math.min(canvas.rows, math.ceil(math.max(ay, by, cy)))

  for row = top, bottom do
    local sample_y = row - 0.5
    for col = left, right do
      local sample_x = col - 0.5
      local weight_b = ((sample_x - ax) * (cy - ay) - (sample_y - ay) * (cx - ax)) / area
      local weight_c = ((bx - ax) * (sample_y - ay) - (by - ay) * (sample_x - ax)) / area
      local weight_a = 1 - weight_b - weight_c
      if weight_a >= 0 and weight_b >= 0 and weight_c >= 0 then
        local depth = weight_a * az + weight_b * bz + weight_c * cz
        local index = (row - 1) * canvas.cols + col
        if canvas.depth[index] == nil or depth > canvas.depth[index] then
          canvas.depth[index] = depth
          canvas.colour[index] = colour
        end
      end
    end
  end
end

--- Rasterises a mesh into a canvas of the given size.
---@param mesh table from `distract.voxel`
---@param yaw number radians
---@return table[] `matrix[row][col]` of `{r, g, b}` or `false`
function M.rasterise(mesh, yaw)
  local cols, rows = mesh.extent[1], mesh.extent[2]
  local canvas = { cols = cols, rows = rows, depth = {}, colour = {} }
  local projected = project(mesh, yaw)
  local sine, cosine = math.sin(yaw), math.cos(yaw)

  local shade_cache = {}
  local index = 1
  while index + 2 <= #mesh.indices do
    -- Both triangles of a quad share its first vertex, and every vertex of a
    -- face shares that face's normal and colour, so the shade is computed once
    -- per distinct normal rather than per fragment.
    local first = mesh.vertices[mesh.indices[index] + 1]
    local normal = yaw_point(first.normal, sine, cosine)
    local shade_key = string.format("%.4f|%.4f|%.4f", normal[1], normal[2], normal[3])
    local shade = shade_cache[shade_key]
    if not shade then
      shade = shade_for(normal)
      shade_cache[shade_key] = shade
    end

    fill_triangle(canvas, {
      projected[mesh.indices[index] + 1],
      projected[mesh.indices[index + 1] + 1],
      projected[mesh.indices[index + 2] + 1],
    }, lit_colour(first.colour, shade))
    index = index + 3
  end

  local matrix = {}
  for row = 1, rows do
    local pixels = {}
    for col = 1, cols do
      pixels[col] = canvas.colour[(row - 1) * cols + col] or false
    end
    matrix[row] = pixels
  end
  return matrix
end

--- One frame of an asset as a voxel model, rasterised into its sprite canvas.
---
--- Cached by `(asset, frame, facing)`, which is what makes this affordable: in the
--- steady state a 3D pet costs a table lookup per draw, exactly as a 2D one does.
---@param asset_name string
---@param frame_idx integer 1-based
---@param flip_x boolean
---@return table[]|nil matrix, or nil when the asset has no art
function M.matrix(asset_name, frame_idx, flip_x)
  local key = string.format("%s|%d|%s", asset_name, frame_idx, flip_x and "flipped" or "facing")
  local cached = cache[key]
  if cached then
    return cached
  end

  local frames = sources.get_pixel_frames(asset_name, TERMINAL_CAPABILITY)
  local source = frames and (frames[frame_idx] or frames[1])
  if not source then
    return nil
  end

  local mesh = voxel.build(source, {
    max_width = settings.voxel_max_width,
    depth = settings.voxel_depth,
  })
  if #mesh.indices == 0 then
    return nil
  end

  local matrix = M.rasterise(mesh, M.yaw_for(flip_x))
  cache[key] = matrix
  return matrix
end

return M
