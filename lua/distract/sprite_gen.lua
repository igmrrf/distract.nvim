--- Procedural sprite generator.
---
--- Sprites are drawn, not hand-authored. A canvas is a grid of RGB triples (or
--- `false` for transparent) which `distract.terminal_sprites` turns into
--- half-block rows. Drawing them means animation can be produced by sampling a
--- pose function, so a state's frames are smooth by construction rather than by
--- getting each hand-drawn frame right by eye.
---
--- Volume comes from `orb`, which shades an ellipse as if it were a lit
--- hemisphere: Lambert diffuse from a light direction, a rim term at grazing
--- angles, and a specular highlight. That is what gives flat pixel art its
--- rounded, three-dimensional read.

local M = {}

local floor, sqrt, max, min = math.floor, math.sqrt, math.max, math.min

-- Default key light: above, slightly to the entity's left, angled toward the
-- viewer. Shared by every asset so they look lit by the same source.
M.DEFAULT_LIGHT = { -0.5, -0.62, 0.6 }

-- =========================================================================
-- Canvas
-- =========================================================================

--- Creates a `w` x `h` fully transparent canvas.
function M.canvas(w, h)
  local rows = {}
  for y = 1, h do
    local row = {}
    for x = 1, w do
      row[x] = false
    end
    rows[y] = row
  end
  return { w = w, h = h, rows = rows }
end

--- Writes a pixel. Coordinates are 1-based; out-of-bounds writes are dropped.
function M.set(c, x, y, color)
  x, y = floor(x), floor(y)
  if x < 1 or y < 1 or x > c.w or y > c.h then
    return
  end
  c.rows[y][x] = color
end

--- Reads a pixel, or nil when it is transparent or out of bounds.
function M.get(c, x, y)
  x, y = floor(x), floor(y)
  if x < 1 or y < 1 or x > c.w or y > c.h then
    return nil
  end
  local px = c.rows[y][x]
  if px == false then
    return nil
  end
  return px
end

--- Converts a canvas into the row matrix `render_halfblock_frame` consumes.
--- Rows are dense and use `false` for transparent cells: a nil would truncate
--- the row and silently shrink the sprite.
function M.to_matrix(c)
  local m = {}
  for y = 1, c.h do
    local row = {}
    for x = 1, c.w do
      row[x] = c.rows[y][x] or false
    end
    m[y] = row
  end
  return m
end

-- =========================================================================
-- Colour
-- =========================================================================

local function clamp8(v)
  return max(0, min(255, floor(v + 0.5)))
end

--- Darkens (`amount` < 0) or lightens (`amount` > 0) a colour. `amount` is
--- clamped to -1..1, where -1 is black and 1 is white.
function M.shade(color, amount)
  amount = max(-1, min(1, amount))
  if amount < 0 then
    local k = 1 + amount
    return { clamp8(color[1] * k), clamp8(color[2] * k), clamp8(color[3] * k) }
  end
  return {
    clamp8(color[1] + (255 - color[1]) * amount),
    clamp8(color[2] + (255 - color[2]) * amount),
    clamp8(color[3] + (255 - color[3]) * amount),
  }
end

--- Linear interpolation between two colours, `t` in 0..1.
function M.mix(a, b, t)
  t = max(0, min(1, t))
  return {
    clamp8(a[1] + (b[1] - a[1]) * t),
    clamp8(a[2] + (b[2] - a[2]) * t),
    clamp8(a[3] + (b[3] - a[3]) * t),
  }
end

-- =========================================================================
-- Primitives
-- =========================================================================

--- Axis-aligned filled rectangle. Clips to the canvas.
function M.rect(c, x, y, w, h, color)
  for dy = 0, floor(h) - 1 do
    for dx = 0, floor(w) - 1 do
      M.set(c, x + dx, y + dy, color)
    end
  end
end

--- Filled ellipse centred on (cx, cy) with radii rx, ry.
function M.ellipse(c, cx, cy, rx, ry, color)
  rx, ry = max(rx, 0.5), max(ry, 0.5)
  for dy = -floor(ry), floor(ry) do
    for dx = -floor(rx), floor(rx) do
      local nx, ny = dx / rx, dy / ry
      if nx * nx + ny * ny <= 1.0 then
        M.set(c, cx + dx, cy + dy, color)
      end
    end
  end
end

--- Bresenham line between two points, inclusive of both endpoints.
function M.line(c, x0, y0, x1, y1, color)
  x0, y0, x1, y1 = floor(x0), floor(y0), floor(x1), floor(y1)
  local dx, dy = math.abs(x1 - x0), -math.abs(y1 - y0)
  local sx = x0 < x1 and 1 or -1
  local sy = y0 < y1 and 1 or -1
  local err = dx + dy

  while true do
    M.set(c, x0, y0, color)
    if x0 == x1 and y0 == y1 then
      break
    end
    local e2 = 2 * err
    if e2 >= dy then
      err = err + dy
      x0 = x0 + sx
    end
    if e2 <= dx then
      err = err + dx
      y0 = y0 + sy
    end
  end
