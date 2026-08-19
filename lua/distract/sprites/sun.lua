local g = require("distract.sprite_gen")

local W, H = 16, 16

-- Flat, banded palette: a disc twelve pixels across cannot carry a gradient, and
-- the old per-pixel shading of the corona and the rays spent a distinct colour on
-- every radius.
local CORE = { 255, 246, 196 }
local SURFACE = { 255, 206, 62 }
local LIMB = { 236, 132, 22 }
local CORONA = { 255, 224, 132 }
local WHITE_HOT = { 255, 255, 240 }
local MOON = { 34, 32, 46 }
local HORIZON = { 96, 78, 128 }
local HORIZON_DEEP = { 66, 52, 94 }

local sin, cos, pi, floor, max, sqrt = math.sin, math.cos, math.pi, math.floor, math.max, math.sqrt

--- The corona: one band, not a gradient.
---
--- `g.shade` per pixel produced a distinct colour per radius, which is what made
--- three assets consume 46% of the highlight-group cap between them. One tone at
--- a wobbling edge reads the same at eight rows and costs one group.
local function draw_corona(c, cx, cy, radius, corona, spin)
  if corona <= 0.02 then
    return
  end
  local inner = radius + 0.4
  local outer = radius + 1.0 + corona * 3.2
  for y = 1, H do
    for x = 1, W do
      local dx, dy = x - cx, y - cy
      local d = sqrt(dx * dx + dy * dy)
      if d > inner and d <= outer then
        local ang = math.atan2(dy, dx)
        local edge = outer * (1 + 0.10 * sin(ang * 6 + spin * 2 * pi))
        if d <= edge then
          g.set(c, x, y, CORONA)
        end
      end
    end
  end
end

--- Eight rays, two tones, thick enough to survive eight half-block rows.
---
--- A one-pixel ray drawn in a per-step gradient disappeared entirely at sprite
--- size. Each is two pixels across for its inner half, one for its tip.
local function draw_rays(c, cx, cy, radius, rays, spin)
  if rays <= 0.05 then
    return
  end
  local inner = radius + 0.6
  local outer = inner + rays * 3.2
  for index = 0, 7 do
    local ang = (index / 8 + spin) * 2 * pi
    local ca, sa = cos(ang), sin(ang)
    local steps = max(1, floor((outer - inner) * 2))
    for step = 0, steps do
      local t = step / steps
      local rr = inner + (outer - inner) * t
      local tone = t < 0.55 and SURFACE or CORONA
      g.set(c, cx + ca * rr, cy + sa * rr, tone)
      if t < 0.5 then
        -- Thickened across the ray, not along it, so a ray reads as a spike
        -- rather than as a dotted line.
        g.set(c, cx + ca * rr - sa * 0.9, cy + sa * rr + ca * 0.9, tone)
      end
    end
  end
end

--- A clean disc: flat surface, a rim in the deeper tone, one bright core band.
local function draw_disc(c, cx, cy, radius, flare)
  g.blob(c, cx, cy, radius, radius, SURFACE, LIMB)
  g.ellipse(c, cx - radius * 0.18, cy - radius * 0.22, radius * 0.5, radius * 0.5, CORE)
  if flare > 0.35 then
    g.ellipse(c, cx, cy, radius * 0.22, radius * 0.22, WHITE_HOT)
  end
end

--- The eclipse silhouette, kept distinguishable from the shining pose.
---
--- The moon is flat and dark inside a bright rim, which is the one thing that
--- separates the two poses when both are a disc at eight rows.
local function draw_eclipse(c, cx, cy, radius, occlude)
  if occlude <= 0.02 then
    return
  end
  local mx = cx - radius * 2.2 + occlude * radius * 2.2
  g.blob(c, mx, cy, radius * 1.08, radius * 1.08, MOON, CORONA)
  if occlude > 0.82 then
    g.spark(c, cx + radius * 0.75, cy - radius * 0.75, 2, WHITE_HOT)
  end
end

--- The horizon band, two flat tones rather than a shaded ramp.
local function draw_horizon(c, horizon)
  if horizon <= 0.02 then
    return
  end
  local band_y = 13
  for row = 0, 2 do
    local tone = row == 0 and HORIZON or HORIZON_DEEP
    for x = 1, W do
      -- The gap in the top row is what makes the band read as a horizon rather
      -- than as a bar. Two of the three sprite-parity drift pixels live in it.
      if row > 0 or ((x + row) % 7) ~= 0 then
        g.set(c, x, band_y + row, tone)
      end
    end
  end
end

local function draw(pose)
  local c = g.canvas(W, H)
  local radius = pose.radius or 4.6
  local rays = pose.rays or 1
  local spin = pose.spin or 0
  local corona = pose.corona or 0
  local occlude = pose.occlude or 0
  local drop = pose.drop or 0
  local horizon = pose.horizon or 0
  local flare = pose.flare or 0

  local cx = 8
  local cy = 8 + drop * 3.4

  draw_corona(c, cx, cy, radius, corona, spin)
  draw_rays(c, cx, cy, radius, rays, spin)
  draw_disc(c, cx, cy, radius, flare)
  draw_eclipse(c, cx, cy, radius, occlude)
  draw_horizon(c, horizon)

  return c
end

local pose_sets = {}
local layout = {}
local frame_count = 0

local function add(state, poses)
  local start = frame_count
  pose_sets[#pose_sets + 1] = poses
  frame_count = frame_count + #poses
  local idx = {}
  for i = 0, #poses - 1 do
    idx[i + 1] = start + i
  end
  layout[state] = idx
end

add(
  "shining",
  g.cycle(4, function(t)
    return {
      radius = 3.6 + 0.25 * sin(t * 2 * pi),
      rays = 0.75 + 0.25 * sin(t * 2 * pi),
      spin = t / 8,
      corona = 0.18 + 0.10 * sin(t * 2 * pi),
    }
  end)
)

add(
  "eclipse",
  g.sequence(5, function(t)
    local e = g.ease(t)
    return {
      radius = 3.7,
      rays = 0.6 * (1 - e),
      corona = 0.15 + e * 0.85,
      occlude = e,
      spin = t / 12,
    }
  end)
)

add(
  "flare",
  g.sequence(4, function(t)
    local burst = sin(g.ease(t) * pi)
    return {
      radius = 3.5 + burst * 0.7,
      rays = 0.7 + burst * 0.3,
      corona = 0.2 + burst * 0.5,
      flare = burst,
      spin = t / 6,
    }
  end)
)

add(
  "rising",
  g.sequence(6, function(t)
    local e = g.ease(t)
    return {
      radius = 3.2 + e * 0.5,
      rays = e * 0.85,
      corona = 0.30 - e * 0.12,
      drop = 1 - e * 1.6,
      horizon = 1 - e * 0.35,
    }
  end)
)

add(
  "setting",
  g.sequence(6, function(t)
    local e = g.ease(t)
    return {
      radius = 3.7 - e * 0.5,
      rays = 0.85 * (1 - e),
      corona = 0.18 + e * 0.24,
      drop = -0.6 + e * 1.6,
      horizon = 0.65 + e * 0.35,
    }
  end)
)

local frames_cache = nil

local function frames()
  if not frames_cache then
    frames_cache = {}
    for _, poses in ipairs(pose_sets) do
      for _, matrix in ipairs(g.render_poses(poses, draw)) do
        frames_cache[#frames_cache + 1] = matrix
      end
    end
  end
  return frames_cache
end

return { frames = frames, layout = layout, width = W, height = H }
