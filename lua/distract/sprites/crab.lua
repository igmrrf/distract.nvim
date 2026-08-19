local g = require("distract.sprite_gen")

local W, H = 24, 16

-- Flat, banded palette. Four shell tones did not read at 24x16 -- a carapace
-- twelve pixels across cannot carry a gradient -- and every distinct colour pair
-- is a Neovim highlight group. One fill, one shadow band, one light band, a
-- one-pixel contour and the accents that carry identity.
local CONTOUR = { 44, 14, 18 }
local SHELL = { 232, 54, 46 }
local SHELL_DARK = { 150, 24, 28 }
local SHELL_LIGHT = { 255, 132, 96 }
local CLAW = { 252, 108, 64 }
local CLAW_TOOTH = { 255, 246, 230 }
local EYE_WHITE = { 255, 255, 255 }
local EYE_DARK = { 24, 20, 32 }
local SAND = { 224, 192, 138 }
local SAND_DEEP = { 186, 154, 104 }
local ZZZ = { 176, 212, 255 }

-- The bands the old five-term model spent on gradient, aliased so every drawing
-- call still names the part of the crab it is drawing.
local SHELL_SPEC = SHELL_LIGHT
local SHELL_GROOVE = SHELL_DARK
local CLAW_DARK = SHELL_DARK
local CLAW_LIGHT = SHELL_LIGHT
local LEG = SHELL_DARK
local LEG_DARK = SHELL_DARK
local LEG_LIGHT = SHELL
local WHITE = EYE_WHITE
local SPARKLE = CLAW_TOOTH
local ZZZ_FADE = ZZZ

-- The rim is a darker shell tone rather than the near-black contour, for the same
-- reason the cat's is: a near-black outline merges into a dark editor background
-- and takes the silhouette's edge with it. CONTOUR is kept for the eye pupil and
-- the closed-eye line, which must read as holes.
local RIM = SHELL_DARK

local sin, cos, pi, floor, max = math.sin, math.cos, math.pi, math.floor, math.max

local function draw_legs(c, cx, cy, shell_ry, leg, sink)
  if sink >= 0.75 then
    return
  end
  for i = 0, 3 do
    local hip_x = cx - 3.4 + i * 2.2
    local dir = i < 2 and -1 or 1
    local phase = i * 0.25
    local swing = sin((leg + phase) * 2 * pi)
    local foot_x = hip_x + dir * (2.8 + swing * 1.3)
    local foot_y = cy + 3.6 + max(0, -swing) * 1.1
    -- Fill and contour are the same tone: a leg one pixel across cannot carry
    -- both, and the leg is already the darkest thing on the crab.
    g.limb(c, hip_x, cy + shell_ry * 0.5, foot_x, foot_y, 1.15, LEG_DARK, LEG_DARK)
  end
end

local function draw_claws(c, cx, cy, shell_rx, raise, clamp)
  local sides = { -1, 1 }
  for _, side in ipairs(sides) do
    local base_x = cx + side * (shell_rx + 0.6)
    local base_y = cy - 0.4 - raise * 3.4
    local reach_x = base_x + side * 2.2
    -- The arm is thin so the pincer at the end of it is the wide part.
    g.limb(
      c,
      cx + side * shell_rx * 0.7,
      cy - 0.2,
      base_x + side * 1.4,
      base_y,
      1.1,
      SHELL_DARK,
      SHELL_DARK
    )
    local gap = 1.2 + (1 - clamp) * 3.4
    -- Two prongs with daylight between them, which is the only way a pincer
    -- reads as a pincer at this size.
    g.blob(c, reach_x, base_y - gap * 0.5, 2.4, 1.3, CLAW, RIM)
    g.blob(c, reach_x, base_y + gap * 0.5, 2.4, 1.3, CLAW, RIM)
    g.set(c, reach_x + side * 1.6, base_y - gap * 0.3, CLAW_TOOTH)
    g.set(c, reach_x + side * 1.6, base_y + gap * 0.3, CLAW_TOOTH)
    if clamp > 0.85 then
      g.spark(c, reach_x + side * 1.2, base_y, 1, SPARKLE)
    end
  end
end

local function draw_eyestalks(c, cx, cy, stalk, eye)
  local sides = { -1, 1 }
  for _, side in ipairs(sides) do
    local sx = cx + side * 2.1
    local sy = cy - 2.8 - stalk * 1.6
    g.line(c, sx, cy - 1.2, sx, sy + 1.2, RIM)
    g.line(c, sx, cy - 1.0, sx, sy + 1.4, SHELL)
    if eye > 0.3 then
      g.blob(c, sx, sy, 1.6, 1.6, EYE_WHITE, RIM)
      g.set(c, sx, sy, EYE_DARK)
      g.set(c, sx + 0.4, sy - 0.4, WHITE)
    else
      g.line(c, sx - 1, sy, sx + 1, sy, CONTOUR)
    end
  end
