--- Procedurally drawn crab sprite.
---
--- Same pose-function approach as the cat: a handful of scalars (claw opening,
--- leg phase, eyestalk height, how far it has sunk into the sand) drive one
--- draw routine, and each state samples them along a curve.

local g = require("distract.sprite_gen")

local W, H = 24, 16

local SHELL = { 226, 62, 52 }
local SHELL_DARK = { 158, 30, 26 }
local CLAW = { 250, 116, 74 }
local LEG = { 176, 40, 34 }
local EYE_WHITE = { 248, 248, 252 }
local EYE_DARK = { 26, 24, 32 }
local SAND = { 198, 170, 122 }
local ZZZ = { 186, 214, 255 }

local sin, cos, pi, floor, max = math.sin, math.cos, math.pi, math.floor, math.max

--- Draws one crab pose.
--- pose fields:
---   leg     0..1 phase of the sideways scuttle
---   clamp   0..1 claws closed (0 wide open, 1 snapped shut)
---   raise   0..1 claws lifted overhead
---   stalk   0..1 eyestalks extended
---   eye     0..1 eye opening
---   sink    0..1 buried in sand
---   bob     -1..1 shell bob
---   zzz     0..1 sleep marks
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
  local cy = 8.4 + bob * 0.6 + sink * 4.0
  local shell_rx, shell_ry = 5.6, 3.4

  -- Legs: four per side, stepping in counter-phase.
  local function leg_at(hip_x, dir, phase)
    local swing = sin((leg + phase) * 2 * pi)
    local foot_x = hip_x + dir * (2.6 + swing * 1.3)
    local foot_y = cy + 3.6 + max(0, -swing) * 1.1
    g.limb(c, hip_x, cy + shell_ry * 0.5, foot_x, foot_y, 1.05, LEG)
  end

  if sink < 0.75 then
    for i = 0, 3 do
      local hip = cx - 3.4 + i * 2.2
      leg_at(hip, i < 2 and -1 or 1, i * 0.25)
    end
  end

  -- Claws: an upper and lower pincer whose gap closes as clamp goes to 1.
  local function claw_at(side)
    local base_x = cx + side * (shell_rx + 0.6)
    local base_y = cy - 0.4 - raise * 3.4
    local reach_x = base_x + side * 2.0
    -- Arm
    g.limb(c, cx + side * shell_rx * 0.7, cy - 0.2, base_x + side * 1.4, base_y, 1.2, SHELL)
    -- Pincer halves swing apart around the arm axis.
    local gap = (1 - clamp) * 2.6
    g.orb(c, reach_x, base_y - gap * 0.5 - 0.4, 2.2, 1.5, CLAW,
      { ambient = 0.46, rim = 0.26, specular = 0.32 })
    g.orb(c, reach_x, base_y + gap * 0.5 + 0.4, 2.2, 1.5, g.shade(CLAW, -0.16),
      { ambient = 0.46, rim = 0.20, specular = 0.24 })
  end
  claw_at(-1)
  claw_at(1)

  -- Shell, with a darker inner carapace band for depth.
  g.orb(c, cx, cy, shell_rx, shell_ry, SHELL, { ambient = 0.34, rim = 0.30, specular = 0.34 })
  g.orb(c, cx, cy + 0.5, shell_rx * 0.66, shell_ry * 0.52, SHELL_DARK,
    { ambient = 0.42, rim = 0.12, specular = 0.16 })

  -- Eyestalks rise out of the shell and carry the eyes.
  local function eyestalk(side)
    local sx = cx + side * 2.1
    local top = cy - shell_ry - 1.0 - stalk * 2.0
    g.limb(c, sx, cy - shell_ry * 0.6, sx, top + 0.6, 0.85, SHELL)
    if eye > 0.3 then
      -- A single-pixel pupil: at this size a wider one swallows the white and
      -- the eyestalk stops reading as an eye.
      g.orb(c, sx, top, 1.4, 1.4, EYE_WHITE, { ambient = 0.62, rim = 0.30, specular = 0.42 })
      g.set(c, sx, top, EYE_DARK)
    else
      g.line(c, sx - 1, top, sx + 1, top, g.shade(SHELL_DARK, -0.25))
    end
  end
  eyestalk(-1)
  eyestalk(1)

  -- Sand mound rises over the crab as it burrows.
  if sink > 0.05 then
    local mound_w = floor(4 + sink * 7)
    local mound_y = 13
    for row = 0, floor(sink * 3) do
      local half = mound_w - row * 2
      for dx = -half, half do
        g.set(c, cx + dx, mound_y - row,
          g.shade(SAND, -0.08 * row + 0.05 * cos(dx * 0.9)))
      end
    end
  end

  if zzz > 0.05 then
    local rise = floor(zzz * 4)
    for i = 0, 2 do
      local size = 3 - i
      local zy = cy - shell_ry - 4 - i * 2 + rise
      local zx = cx + 5 + i
      local tone = g.shade(ZZZ, -0.12 * i)
      g.line(c, zx, zy, zx + size, zy, tone)
      g.line(c, zx + size, zy, zx, zy + size, tone)
      g.line(c, zx, zy + size, zx + size, zy + size, tone)
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

