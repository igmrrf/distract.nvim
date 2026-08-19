local M = {}

local floor, sqrt, max, min = math.floor, math.sqrt, math.max, math.min

M.DEFAULT_LIGHT = { -0.5, -0.62, 0.6 }

local BAYER_4X4 = {
  { -0.46875, 0.03125, -0.34375, 0.15625 },
  { 0.28125, -0.21875, 0.40625, -0.09375 },
  { -0.28125, 0.21875, -0.40625, 0.09375 },
  { 0.46875, -0.03125, 0.34375, -0.15625 },
}

local function clamp8(value)
  return max(0, min(255, floor(value + 0.5)))
end

M.clamp8 = clamp8

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

function M.mix(color_a, color_b, t)
  t = max(0, min(1, t))
  return {
    clamp8(color_a[1] + (color_b[1] - color_a[1]) * t),
    clamp8(color_a[2] + (color_b[2] - color_a[2]) * t),
    clamp8(color_a[3] + (color_b[3] - color_a[3]) * t),
  }
end

function M.normalize(vector)
  local length = sqrt(vector[1] * vector[1] + vector[2] * vector[2] + vector[3] * vector[3])
  if length == 0 then
    return { 0, 0, 1 }
  end
  return { vector[1] / length, vector[2] / length, vector[3] / length }
end

function M.dither(x, y, strength)
  strength = strength or 0.12
  local xi = (floor(x) % 4) + 1
  local yi = (floor(y) % 4) + 1
  return BAYER_4X4[yi][xi] * strength
end

function M.ease(t)
  t = max(0, min(1, t))
  return t * t * (3 - 2 * t)
end

return M
