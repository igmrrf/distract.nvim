local g = require("distract.sprite_gen")

local W, H = 24, 16

local FUR = { 238, 142, 54 }
local FUR_DARK = { 164, 76, 24 }
local FUR_LIGHT = { 255, 186, 92 }
local FUR_SPEC = { 255, 214, 140 }
local CONTOUR = { 54, 28, 22 }
local BELLY = { 254, 246, 238 }
local BELLY_DARK = { 218, 202, 190 }
local BELLY_SHADOW = { 184, 168, 156 }
local PAW = { 255, 255, 255 }
local PAW_SHADOW = { 204, 196, 192 }
local NOSE = { 255, 140, 160 }
local EAR_INNER = { 255, 172, 188 }
local EAR_SHADOW = { 216, 128, 144 }
local EAR_LIGHT = { 255, 204, 216 }
local EYE = { 28, 24, 36 }
local EYE_LIT = { 64, 224, 172 }
local WHITE = { 255, 255, 255 }
local MOUTH = { 188, 54, 72 }
local MOUTH_DARK = { 132, 28, 44 }
local ZZZ = { 176, 212, 255 }
local ZZZ_FADE = { 140, 180, 235 }

local sin, pi, max, floor = math.sin, math.pi, math.max, math.floor

local function draw_tail(c, body_cx, body_cy, body_rx, curl, tail)
  local tail_base_x = body_cx - body_rx + 0.8
  for i = 1, 6 do
    local t = i / 6
    local curve = (0.55 + tail * 0.45) * t * t
    local tx = tail_base_x - t * (4.2 - curl * 1.6)
    local ty = body_cy - curve * (4.4 - curl * 2.6) + curl * 0.8
    local radius = 1.45 - t * 0.6
    g.cel_orb(c, tx, ty, radius, radius, FUR_DARK, {
      shadow = CONTOUR,
      highlight = FUR,
      outline = CONTOUR,
      outline_threshold = 0.82,
    })
  end
end

local function draw_legs(c, body_cx, body_cy, body_ry, base_y, lift, stretch, curl, leg)
  if curl >= 0.6 then
    return
  end
  local function leg_at(hip_x, phase)
    local swing = sin((leg + phase) * 2 * pi)
    local knee_x = hip_x + swing * (1.6 + stretch * 1.4)
    local foot_y = base_y + 2.4 - lift * 1.2 - curl * 2.2
    local lifted = max(0, sin((leg + phase) * 2 * pi + pi / 2)) * (0.9 + stretch * 0.8)
    g.cel_limb(c, hip_x, body_cy + body_ry * 0.6, knee_x, foot_y - lifted, 1.35, FUR, {
      shadow = FUR_DARK,
      highlight = FUR_LIGHT,
      outline = CONTOUR,
    })
    g.cel_orb(c, knee_x, foot_y - lifted, 1.5, 1.1, PAW, {
      shadow = PAW_SHADOW,
      highlight = WHITE,
      outline = CONTOUR,
    })
  end
  leg_at(body_cx - 3.2, 0.5)
  leg_at(body_cx + 3.0, 0.0)
  leg_at(body_cx - 1.6, 0.0)
  leg_at(body_cx + 4.4, 0.5)
end

local function draw_ears(c, head_cx, head_cy, head_r, stretch, mouth)
  local lean = -stretch * 0.3 + mouth * 0.2
  local ear_positions = { { head_cx - 1.8, -1.0 }, { head_cx + 1.6, 1.0 } }
  for _, pos in ipairs(ear_positions) do
    local ex, side = pos[1], pos[2]
    local top_x = ex + side * 0.6 + lean * 1.2
    local top_y = head_cy - head_r - 2.2
    g.triangle(
      c,
      ex - 1.2,
      head_cy - head_r + 0.4,
      ex + 1.2,
      head_cy - head_r + 0.4,
      top_x,
      top_y,
      CONTOUR
    )
    g.triangle(
      c,
      ex - 0.8,
      head_cy - head_r + 0.2,
      ex + 0.8,
      head_cy - head_r + 0.2,
      top_x,
      top_y + 0.4,
      FUR
    )
    g.triangle(
      c,
      ex - 0.4,
      head_cy - head_r,
      ex + 0.4,
      head_cy - head_r,
      top_x,
      top_y + 0.8,
      EAR_INNER
    )
    g.set(c, ex, head_cy - head_r + 0.3, EAR_SHADOW)
    g.set(c, ex + side * 0.3, top_y + 0.6, EAR_LIGHT)
  end
