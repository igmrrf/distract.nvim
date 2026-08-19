local g = require("distract.sprite_gen")

local W, H = 24, 16

-- Flat, banded palette. At 24x16 a sprite is 24 columns by eight half-block rows,
-- and the five lighting terms this asset used to spend across a twelve-pixel body
-- read as noise rather than as form: the cat read as a fox. One fill, one shadow
-- band, one light band, a one-pixel contour and the few accents that carry
-- identity is what reads at this size -- and six colours per frame rather than
-- sixteen is what keeps the half-block renderer inside its highlight-group cap.
local CONTOUR = { 40, 26, 30 }
local FUR = { 236, 146, 60 }
local FUR_DARK = { 174, 96, 34 }
local BELLY = { 252, 240, 226 }
local EAR_INNER = { 240, 150, 168 }
local NOSE = { 236, 118, 142 }
local EYE = { 32, 28, 40 }
local ZZZ = { 168, 206, 250 }

-- The rim is a darker fur tone rather than the near-black contour. A near-black
-- outline is the right choice on a light page and the wrong one here: the editor
-- background is dark, so a dark rim merges into it and the silhouette loses its
-- edge -- the rendered cat looked like it had bites taken out of it. CONTOUR is
-- kept for the accents that must read as holes: eyes and an open mouth.
local RIM = FUR_DARK

local sin, pi, max, floor, abs = math.sin, math.pi, math.max, math.floor, math.abs

--- The tail: the cat's primary motion cue, so it is drawn thick enough to read.
---
--- Five segments, not six. The sixth drew nothing at all -- at `i = 6` its centre
--- landed off the canvas's left edge with a radius under a pixel, and any sliver
--- was already covered by the fifth.
---
--- Contours first, then fills, so one segment's outline cannot be painted over
--- the next segment's body and leave a dark seam down the tail.
local function draw_tail(c, body_cx, body_cy, body_rx, curl, tail)
  local base_x = body_cx - body_rx + 1.4
  local base_y = body_cy - 0.8
  local segments = {}
  for index = 1, 5 do
    local t = index / 5
    local rise = (0.5 + tail * 0.5) * t
    segments[index] = {
      x = base_x - t * (4.2 - curl * 3.0),
      y = base_y - rise * (4.6 - curl * 4.2) + curl * 2.0,
      r = 1.7 - t * 0.2,
    }
  end
  for _, segment in ipairs(segments) do
    g.ellipse(c, segment.x, segment.y, segment.r, segment.r, RIM)
  end
  for _, segment in ipairs(segments) do
    -- One pixel inside the contour, which at this radius is a plus rather than a
    -- square: a solid fill would leave the tail all outline and no fur.
    g.ellipse(c, segment.x, segment.y, segment.r - 1.0, segment.r - 1.0, FUR)
  end
end

--- Four legs in two distinguishable pairs.
---
--- The hind pair is short and thick under the haunch, the fore pair thinner and
--- longer under the chest, and they swing half a cycle apart. Four identical
--- capsules was the other half of why the silhouette read as a fox.
local function draw_legs(c, body_cx, body_cy, body_ry, base_y, lift, stretch, curl, leg)
  if curl >= 0.6 then
    return
  end

  local reach = 1.8 + stretch * 1.6
  -- The hip sits at the body's lower edge and the foot on the floor, so the legs
  -- are drawn *below* the barrel rather than inside it. They were inside it, which
  -- is why a rendered cat had no legs at all.
  local hip_y = body_cy + body_ry - 0.4

  local function leg_at(hip_x, width, phase)
    local cycle = (leg + phase) * 2 * pi
    local raise = max(0, sin(cycle + pi * 0.5)) * (0.7 + lift * 1.8)
    local foot_x = hip_x + sin(cycle) * reach
    local foot_y = base_y - raise
    local span = max(1, floor(foot_y - hip_y))

    for step = 0, span do
      local along = step / span
      g.rect(c, floor(hip_x + (foot_x - hip_x) * along), floor(hip_y + step), width, 1, FUR_DARK)
    end
    g.rect(c, floor(foot_x), floor(foot_y), width + 1, 1, BELLY)
  end

  leg_at(body_cx - 3.0, 2, 0.0)
  leg_at(body_cx - 1.4, 2, 0.5)
  leg_at(body_cx + 2.0, 2, 0.5)
  leg_at(body_cx + 3.4, 2, 0.0)
end

--- Two upright ears with a gap between them.
---
--- Three pixels wide and three tall, contoured, with one pink pixel inside. The
--- old pair were 2.4-pixel triangles that read as a single fuzzy line.
local function draw_ears(c, head_cx, head_cy, head_r, curl)
  local tuck = curl * 1.4
  for _, ex in ipairs({ head_cx - 2.2, head_cx + 1.8 }) do
    local base = head_cy - head_r * 0.8 + tuck
    local tip = base - 2.9 + tuck
    g.triangle(c, ex - 1.4, base, ex + 1.4, base, ex, tip, RIM)
    g.triangle(c, ex - 0.9, base - 0.7, ex + 0.9, base - 0.7, ex, tip + 1.1, FUR)
    g.set(c, ex, base - 1.5, EAR_INNER)
  end
end

local function draw_eyes(c, head_cx, head_cy, eye)
  for _, ex in ipairs({ head_cx - 0.8, head_cx + 1.4 }) do
    if eye > 0.3 then
      g.set(c, ex, head_cy - 0.4, EYE)
    else
      g.set(c, ex, head_cy - 0.4, CONTOUR)
      g.set(c, ex + 1, head_cy - 0.4, CONTOUR)
    end
  end
end