end

local function draw_sand_and_sleep(c, cx, cy, sink, zzz)
  if sink > 0.05 then
    local mound_w = floor(4 + sink * 7)
    local mound_y = 13
    -- Two flat tones, not a per-pixel ramp: `g.shade` per column gave the mound
    -- a distinct colour per pixel, which is a highlight group per pixel.
    for row = 0, floor(sink * 3) do
      local half = mound_w - row * 2
      local tone = row == 0 and SAND or SAND_DEEP
      for dx = -half, half do
        g.set(c, cx + dx, mound_y - row, tone)
      end
    end
  end
  if zzz > 0.05 then
    local rise = floor(zzz * 4)
    for i = 0, 1 do
      local size = 2 - i
      local zy = cy - 4 - i * 2 + rise
      local zx = cx + 4 + i + rise * 0.5
      local tone = i == 0 and ZZZ or ZZZ_FADE
      g.line(c, zx, zy, zx + size, zy, tone)
      g.line(c, zx + size, zy, zx, zy + size, tone)
      g.line(c, zx, zy + size, zx + size, zy + size, tone)
    end
  end
end

local function draw(pose)
  local c = g.canvas(W, H)
  local leg = pose.leg or 0
  local clamp = pose.clamp == nil and 0.5 or pose.clamp
  local raise = pose.raise or 0
  local stalk = pose.stalk == nil and 1 or pose.stalk
  local eye = pose.eye == nil and 1 or pose.eye
  local sink = pose.sink or 0
  local bob = pose.bob or 0
  local zzz = pose.zzz or 0

  local cx = 12
  -- Seated low enough that the legs reach the bottom of the canvas: an asset's
  -- cell footprint is its whole canvas, so empty rows underneath would float the
  -- crab above the floor it is anchored to.
  local cy = 9.8 + bob * 0.6 + sink * 4.0
  local shell_rx, shell_ry = 5.6, 3.4

  draw_legs(c, cx, cy, shell_ry, leg, sink)
  draw_claws(c, cx, cy, shell_rx, raise, clamp)

  g.blob(c, cx, cy, shell_rx, shell_ry, SHELL, RIM)
  g.blob(c, cx, cy + 0.5, shell_rx * 0.66, shell_ry * 0.52, SHELL_DARK, RIM)

  g.set(c, cx - 2.0, cy - 1.2, SHELL_SPEC)
  g.set(c, cx - 1.0, cy - 1.4, WHITE)
  g.set(c, cx, cy - 1.4, WHITE)
  g.set(c, cx + 1.0, cy - 1.4, SHELL_SPEC)
  g.set(c, cx - 2.5, cy + 0.2, SHELL_GROOVE)
  g.set(c, cx + 2.5, cy + 0.2, SHELL_GROOVE)

  draw_eyestalks(c, cx, cy, stalk, eye)
  draw_sand_and_sleep(c, cx, cy, sink, zzz)

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
  "idle",
  g.cycle(4, function(t)
    return {
      bob = sin(t * 2 * pi),
      clamp = 0.26 + 0.10 * sin(t * 2 * pi),
      raise = 0,
      stalk = 0.82 + 0.18 * sin(t * 2 * pi),
      leg = 0,
      eye = 1,
    }
  end)
)

add(
  "walk",
  g.cycle(4, function(t)
    return {
      leg = t + 0.125,
      bob = 0.6 * sin(t * 4 * pi),
      clamp = 0.76,
      raise = 0.12,
      stalk = 0.95,
      eye = 1,
    }
  end)
)

add(
  "walk_fast",
  g.cycle(4, function(t)
    return {
      leg = t,
      bob = 1.0 * sin(t * 4 * pi),
      clamp = 0.85,
      raise = 0.15,
      stalk = 0.6,
      eye = 1,
    }
  end)
)

add(
  "clip_claws",
  g.sequence(4, function(t)
    local beat = math.abs(sin(t * 1.5 * pi))
    return {
      raise = 0.35 + t * 0.45 + 0.20 * beat,
      clamp = 1 - beat,
      stalk = 1 - t * 0.15,
      bob = 0.4 * beat,
      eye = 1,
    }
  end)
)

add(
  "burrow",
  g.sequence(5, function(t)
    local e = g.ease(t)
    return {
      sink = 0.14 + e * 0.86,
      stalk = 1 - e * 0.9,
      clamp = 0.4 + e * 0.6,
      raise = 0.30 * (1 - e),
      leg = t * 0.5,
      eye = 1 - e * 0.8,
    }
  end)
)

add(
  "sleep",
  g.cycle(4, function(t)
    return {
      bob = 0.4 * sin(t * 2 * pi),
      clamp = 0.9,
      stalk = 0.18,
      eye = 0,
      zzz = t,
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
