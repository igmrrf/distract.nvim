--- Procedurally drawn cat sprite.
---
--- Every state is a pose function of a few scalars (body lift, leg phase, head
--- tilt, eye opening). Frames come from sampling those scalars, so animation is
--- smooth by construction and a new state is a new curve rather than a new set
--- of hand-drawn pixels.
---
--- `layout` maps state name to the 0-based frame indices in `frames`, and the
--- manifest references it directly so the two can never drift apart.

local g = require("distract.sprite_gen")

local W, H = 24, 16

local FUR = { 236, 142, 56 }
local FUR_DARK = { 176, 92, 28 }
local BELLY = { 252, 226, 196 }
local PAW = { 250, 244, 236 }
local NOSE = { 255, 154, 176 }
local EYE = { 38, 34, 46 }
local EYE_LIT = { 126, 232, 214 }
local ZZZ = { 186, 214, 255 }

local sin, pi, floor = math.sin, math.pi, math.floor

--- Draws one cat pose.
--- pose fields:
---   lift        0..1 body raised off the ground
---   leg         0..1 phase of the four-beat gait
---   stretch     0..1 body extended forward (sprint / mid-air)
---   head_dip    -1..1 head lowered (+) or raised (-)
---   eye         0..1 eye opening, 0 shut
---   mouth       0..1 mouth opening for the yawn
---   curl        0..1 curled up asleep
---   tail        -1..1 tail sway
---   zzz         0..1 sleep marks fading in
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

  -- Ground line drops as the body lifts; curling flattens the whole silhouette.
  local base_y = 12 - lift * 3 + curl * 1.5
  local body_cx = 10 + stretch * 0.8
  local body_cy = base_y - 2.6 + curl * 1.2
  local body_rx = 6.0 + stretch * 1.4 + curl * 1.0
  local body_ry = 3.4 - stretch * 0.5 - curl * 0.7

  -- Tail: sweeps back from the hip and curls upward. The horizontal reach is
  -- kept strictly leftward so the tail always clears the body silhouette
  -- instead of curling back inside it.
  local tail_base_x = body_cx - body_rx + 0.8
  for i = 1, 6 do
    local t = i / 6
    local curve = (0.55 + tail * 0.45) * t * t
    local tx = tail_base_x - t * (4.2 - curl * 1.6)
    local ty = body_cy - curve * (4.4 - curl * 2.6) + curl * 0.8
    g.orb(
      c,
      tx,
      ty,
      1.45 - t * 0.6,
      1.45 - t * 0.6,
      FUR_DARK,
      { ambient = 0.42, rim = 0.20, specular = 0.12 }
    )
  end

  -- Legs: two pairs in counter-phase, so the gait reads as a four-beat walk.
  local function leg_at(hip_x, phase)
    local swing = sin((leg + phase) * 2 * pi)
    local knee_x = hip_x + swing * (1.6 + stretch * 1.4)
    local foot_y = base_y + 2.4 - lift * 1.2 - curl * 2.2
    local lifted = math.max(0, sin((leg + phase) * 2 * pi + pi / 2)) * (0.9 + stretch * 0.8)
    g.limb(c, hip_x, body_cy + body_ry * 0.6, knee_x, foot_y - lifted, 1.35, FUR)
    g.orb(
      c,
      knee_x,
      foot_y - lifted,
      1.5,
      1.1,
      PAW,
      { ambient = 0.58, rim = 0.20, specular = 0.16 }
    )
  end

  if curl < 0.6 then
    leg_at(body_cx - 3.2, 0.5)
    leg_at(body_cx + 3.0, 0.0)
    leg_at(body_cx - 1.6, 0.0)
    leg_at(body_cx + 4.4, 0.5)
  end

  -- Body, then a lighter belly band to suggest a second surface.
  g.orb(c, body_cx, body_cy, body_rx, body_ry, FUR, { ambient = 0.36, rim = 0.26 })
  g.orb(
    c,
    body_cx + 0.4,
    body_cy + body_ry * 0.45,
    body_rx * 0.68,
    body_ry * 0.44,
    BELLY,
    { ambient = 0.52, rim = 0.10, specular = 0.14 }
  )

  -- Head sits forward of and above the body. It is deliberately smaller than
  -- the body and lifted clear of it, otherwise the two orbs merge into one
  -- loaf-shaped silhouette with no readable neck.
  local head_cx = body_cx + body_rx * 0.92 + stretch * 1.0
  local head_cy = body_cy - 3.4 + head_dip * 1.6 + curl * 2.0
  local head_r = 2.9

  -- Ears: triangles that taper to a point at the top and sit *on* the skull.
  -- The run widens as it descends -- widening upward would render a solid slab
  -- across the head -- and the base overlaps the head orb, because a gap there
  -- makes the pair read as antlers rather than ears.
  local EAR_HALF = { 0, 1, 1 }
  local function ear(ex, lean)
    for row = 0, 2 do
      local half = EAR_HALF[row + 1]
      for dx = -half, half do
        g.set(
          c,
          ex + dx + lean * (2 - row) * 0.35,
          head_cy - head_r - 1.1 + row,
          g.shade(FUR_DARK, -0.18 + row * 0.14)
        )
      end
      if row == 2 then
        g.set(c, ex, head_cy - head_r - 1.1 + row, NOSE)
      end
    end
  end
  ear(head_cx - 1.6, -1)
  ear(head_cx + 1.6, 1)

  g.orb(c, head_cx, head_cy, head_r, head_r * 0.94, FUR, {
    ambient = 0.38,
    rim = 0.30,
    fill = 0.16,
    dither = 0.08,
  })
  -- Muzzle
  g.orb(
    c,
    head_cx + 1.1,
    head_cy + 1.3,
    1.7,
    1.1,
    BELLY,
    { ambient = 0.56, rim = 0.14, specular = 0.20, fill = 0.12 }
  )

  -- Eyes: animated with catchlight and pupil
  local function eye_at(ex)
    if eye > 0.25 then
      g.set(c, ex, head_cy - 0.5, EYE)
      g.set(c, ex, head_cy - 1.5, eye > 0.7 and EYE_LIT or EYE)
      if eye > 0.7 then
        g.set(c, ex + 0.5, head_cy - 1.5, { 255, 255, 255 })
      end
    else
      g.line(c, ex - 1, head_cy - 0.8, ex + 1, head_cy - 0.8, g.shade(FUR_DARK, -0.40))
    end
  end
  eye_at(head_cx - 1.1)
  eye_at(head_cx + 1.7)

  -- Nose, and a mouth that opens for the yawn.
  g.set(c, head_cx + 1.1, head_cy + 0.7, NOSE)

  -- Whiskers: delicate vector whiskers extending from muzzle
  if curl < 0.6 then
    local wcolor = g.shade(BELLY, 0.15)
    g.line(c, head_cx + 2.2, head_cy + 0.9, head_cx + 4.6, head_cy + 0.4, wcolor)
    g.line(c, head_cx + 2.2, head_cy + 1.5, head_cx + 4.6, head_cy + 2.0, wcolor)
  end

  -- Threshold kept low so the mouth shrinks out of existence rather than
  -- popping off in one frame.
  if mouth > 0.04 then
    g.ellipse(
      c,
      head_cx + 1.2,
      head_cy + 1.7 + mouth * 0.5,
      0.6 + mouth * 0.8,
      0.4 + mouth * 1.0,
      { 122, 46, 62 }
    )
  end

  -- Sleep marks drift up and to the right as they fade in. The rise is scaled
  -- so each frame of the sleep cycle moves them a whole pixel; a smaller step
  -- would round two neighbouring frames to identical art.
  if zzz > 0.05 then
    local rise = floor(zzz * 4)
    for i = 0, 2 do
      local size = 3 - i
      local zy = head_cy - head_r - 2 - i * 2 + rise
      local zx = head_cx + 3 + i
      local tone = g.shade(ZZZ, -0.12 * i)
      g.line(c, zx, zy, zx + size, zy, tone)
      g.line(c, zx + size, zy, zx, zy + size, tone)
      g.line(c, zx, zy + size, zx + size, zy + size, tone)
    end
  end

  return c