-- Idle: at rest with the claws held open and low, legs planted. The open claws
-- and grounded stance are what separate this from the first frame of a walk.
add("idle", g.cycle(4, function(t)
  return {
    bob = sin(t * 2 * pi),
    clamp = 0.26 + 0.10 * sin(t * 2 * pi),
    raise = 0,
    stalk = 0.82 + 0.18 * sin(t * 2 * pi),
    leg = 0,
    eye = 1,
  }
end))

-- Scuttle: legs stepping, shell rocking with the gait, claws drawn in and up
-- out of the way. Starts mid-stride so it never opens on a planted pose.
add("walk", g.cycle(4, function(t)
  return {
    leg = t + 0.125,
    bob = 0.6 * sin(t * 4 * pi),
    clamp = 0.76,
    raise = 0.12,
    stalk = 0.95,
    eye = 1,
  }
end))

-- Fast scuttle: claws tucked in, body lower, harder rock.
add("walk_fast", g.cycle(4, function(t)
  return {
    leg = t,
    bob = 1.0 * sin(t * 4 * pi),
    clamp = 0.85,
    raise = 0.15,
    stalk = 0.6,
    eye = 1,
  }
end))

-- Clip: two snaps. `beat` alone repeats its values across the sequence, which
-- would make two frames identical, so the claws also climb steadily throughout.
add("clip_claws", g.sequence(4, function(t)
  local beat = math.abs(sin(t * 1.5 * pi))
  return {
    raise = 0.35 + t * 0.45 + 0.20 * beat,
    clamp = 1 - beat,
    stalk = 1 - t * 0.15,
    bob = 0.4 * beat,
    eye = 1,
  }
end))

-- Burrow: sinks into a rising sand mound, eyestalks retracting last. It starts
-- already breaking the sand so the opening pose cannot be mistaken for a walk.
add("burrow", g.sequence(5, function(t)
  local e = g.ease(t)
  return {
    sink = 0.14 + e * 0.86,
    stalk = 1 - e * 0.9,
    clamp = 0.4 + e * 0.6,
    raise = 0.30 * (1 - e),
    leg = t * 0.5,
    eye = 1 - e * 0.8,
  }
end))

-- Sleep: settled, eyes shut, claws slack, Zzz drifting up.
add("sleep", g.cycle(4, function(t)
  return {
    bob = 0.4 * sin(t * 2 * pi),
    clamp = 0.9,
    stalk = 0.18,
    eye = 0,
    zzz = t,
  }
end))

return { frames = frames, layout = layout, width = W, height = H }