local function draw_head(c, head_cx, head_cy, head_r, mouth, eye, curl)
  draw_ears(c, head_cx, head_cy, head_r, curl)
  g.blob(c, head_cx, head_cy, head_r, head_r * 0.92, FUR, RIM)
  -- Muzzle: one light band, not a modelled snout. It is what tells the head
  -- which way it faces.
  g.ellipse(c, head_cx + 1.1, head_cy + 1.2, 1.2, 0.6, BELLY)
  g.set(c, head_cx + 2.0, head_cy + 0.9, NOSE)
  draw_eyes(c, head_cx, head_cy, eye)
  if mouth > 0.04 then
    g.ellipse(c, head_cx + 1.4, head_cy + 1.9, 0.6 + mouth * 0.7, 0.5 + mouth * 0.9, CONTOUR)
  end
end

local function draw_sleep(c, head_cx, head_cy, zzz)
  if zzz <= 0.05 then
    return
  end
  local rise = floor(zzz * 3)
  for index = 0, 1 do
    local size = 2 - index
    local zy = head_cy - 4 - index * 2 + rise
    local zx = head_cx + 3 + index + rise
    g.line(c, zx, zy, zx + size, zy, ZZZ)
    g.line(c, zx + size, zy, zx, zy + size, ZZZ)
    g.line(c, zx, zy + size, zx + size, zy + size, ZZZ)
  end
end

local function draw(pose)
  local c = g.canvas(W, H)
  local lift = pose.lift or 0
  local leg = pose.leg or 0
  local stretch = pose.stretch or 0
  local head_dip = pose.head_dip or 0
  local eye = pose.eye == nil and 1 or pose.eye
  local mouth = pose.mouth or 0
  local curl = pose.curl or 0
  local tail = pose.tail or 0
  local zzz = pose.zzz or 0

  -- Laid out so the whole canvas is used: ear tips on row 0, head above the
  -- shoulder, body across the middle, paws on the floor row. An asset's cell
  -- footprint is its whole canvas, so empty rows at the bottom would float the
  -- cat above the floor it is anchored to.
  local base_y = 15 - lift * 2.6 + curl * 1.2
  local body_cx = 9.0 + stretch * 0.6
  local body_cy = base_y - 5.4 + curl * 1.6
  local body_rx = 4.9 + stretch * 1.1 + curl * 0.9
  local body_ry = 2.3 - stretch * 0.25 - curl * 0.3

  draw_tail(c, body_cx, body_cy, body_rx, curl, tail)
  draw_legs(c, body_cx, body_cy, body_ry, base_y, lift, stretch, curl, leg)

  -- Haunch first, so the barrel's contour closes over where the two meet: a cat's
  -- rear is its most recognisable line after the ears and the tail.
  g.blob(c, body_cx - body_rx * 0.6, body_cy + 0.2, 2.5, 2.5, FUR, RIM)
  g.blob(c, body_cx, body_cy, body_rx, body_ry, FUR, RIM)
  -- One band, one row tall. A thicker one read as a cream stripe down a sausage
  -- rather than as a belly.
  g.ellipse(c, body_cx + 0.4, body_cy + body_ry - 1.2, body_rx * 0.5, 0.6, BELLY)

  local head_cx = body_cx + body_rx * 0.92 + stretch * 0.9
  local head_cy = body_cy - 4.6 + head_dip * 1.6 + curl * 3.2
  draw_head(c, head_cx, head_cy, 2.3, mouth, eye, curl)

  if curl >= 0.6 then
    draw_sleep(c, head_cx, head_cy, zzz)
  end

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
      lift = 0.04 + 0.04 * sin(t * 2 * pi),
      tail = sin(t * 2 * pi) * 0.8,
      head_dip = 0.10 * sin(t * 2 * pi),
      eye = 1,
    }
  end)
)

add(
  "walk",
  g.cycle(4, function(t)
    return {
      leg = t,
      lift = 0.10 + 0.08 * math.abs(sin(t * 2 * pi)),
      stretch = 0.12,
      tail = sin(t * 2 * pi) * 0.6,
      eye = 1,
    }
  end)
)

add(
  "walk_fast",
  g.cycle(4, function(t)
    return {
      leg = t,
      lift = 0.16 + 0.14 * math.abs(sin(t * 2 * pi)),
      stretch = 0.85,
      head_dip = 0.30,
      tail = 0.9 - 0.25 * sin(t * 2 * pi),
      eye = 1,
    }
  end)
)

add(
  "jump",
  g.sequence(8, function(t)
    local crouch = math.max(0, 1 - t / 0.34) ^ 2
    local arc = sin(math.max(0, (t - 0.12) / 0.88) * pi)
    return {
      lift = arc - crouch * 0.26,
      stretch = 0.30 + arc * 0.5 - crouch * 0.18,
      leg = 0.25 + arc * 0.2,
      head_dip = 0.22 * crouch - 0.45 * arc,
      tail = -0.5 + arc * 1.2,
      eye = 1,
    }
  end)
)

add(
  "yawn",
  g.sequence(5, function(t)
    local open = sin(g.ease(t) * pi)
    return {
      lift = 0.06 + 0.30 * open,
      stretch = 0.34 * open,
      leg = 0.12 * open,
      head_dip = 0.22 - 0.70 * t - 0.45 * open,
      mouth = open,
      eye = 1 - open * 0.95,
      tail = -0.45 + 1.3 * t + 0.3 * open,
    }
  end)
)

add(
  "sleep",
  g.cycle(4, function(t)
    return {
      curl = 1,
      lift = 0.02 * sin(t * 2 * pi),
      head_dip = 0.55 + 0.10 * sin(t * 2 * pi),
      eye = 0,
      tail = -0.6,
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