end

-- =========================================================================
-- State curves
-- =========================================================================

local pose_sets = {}
local layout = {}
local frame_count = 0

--- Records a state's poses and its 0-based frame index range.
---
--- Poses are cheap to build; drawing them is not. Nothing is rasterised here so
--- that requiring this module — which every manifest does, and therefore so
--- does every Neovim startup — stays close to free. Frames are drawn on first
--- use by `frames()`.
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

-- Idle: slow breathing, a gentle tail sway, blinking held open.
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

-- Walk: four-beat gait with a slight body bob.
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

-- Sprint: body lower and longer, legs reaching further, tail streamed back.
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

-- Jump: crouch, launch, apex, fall, land. The crouch decays smoothly into the
-- sine arc rather than switching over at a threshold, which would put a jump
-- cut between the first two frames.
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

-- Yawn: mouth opens and closes while the eyes squeeze shut. The mouth arc alone
-- returns to the same value on the way down as on the way up, which would make
-- two frames identical, so the head tips and the tail sweeps monotonically
-- through the whole yawn to keep every frame distinct.
add(
  "yawn",
  g.sequence(5, function(t)
    local open = sin(g.ease(t) * pi)
    return {
      -- A yawn is a whole-body stretch, not just a mouth. Moving the body too
      -- keeps every frame doing something; with only the mouth animating, the
      -- frame where it finally shuts is the one big change in an otherwise
      -- static run and reads as a cut.
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

-- Sleep: curled, breathing, with Zzz drifting upward.
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

-- Drawn once, on first use.
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
