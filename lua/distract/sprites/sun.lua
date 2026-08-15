--- Procedurally drawn sun sprite.
---
--- The disc is a lit sphere with the light aimed straight at the viewer, so it
--- reads as a glowing ball rather than a flat circle. Rays, corona, occluding
--- moon and horizon are all parameters of one draw routine.

local g = require("distract.sprite_gen")

local W, H = 16, 16

local CORE = { 255, 246, 196 }
local SURFACE = { 255, 206, 62 }
local LIMB = { 255, 146, 26 }
local CORONA = { 255, 224, 132 }
local MOON = { 34, 32, 46 }
local HORIZON = { 92, 74, 124 }

local sin, cos, pi, floor, max, sqrt = math.sin, math.cos, math.pi, math.floor, math.max, math.sqrt

--- Draws one sun pose.
--- pose fields:
---   radius    disc radius in pixels
---   rays      0..1 length of the emitted rays
---   spin      ray rotation in turns
---   corona    0..1 strength of the outer glow ring
---   occlude   0..1 how far the moon has crossed the disc
---   drop      -1..1 vertical offset, negative is higher in the sky
---   horizon   0..1 opacity of the horizon band
---   flare     0..1 brightness surge
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

  -- Corona: a radial falloff just outside the disc, evaluated per pixel. Drawn
  -- as a field rather than a ring of blobs, which would pile up into a lumpy
  -- mass thick enough to swallow the disc it is meant to surround.
  if corona > 0.02 then
    local inner = radius + 0.4
    local outer = radius + 1.0 + corona * 3.2
    for y = 1, H do
      for x = 1, W do
        local dx, dy = x - cx, y - cy
        local d = sqrt(dx * dx + dy * dy)
        if d > inner and d <= outer then
          -- Petal wobble keeps the glow from reading as a perfect circle.
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

  -- Rays: eight straight spokes, brightest at the disc and fading outward.
  if rays > 0.05 then
    local inner = radius + 0.7
    local outer = inner + rays * 3.4
    for i = 0, 7 do
      local ang = (i / 8 + spin) * 2 * pi
      local ca, sa = cos(ang), sin(ang)
      local steps = max(1, floor((outer - inner) * 2))
      for step = 0, steps do
        local t = step / steps
        local rr = inner + (outer - inner) * t
        g.set(c, cx + ca * rr, cy + sa * rr,
          g.shade(g.mix(SURFACE, CORONA, t), 0.10 - t * 0.30))
      end
    end
  end

  -- The disc itself: lit head-on so the falloff is radial, giving a sphere.
  g.orb(c, cx, cy, radius, radius, SURFACE, {
    light = { 0, 0, 1 },
    ambient = 0.30 + flare * 0.35,
    rim = 0.55,
    rim_color = LIMB,
    specular = 0.0,
  })
  -- Hot core.
  g.orb(c, cx, cy, radius * 0.55, radius * 0.55,
    g.shade(CORE, flare * 0.35), {
      light = { 0, 0, 1 },
      ambient = 0.62,
      rim = 0.20,
      rim_color = CORE,
      specular = 0.0,
    })

  -- Moon slides across from the left as occlude runs 0 -> 1.
  if occlude > 0.02 then
    local mx = cx - radius * 2.2 + occlude * radius * 2.2
    g.orb(c, mx, cy, radius * 1.02, radius * 1.02, MOON, {
      light = { -0.4, -0.4, 0.7 },
      ambient = 0.5,
      rim = 0.42,
      rim_color = CORONA,
      specular = 0.0,
    })
  end

  -- Horizon band for sunrise and sunset.
  if horizon > 0.02 then
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

  return c
end

local frames = {}
local layout = {}

local function add(state, poses)
  local start = #frames
  for _, matrix in ipairs(g.render_poses(poses, draw)) do
    frames[#frames + 1] = matrix
  end
  local idx = {}
  for i = 0, #poses - 1 do idx[i + 1] = start + i end
  layout[state] = idx
end

-- Shining: the disc breathes and the rays rotate a full step per cycle.
add("shining", g.cycle(4, function(t)
  return {
    radius = 3.6 + 0.25 * sin(t * 2 * pi),
    rays = 0.75 + 0.25 * sin(t * 2 * pi),
    spin = t / 8,
    corona = 0.18 + 0.10 * sin(t * 2 * pi),
  }
end))

-- Eclipse: the moon crosses the disc, the corona flares as totality arrives.
add("eclipse", g.sequence(5, function(t)
  local e = g.ease(t)
  return {
    radius = 3.7,
    rays = 0.6 * (1 - e),
    corona = 0.15 + e * 0.85,
    occlude = e,
    spin = t / 12,
  }
end))

-- Flare: a brightness surge with the rays thrown wide, then settling.
add("flare", g.sequence(4, function(t)
  local burst = sin(g.ease(t) * pi)
  return {
    radius = 3.5 + burst * 0.7,
    rays = 0.7 + burst * 0.3,
    corona = 0.2 + burst * 0.5,
    flare = burst,
    spin = t / 6,
  }
end))

-- Rising: climbs out of the horizon, rays lengthening as it clears.
add("rising", g.sequence(6, function(t)
  local e = g.ease(t)
  return {
    radius = 3.2 + e * 0.5,
    rays = e * 0.85,
    corona = 0.30 - e * 0.12,
    drop = 1 - e * 1.6,
    horizon = 1 - e * 0.35,
  }
end))

-- Setting: sinks back down, rays shortening and the band deepening.
add("setting", g.sequence(6, function(t)
  local e = g.ease(t)
  return {
    radius = 3.7 - e * 0.5,
    rays = 0.85 * (1 - e),
    corona = 0.18 + e * 0.24,
    drop = -0.6 + e * 1.6,
    horizon = 0.65 + e * 0.35,
  }
end))

return { frames = frames, layout = layout, width = W, height = H }
