local g = require("distract.sprite_gen")

local W, H = 16, 16

local CORE = { 255, 246, 196 }
local SURFACE = { 255, 206, 62 }
local LIMB = { 255, 146, 26 }
local CORONA = { 255, 224, 132 }
local MOON = { 34, 32, 46 }
local HORIZON = { 92, 74, 124 }

local sin, cos, pi, floor, max, sqrt = math.sin, math.cos, math.pi, math.floor, math.max, math.sqrt

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
          local falloff = 1 - (d - inner) / max(0.001, edge - inner)
          g.set(c, x, y, g.shade(CORONA, -0.62 + falloff * 0.5 * corona))
        end
      end
    end
  end
end

local function draw_rays(c, cx, cy, radius, rays, spin)
  if rays <= 0.05 then
    return
  end
  local inner = radius + 0.7
  local outer = inner + rays * 3.4
  for i = 0, 7 do
    local ang = (i / 8 + spin) * 2 * pi
    local ca, sa = cos(ang), sin(ang)
    local steps = max(1, floor((outer - inner) * 2))
    for step = 0, steps do
      local t = step / steps
      local rr = inner + (outer - inner) * t
      g.set(c, cx + ca * rr, cy + sa * rr, g.shade(g.mix(SURFACE, CORONA, t), 0.10 - t * 0.30))
    end
  end
end

local function draw_disc(c, cx, cy, radius, flare)
  g.orb(c, cx, cy, radius, radius, SURFACE, {
    light = { 0, 0, 1 },
    ambient = 0.30 + flare * 0.35,
    rim = 0.55,
    rim_color = LIMB,
    specular = 0.0,
    dither = 0.06,
  })
  g.orb(c, cx, cy, radius * 0.55, radius * 0.55, g.shade(CORE, flare * 0.35), {
    light = { 0, 0, 1 },
    ambient = 0.62,
    rim = 0.20,
    rim_color = CORE,
    specular = 0.0,
  })
end

local function draw_eclipse(c, cx, cy, radius, occlude)
  if occlude <= 0.02 then
    return
  end
  local mx = cx - radius * 2.2 + occlude * radius * 2.2
  g.orb(c, mx, cy, radius * 1.02, radius * 1.02, MOON, {
    light = { -0.4, -0.4, 0.7 },
    ambient = 0.5,
    rim = 0.42,
    rim_color = CORONA,
    specular = 0.0,
  })
  if occlude > 0.82 then
    g.spark(c, cx + radius * 0.75, cy - radius * 0.75, 2, { 255, 255, 240 })
  end
end

local function draw_horizon(c, horizon)
  if horizon <= 0.02 then
    return
  end
  local band_y = 13
  for row = 0, 2 do
    local tone = g.shade(HORIZON, -0.12 * row + (1 - horizon) * 0.4)
    for x = 1, W do
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
