--- Procedural sprite generator.
---
--- Sprites are drawn, not hand-authored. A canvas is a grid of RGB triples (or
--- `false` for transparent) which `distract.terminal_sprites` turns into
--- half-block rows. Drawing them means animation can be produced by sampling a
--- pose function, so a state's frames are smooth by construction rather than by
--- getting each hand-drawn frame right by eye.
---
--- Two shading models build volume from a flat base colour: `orb` is
--- continuous Lambertian shading (the sun's smooth disc), `cel_orb` quantises
--- that same lighting into flat shadow/base/highlight bands with a hard
--- outline (the cat and crab's cartoon look).
---
--- This is a line-by-line port of `engine/src/sprite_gen.rs`; the two must be
--- kept in parity by hand, since nothing enforces it at compile time.

local M = {}

local floor, sqrt, max, min = math.floor, math.sqrt, math.max, math.min
local abs = math.abs

--- Default key light: above, slightly to the entity's left, angled toward the
--- viewer. Shared by every asset so they look lit by the same source.
M.DEFAULT_LIGHT = { -0.5, -0.62, 0.6 }

--- Bayer 4x4 ordered dithering matrix normalised to -0.5 .. 0.5.
local BAYER_4X4 = {
  { -0.46875, 0.03125, -0.34375, 0.15625 },
  { 0.28125, -0.21875, 0.40625, -0.09375 },
  { -0.28125, 0.21875, -0.40625, 0.09375 },
  { 0.46875, -0.03125, 0.34375, -0.15625 },
}

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
  local matrix = {}
  for y = 1, c.h do
    local row = {}
    for x = 1, c.w do
      row[x] = c.rows[y][x] or false
    end
    matrix[y] = row
  end
  return matrix
end

local function clamp8(v)
  return max(0, min(255, floor(v + 0.5)))
end

--- Darkens (`amount` < 0) or lightens (`amount` > 0) a colour. `amount` is
--- clamped to -1..1, where -1 is black and 1 is white.
function M.shade(color, amount)
  amount = max(-1, min(1, amount))
  if amount < 0 then
    local factor = 1 + amount
    return { clamp8(color[1] * factor), clamp8(color[2] * factor), clamp8(color[3] * factor) }
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

--- Flat-filled ellipse with a genuine one-pixel contour around it.
---
--- The silhouette primitive. At 24x16 a sprite is 24 columns by eight half-block
--- rows, and a five-term lighting model spends every one of them on gradient
--- nobody can see; a flat fill inside a dark outline is what actually reads, and
--- it collapses the number of distinct colours -- and so of Neovim highlight
--- groups -- an asset needs.
---
--- The contour is the *rim*: a pixel inside the ellipse whose four-neighbourhood
--- leaves it. Drawing it as two ellipses instead -- a contour disc with a smaller
--- fill disc inset -- looks equivalent and is not, because the radii quantise to
--- whole pixels: at a head-sized `rx = 2.4` the inset fill collapses to a single
--- plus and the head renders as a dark blob with a fur pixel in it. That is
--- exactly what the cat's head did.
function M.blob(c, cx, cy, rx, ry, fill, contour)
  rx, ry = max(rx, 0.5), max(ry, 0.5)
  local function is_inside(dx, dy)
    local nx, ny = dx / rx, dy / ry
    return nx * nx + ny * ny <= 1.0
  end

  for dy = -floor(ry), floor(ry) do
    for dx = -floor(rx), floor(rx) do
      if is_inside(dx, dy) then
        local on_rim = not is_inside(dx - 1, dy)
          or not is_inside(dx + 1, dy)
          or not is_inside(dx, dy - 1)
          or not is_inside(dx, dy + 1)
        M.set(c, cx + dx, cy + dy, on_rim and contour or fill)
      end
    end
  end
end

--- Bresenham line between two points, inclusive of both endpoints.
function M.line(c, x0, y0, x1, y1, color)
  x0, y0, x1, y1 = floor(x0), floor(y0), floor(x1), floor(y1)
  local dx, dy = abs(x1 - x0), -abs(y1 - y0)
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

local function normalize(v)
  local len = sqrt(v[1] * v[1] + v[2] * v[2] + v[3] * v[3])
  if len == 0 then
    return { 0, 0, 1 }
  end
  return { v[1] / len, v[2] / len, v[3] / len }
end

--- Retrieves the Bayer dither offset for integer screen coordinate (x, y).
function M.dither(x, y, strength)
  strength = strength or 0.12
  local xi = (floor(x) % 4) + 1
  local yi = (floor(y) % 4) + 1
  return BAYER_4X4[yi][xi] * strength
end

--- Shaded ellipse, lit as a continuous hemisphere with multi-point lighting.
---
--- opts:
---   light        {x, y, z} direction the key light comes from (default DEFAULT_LIGHT)
---   ambient      floor brightness in shadow, 0..1 (default 0.34)
---   rim          strength of the grazing-angle rim light, 0..1 (default 0.30)
---   rim_color    colour of the rim light (default a cool white)
---   fill         strength of warm bounce fill light, 0..1 (default 0.15)
---   fill_color   colour of fill light (default { 255, 230, 200 })
---   specular     strength of the highlight, 0..1 (default 0.45)
---   shininess    specular exponent; higher is tighter (default 12)
---   dither       subtle Bayer ordered dithering strength (default 0)
---   flatten      0..1, blends the shading back toward flat (default 0)
function M.orb(c, cx, cy, rx, ry, base, opts)
  opts = opts or {}
  local light = normalize(opts.light or M.DEFAULT_LIGHT)
  local fill_dir = normalize(opts.fill_dir or { -light[1] * 0.7, 0.8, -light[3] * 0.5 })
  local ambient = opts.ambient or 0.34
  local rim_strength = opts.rim or 0.30
  local rim_color = opts.rim_color or { 220, 235, 255 }
  local fill_strength = opts.fill or 0.15
  local fill_color = opts.fill_color or { 255, 230, 200 }
  local spec_strength = opts.specular or 0.45
  local shininess = opts.shininess or 12
  local dither_strength = opts.dither or 0
  local flatten = opts.flatten or 0

  rx, ry = max(rx, 0.5), max(ry, 0.5)

  for dy = -floor(ry), floor(ry) do
    for dx = -floor(rx), floor(rx) do
      local nx, ny = dx / rx, dy / ry
      local r2 = nx * nx + ny * ny
      if r2 <= 1.0 then
        local nz = sqrt(max(0, 1 - r2))
        local diffuse = max(0, nx * light[1] + ny * light[2] + nz * light[3])
        local fill_diffuse = max(0, nx * fill_dir[1] + ny * fill_dir[2] + nz * fill_dir[3])

        local level = ambient + (1 - ambient) * diffuse
        if dither_strength > 0 then
          level = level + M.dither(cx + dx, cy + dy, dither_strength)
        end
        local color = M.shade(base, (level - 1) * 0.85)

        if fill_strength > 0 then
          color = M.mix(color, fill_color, fill_diffuse * fill_strength)
        end
        if rim_strength > 0 then
          local rim = (1 - nz) ^ 3
          color = M.mix(color, rim_color, rim * rim_strength)
        end
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

--- Shaded ellipse quantised into flat shadow/base/highlight bands with a hard
--- silhouette outline, for a cel-shaded/cartoon read.
---
--- opts:
---   light               direction the key light comes from (default DEFAULT_LIGHT)
---   shadow              flat shadow-band colour (default `base` darkened by 0.36)
---   highlight           flat highlight-band colour (default `base` lightened by 0.28)
---   outline             outline colour drawn at the silhouette edge; nil for no outline
---   outline_threshold   normalised radius^2 beyond which the outline is drawn (default 0.84)
---   rim                 strength of the grazing-angle rim light, 0..1 (default 0.0)
---   rim_color           colour of the rim light (default white)
function M.cel_orb(c, cx, cy, rx, ry, base, opts)
  opts = opts or {}
  local light = normalize(opts.light or M.DEFAULT_LIGHT)
  local shadow_color = opts.shadow or M.shade(base, -0.36)
  local highlight_color = opts.highlight or M.shade(base, 0.28)
  local outline_color = opts.outline
  local outline_threshold = opts.outline_threshold or 0.84
  local rim_strength = opts.rim or 0.0
  local rim_color = opts.rim_color or { 255, 255, 255 }

  rx, ry = max(rx, 0.5), max(ry, 0.5)

  for dy = -floor(ry), floor(ry) do
    for dx = -floor(rx), floor(rx) do
      local nx, ny = dx / rx, dy / ry
      local r2 = nx * nx + ny * ny
      if r2 <= 1.0 then
        if outline_color and r2 >= outline_threshold then
          M.set(c, cx + dx, cy + dy, outline_color)
        else
          local nz = sqrt(max(0, 1 - r2))
          local diffuse = nx * light[1] + ny * light[2] + nz * light[3]
          local pixel_color = base
          if diffuse > 0.42 then
            pixel_color = highlight_color
          elseif diffuse < -0.05 or ny > 0.35 then
            pixel_color = shadow_color
          end
          if rim_strength > 0 and nz < 0.35 and diffuse > -0.1 then
            pixel_color = M.mix(pixel_color, rim_color, rim_strength)
          end
          M.set(c, cx + dx, cy + dy, pixel_color)
        end
      end
    end
  end
end

--- Shaded capsule from (x0, y0) to (x1, y1), tapering toward the far end;
--- used for limbs and tails drawn with cel shading. `opts` is forwarded to
--- `cel_orb` unchanged at every step.
function M.cel_limb(c, x0, y0, x1, y1, radius, base, opts)
  opts = opts or {}
  local steps = max(1, floor(sqrt((x1 - x0) ^ 2 + (y1 - y0) ^ 2) * 2))
  for i = 0, steps do
    local t = i / steps
    local x = x0 + (x1 - x0) * t
    local y = y0 + (y1 - y0) * t
    local r = radius * (1 - 0.25 * t)
    M.cel_orb(c, x, y, r, r, base, opts)
  end
end

--- Flat capsule from (x0, y0) to (x1, y1) with a one-pixel contour.
---
--- The silhouette counterpart to `cel_limb`: two passes, so one step's contour
--- cannot be painted over the previous step's fill and leave a dark seam down the
--- middle of a leg.
function M.limb(c, x0, y0, x1, y1, radius, fill, contour)
  local steps = max(1, floor(sqrt((x1 - x0) ^ 2 + (y1 - y0) ^ 2) * 2))
  for pass = 1, 2 do
    local color = pass == 1 and contour or fill
    local inset = pass == 1 and 0 or 1
    for index = 0, steps do
      local t = index / steps
      local r = radius * (1 - 0.25 * t) - inset
      M.ellipse(c, x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, r, r, color)
    end
  end
end

--- Filled flat-colour triangle, used for angular details like ears.
function M.triangle(c, x0, y0, x1, y1, x2, y2, color)
  local min_x = floor(min(x0, min(x1, x2)))
  local max_x = floor(max(x0, max(x1, x2)))
  local min_y = floor(min(y0, min(y1, y2)))
  local max_y = floor(max(y0, max(y1, y2)))

  local function edge(ax, ay, bx, by, px, py)
    return (px - ax) * (by - ay) - (py - ay) * (bx - ax)
  end

  for y = min_y, max_y do
    for x = min_x, max_x do
      local w0 = edge(x1, y1, x2, y2, x + 0.5, y + 0.5)
      local w1 = edge(x2, y2, x0, y0, x + 0.5, y + 0.5)
      local w2 = edge(x0, y0, x1, y1, x + 0.5, y + 0.5)
      if (w0 >= 0 and w1 >= 0 and w2 >= 0) or (w0 <= 0 and w1 <= 0 and w2 <= 0) then
        M.set(c, x, y, color)
      end
    end
  end
end

--- 4-pointed micro sparkle / specular star, fading outward from its centre.
function M.spark(c, cx, cy, radius, color)
  cx, cy = floor(cx), floor(cy)
  color = color or { 255, 255, 255 }
  local r = floor(radius or 2)
  M.set(c, cx, cy, color)
  for d = 1, r do
    local fade = M.shade(color, -0.3 * d)
    M.set(c, cx + d, cy, fade)
    M.set(c, cx - d, cy, fade)
    M.set(c, cx, cy + d, fade)
    M.set(c, cx, cy - d, fade)
  end
end

--- Filled annulus between `inner_r` and `outer_r`, flat coloured.
function M.ring(c, cx, cy, inner_r, outer_r, color)
  local r_max = floor(outer_r)
  for dy = -r_max, r_max do
    for dx = -r_max, r_max do
      local dist = sqrt(dx * dx + dy * dy)
      if dist >= inner_r and dist <= outer_r then
        M.set(c, cx + dx, cy + dy, color)
      end
    end
  end
end

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