end

-- =========================================================================
-- Volumetric shading
-- =========================================================================

local function normalize(v)
  local len = sqrt(v[1] * v[1] + v[2] * v[2] + v[3] * v[3])
  if len == 0 then
    return { 0, 0, 1 }
  end
  return { v[1] / len, v[2] / len, v[3] / len }
end

--- Shaded ellipse, lit as a hemisphere. This is what makes a sprite read as a
--- rounded volume instead of a flat blob.
---
--- opts:
---   light     {x, y, z} direction the light comes from (default DEFAULT_LIGHT)
---   ambient   floor brightness in shadow, 0..1 (default 0.34)
---   rim       strength of the grazing-angle rim light, 0..1 (default 0.30)
---   rim_color colour of the rim light (default a cool white)
---   specular  strength of the highlight, 0..1 (default 0.45)
---   shininess specular exponent; higher is tighter (default 12)
---   flatten   0..1, blends the shading back toward flat (default 0)
function M.orb(c, cx, cy, rx, ry, base, opts)
  opts = opts or {}
  local light = normalize(opts.light or M.DEFAULT_LIGHT)
  local ambient = opts.ambient or 0.34
  local rim_strength = opts.rim or 0.30
  local rim_color = opts.rim_color or { 220, 235, 255 }
  local spec_strength = opts.specular or 0.45
  local shininess = opts.shininess or 12
  local flatten = opts.flatten or 0

  rx, ry = max(rx, 0.5), max(ry, 0.5)

  for dy = -floor(ry), floor(ry) do
    for dx = -floor(rx), floor(rx) do
      local nx, ny = dx / rx, dy / ry
      local r2 = nx * nx + ny * ny
      if r2 <= 1.0 then
        -- Treat the disc as the silhouette of a hemisphere facing the viewer.
        -- The normal is (nx, ny, nz) in screen space, where +y points down, and
        -- `light` points from the surface toward the light source, so the
        -- Lambert term is a plain dot product of the two.
        local nz = sqrt(max(0, 1 - r2))
        local diffuse = max(0, nx * light[1] + ny * light[2] + nz * light[3])

        local level = ambient + (1 - ambient) * diffuse
        local color = M.shade(base, (level - 1) * 0.85)

        -- Rim light: strongest where the surface turns away from the viewer.
        if rim_strength > 0 then
          local rim = (1 - nz) ^ 3
          color = M.mix(color, rim_color, rim * rim_strength)
        end

        -- Specular: a tight highlight where the surface points at the light.
        if spec_strength > 0 then
          local spec = diffuse ^ shininess
          color = M.mix(color, { 255, 255, 255 }, spec * spec_strength)
        end

        if flatten > 0 then
          color = M.mix(color, base, flatten)
        end

        M.set(c, cx + dx, cy + dy, color)
      end
    end
  end
end

--- Shaded capsule along a horizontal axis; used for limbs and tails.
function M.limb(c, x0, y0, x1, y1, radius, base, opts)
  opts = opts or {}
  local steps = max(1, floor(sqrt((x1 - x0) ^ 2 + (y1 - y0) ^ 2) * 2))
  for i = 0, steps do
    local t = i / steps
    local x = x0 + (x1 - x0) * t
    local y = y0 + (y1 - y0) * t
    -- Taper slightly toward the far end so limbs do not read as pipes.
    local r = radius * (1 - 0.25 * t)
    M.orb(c, x, y, r, r, base, {
      light = opts.light,
      ambient = opts.ambient or 0.42,
      rim = opts.rim or 0.16,
      specular = opts.specular or 0.12,
      shininess = opts.shininess or 8,
    })
  end
end

-- =========================================================================
-- Pose sampling
-- =========================================================================

--- Samples `n` poses for a looping animation. `t` runs 0 .. (n-1)/n so the last
--- frame flows back into the first without repeating it.
function M.cycle(n, pose_fn)
  local poses = {}
  for i = 0, n - 1 do
    poses[i + 1] = pose_fn(i / n)
  end
  return poses
end

--- Samples `n` poses for a one-shot animation. `t` runs 0 .. 1 inclusive.
function M.sequence(n, pose_fn)
  local poses = {}
  if n == 1 then
    return { pose_fn(0) }
  end
  for i = 0, n - 1 do
    poses[i + 1] = pose_fn(i / (n - 1))
  end
  return poses
end

--- Renders a list of poses through a draw function into a list of matrices.
function M.render_poses(poses, draw_fn)
  local frames = {}
  for i, pose in ipairs(poses) do
    frames[i] = M.to_matrix(draw_fn(pose))
  end
  return frames
end

--- Smooth ease in/out over 0..1, for pose curves that should not start or stop
--- abruptly.
function M.ease(t)
  t = max(0, min(1, t))
  return t * t * (3 - 2 * t)
end

return M
