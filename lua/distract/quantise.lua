--- Palette reduction for imported art.
---
--- The half-block renderer creates one Neovim highlight group per distinct
--- foreground/background pair it draws, and never gets one back. Procedural
--- sprites are drawn from a handful of tones, so that stayed merely untidy; a
--- GIF frame carries up to 256 colours and turns it into unbounded growth.
---
--- Median cut, weighted by how often each colour actually appears: the bucket
--- with the widest spread is split along its widest channel until the cap is
--- met, and every colour in a bucket becomes that bucket's weighted average.
--- Colours are sorted before anything is split, because `pairs` order varies
--- between runs and a quantiser that returns different art each time would
--- invalidate every cache keyed on the frame.

local M = {}

local CHANNELS = 3

local function colour_key(colour)
  return colour[1] * 65536 + colour[2] * 256 + colour[3]
end

--- Distinct colours and how many pixels wear each, in a stable order.
local function census(rows)
  local counts, entries = {}, {}
  for _, row in ipairs(rows) do
    for _, pixel in ipairs(row) do
      if pixel then
        local key = colour_key(pixel)
        local entry = counts[key]
        if entry then
          entry.weight = entry.weight + 1
        else
          entry = { colour = pixel, weight = 1, key = key }
          counts[key] = entry
          entries[#entries + 1] = entry
        end
      end
    end
  end

  table.sort(entries, function(left, right)
    return left.key < right.key
  end)
  return counts, entries
end

--- Widest channel of a bucket, and how wide it is.
local function widest_channel(bucket)
  local widest_index, widest_range = 1, -1
  for channel = 1, CHANNELS do
    local low, high = 255, 0
    for _, entry in ipairs(bucket) do
      local value = entry.colour[channel]
      low = math.min(low, value)
      high = math.max(high, value)
    end
    local range = high - low
    if range > widest_range then
      widest_index, widest_range = channel, range
    end
  end
  return widest_index, widest_range
end

--- The bucket worth splitting next, or nil when none can be split further.
local function pick_bucket(buckets)
  local chosen, chosen_range, chosen_channel = nil, 0, 1
  for _, bucket in ipairs(buckets) do
    if #bucket > 1 then
      local channel, range = widest_channel(bucket)
      if range > chosen_range then
        chosen, chosen_range, chosen_channel = bucket, range, channel
      end
    end
  end
  return chosen, chosen_channel
end

local function split(bucket, channel)
  table.sort(bucket, function(left, right)
    if left.colour[channel] == right.colour[channel] then
      return left.key < right.key
    end
    return left.colour[channel] < right.colour[channel]
  end)

  local middle = math.floor(#bucket / 2)
  local lower, upper = {}, {}
  for index, entry in ipairs(bucket) do
    if index <= middle then
      lower[#lower + 1] = entry
    else
      upper[#upper + 1] = entry
    end
  end
  return lower, upper
end

local function representative(bucket)
  local red, green, blue, weight = 0, 0, 0, 0
  for _, entry in ipairs(bucket) do
    red = red + entry.colour[1] * entry.weight
    green = green + entry.colour[2] * entry.weight
    blue = blue + entry.colour[3] * entry.weight
    weight = weight + entry.weight
  end
  return {
    math.floor(red / weight + 0.5),
    math.floor(green / weight + 0.5),
    math.floor(blue / weight + 0.5),
  }
end

local function build_buckets(entries, max_colours)
  local buckets = { entries }
  while #buckets < max_colours do
    local bucket, channel = pick_bucket(buckets)
    if not bucket then
      break
    end

    local lower, upper = split(bucket, channel)
    for index, candidate in ipairs(buckets) do
      if candidate == bucket then
        buckets[index] = lower
        break
      end
    end
    buckets[#buckets + 1] = upper
  end
  return buckets
end

--- Reduces a pixel matrix to at most `max_colours` distinct colours.
---
--- Transparent cells stay transparent: alpha is not a colour here, it is
--- whether the editor shows through.
---@param rows table<integer, table<integer, integer[]|false>>
---@param max_colours integer
---@return table<integer, table<integer, integer[]|false>>
function M.reduce(rows, max_colours)
  if type(max_colours) ~= "number" or max_colours < 1 then
    error("distract.quantise: max_colours must be at least 1")
  end

  local counts, entries = census(rows)
  if #entries <= max_colours then
    return rows
  end

  local replacement = {}
  for _, bucket in ipairs(build_buckets(entries, max_colours)) do
    local colour = representative(bucket)
    for _, entry in ipairs(bucket) do
      replacement[entry.key] = colour
    end
  end

  local reduced = {}
  for row_index, row in ipairs(rows) do
    local out = {}
    for column = 1, #row do
      local pixel = row[column]
      out[column] = pixel and replacement[counts[colour_key(pixel)].key] or false
    end
    reduced[row_index] = out
  end
  return reduced
end

return M