end

local function draw_eyes(c, head_cx, head_cy, eye)
  local eye_x_positions = { head_cx - 1.1, head_cx + 1.7 }
  for _, ex in ipairs(eye_x_positions) do
    if eye > 0.3 then
      g.set(c, ex, head_cy - 0.5, EYE)
      g.set(c, ex, head_cy - 1.5, EYE_LIT)
      g.set(c, ex + 0.5, head_cy - 1.5, WHITE)
    else
      g.line(c, ex - 1, head_cy - 0.8, ex + 1, head_cy - 0.8, CONTOUR)
    end
  end
end

local function draw_head(c, head_cx, head_cy, head_r, stretch, mouth, eye, curl)
  draw_ears(c, head_cx, head_cy, head_r, stretch, mouth)
  g.cel_orb(c, head_cx, head_cy, head_r, head_r * 0.94, FUR, {
    shadow = FUR_DARK,
    highlight = FUR_LIGHT,
    outline = CONTOUR,
    rim = 0.2,
    rim_color = FUR_SPEC,
  })
  g.cel_orb(c, head_cx + 1.1, head_cy + 1.3, 1.7, 1.1, BELLY, {
    shadow = BELLY_DARK,
    highlight = WHITE,
    outline = CONTOUR,
  })
  g.set(c, head_cx + 0.8, head_cy + 1.6, BELLY_SHADOW)
  draw_eyes(c, head_cx, head_cy, eye)
  g.set(c, head_cx + 1.1, head_cy + 0.7, NOSE)
  if curl < 0.6 then
    g.line(c, head_cx + 2.2, head_cy + 0.9, head_cx + 4.6, head_cy + 0.4, BELLY_DARK)
    g.line(c, head_cx + 2.2, head_cy + 1.5, head_cx + 4.6, head_cy + 2.0, BELLY_DARK)
  end
  if mouth > 0.04 then
    g.ellipse(
      c,
      head_cx + 1.2,
      head_cy + 1.7 + mouth * 0.5,
      0.8 + mouth * 0.8,
      0.5 + mouth * 1.0,
      MOUTH
    )
    g.set(c, head_cx + 1.2, head_cy + 1.8 + mouth * 0.5, MOUTH_DARK)
  end
end

local function draw_sleep(c, head_cx, head_cy, zzz)
  if zzz <= 0.05 then
    return
  end
  local rise = floor(zzz * 4)
  for i = 0, 1 do
    local size = 2 - i
    local zy = head_cy - 3 - i * 2 + rise
    local zx = head_cx + 3 + i + rise * 0.5
    local tone = i == 0 and ZZZ or ZZZ_FADE
    g.line(c, zx, zy, zx + size, zy, tone)
    g.line(c, zx + size, zy, zx, zy + size, tone)
    g.line(c, zx, zy + size, zx + size, zy + size, tone)
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

  local base_y = 12 - lift * 3 + curl * 1.5
  local body_cx = 10 + stretch * 0.8
  local body_cy = base_y - 2.6 + curl * 1.2
  local body_rx = 6.0 + stretch * 1.4 + curl * 1.0
  local body_ry = 3.4 - stretch * 0.5 - curl * 0.7

  draw_tail(c, body_cx, body_cy, body_rx, curl, tail)
  draw_legs(c, body_cx, body_cy, body_ry, base_y, lift, stretch, curl, leg)

  g.cel_orb(c, body_cx, body_cy, body_rx, body_ry, FUR, {
    shadow = FUR_DARK,
    highlight = FUR_LIGHT,
    outline = CONTOUR,
    rim = 0.25,
    rim_color = FUR_SPEC,
  })
  g.cel_orb(c, body_cx + 0.4, body_cy + body_ry * 0.45, body_rx * 0.68, body_ry * 0.44, BELLY, {
    shadow = BELLY_DARK,
    highlight = WHITE,
    outline = CONTOUR,
  })
  g.set(c, body_cx + 0.2, body_cy + body_ry * 0.8, BELLY_SHADOW)
  g.set(c, body_cx - 1.0, body_cy - body_ry + 0.8, FUR_SPEC)
  g.set(c, body_cx, body_cy - body_ry + 0.6, WHITE)

  local head_cx = body_cx + body_rx * 0.92 + stretch * 1.0
  local head_cy = body_cy - 3.4 + head_dip * 1.6 + curl * 2.0
  local head_r = 2.9

  draw_head(c, head_cx, head_cy, head_r, stretch, mouth, eye, curl)
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
