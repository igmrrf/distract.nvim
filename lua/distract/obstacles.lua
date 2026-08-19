--- Solid platforms and hazards a plugin registered.
---
--- A provider is a function the editor calls on a debounced cadence — never per
--- tick per entity, because a Tree-sitter query per frame is a performance trap
--- — and its rectangles are pushed to whichever engine is running. Neither
--- engine discovers its own: only the editor can run a query, read a fold or see
--- where a function header is, and an engine that went looking would reintroduce
--- the divergence class the physics parity harness exists to catch.
---
--- In terminal cells. `external.lua` converts to overlay pixels at the boundary,
--- as it does for the floor. The three resolution rules mirror
--- `engine/src/obstacles.rs` function for function.

local M = {}

M.SOLID_PLATFORM = "solid_platform"
M.HAZARD = "hazard"

local KINDS = { [M.SOLID_PLATFORM] = true, [M.HAZARD] = true }

--- The most obstacles that are kept.
---
--- The physics pass is per entity per obstacle per frame, so a query over a
--- large file is bounded here rather than trusted. Matches
--- `obstacles::MAX_OBSTACLES`.
local MAX_OBSTACLES = 128

local providers = {}

--- The rectangles last collected, in cells.
local collected = {}

--- Whether the count cap has already been reported this session.
local has_warned_about_cap = false

--- Registers a provider.
---
--- @param provider function `function(win_id, buf_id) -> table[]`, each entry
---   `{ x, y, width, height, type }` in terminal cells
--- @return integer the provider's id, for `unregister_provider`
function M.register_provider(provider)
  if type(provider) ~= "function" then
    error("distract.register_obstacle_provider: provider must be a function")
  end
  table.insert(providers, provider)
  return #providers
end

function M.unregister_provider(id)
  if providers[id] == nil then
    return false
  end
  table.remove(providers, id)
  return true
end

function M.provider_count()
  return #providers
end

--- For tests, and for a full plugin reload.
function M.reset()
  providers = {}
  collected = {}
  has_warned_about_cap = false
end

--- Whether a rectangle is usable, and refuses it loudly when it is not.
---
--- A provider returning a malformed rectangle is a bug in that plugin. It is
--- skipped with one message rather than being allowed to reach the physics pass,
--- where a nil width becomes an arithmetic error at 30 frames a second.
local function is_valid(rect, source)
  if type(rect) ~= "table" then
    return false, "an obstacle must be a table"
  end
  for _, field in ipairs({ "x", "y", "width", "height" }) do
    if type(rect[field]) ~= "number" then
      return false, string.format("obstacle field '%s' must be a number", field)
    end
  end
  if rect.width <= 0 or rect.height <= 0 then
    return false, "an obstacle must have a positive width and height"
  end
  if not KINDS[rect.type] then
    return false,
      string.format(
        "obstacle type must be '%s' or '%s', got '%s'",
        M.SOLID_PLATFORM,
        M.HAZARD,
        tostring(rect.type)
      )
  end
  return true, source
end

--- Calls every provider and validates what comes back.
---
--- A provider that errors is reported once per collection and skipped; the rest
--- still contribute, because one broken plugin should not remove the ground
--- every other plugin registered.
---@param context { win: integer, buf: integer }|nil
---@return table[] the accepted rectangles, in cells
function M.collect(context)
  context = context or {}
  local win = context.win or vim.api.nvim_get_current_win()
  local buf = context.buf or vim.api.nvim_win_get_buf(win)

  local accepted = {}
  for index, provider in ipairs(providers) do
    local ok, result = pcall(provider, win, buf)
    if not ok then
      vim.notify(
        string.format("[Distract] Obstacle provider #%d failed: %s", index, tostring(result)),
        vim.log.levels.WARN
      )
    else
      for _, rect in ipairs(result or {}) do
        local valid, reason = is_valid(rect)
        if valid then
          table.insert(accepted, {
            x = rect.x,
            y = rect.y,
            width = rect.width,
            height = rect.height,
            type = rect.type,
          })
        else
          vim.notify(
            string.format("[Distract] Obstacle provider #%d: %s", index, reason),
            vim.log.levels.WARN
          )
        end
      end
    end
  end

  if #accepted > MAX_OBSTACLES then
    if not has_warned_about_cap then
      has_warned_about_cap = true
      vim.notify(
        string.format(
          "[Distract] %d obstacles registered; only the first %d are used. "
            .. "Narrow the provider's query.",
          #accepted,
          MAX_OBSTACLES
        ),
        vim.log.levels.WARN
      )
    end
    accepted = vim.list_slice(accepted, 1, MAX_OBSTACLES)
  end

  collected = accepted
  return collected
end

--- The rectangles last collected, in cells.
function M.rects()
  return collected
end

--- Replaces the collected list directly. For the engines' own tests.
function M.set_rects(rects)
  collected = rects or {}
end

local function spans(obstacle, left, right)
  return obstacle.x < right and left < obstacle.x + obstacle.width
end

--- The platform a falling entity has just crossed, or nil.
---
--- Crossing is what counts, not overlapping: an entity resting on a platform has
--- its feet exactly on the top edge and keeps re-crossing it every frame as
--- gravity re-accelerates it, which is what holds it there. Something moving
--- upward through a platform crosses nothing, which is what makes a jump onto one
--- work.
---
--- The highest crossed edge wins, so falling past several platforms in one frame
--- lands on the first one reached.
---@param footprint { left: number, top: number, width: number, height: number }
---@param feet_before number where the feet were before this frame's integration
---@return number|nil the platform's top edge
function M.crossed_platform(rects, footprint, feet_before)
  local feet_after = footprint.top + footprint.height
  if feet_after < feet_before then
    return nil
  end

  local highest = nil
  local right = footprint.left + footprint.width
  for _, obstacle in ipairs(rects) do
    if
      obstacle.type == M.SOLID_PLATFORM
      and spans(obstacle, footprint.left, right)
      and feet_before <= obstacle.y
      and obstacle.y <= feet_after
      and (highest == nil or obstacle.y < highest)
    then
      highest = obstacle.y
    end
  end
  return highest
end

--- The surface a grounded entity stands on, given the floor it would use.
---
--- A grounded state has no gravity — a walking cat's `y` never changes on its own
--- — so which surface it stands on is resolved rather than integrated. Only
--- platforms between the entity's own top and its floor count: one above its head
--- is scenery, one below the floor is unreachable.
---@return number the surface's top edge
function M.standing_surface(rects, footprint, floor)
  local highest = floor
  local right = footprint.left + footprint.width
  for _, obstacle in ipairs(rects) do
    if
      obstacle.type == M.SOLID_PLATFORM
      and spans(obstacle, footprint.left, right)
      and obstacle.y >= footprint.top
      and obstacle.y <= floor
      and obstacle.y < highest
    then
      highest = obstacle.y
    end
  end
  return highest
end

--- Where a hazard puts an entity that has walked into it.
---
--- The side is decided by which edge the entity is nearer to, so one that arrived
--- from the left is returned to the left. An entity exactly centred on a hazard is
--- returned the way its heading says it came.
---@return table|nil `{ x = number, heading_x = number }`
function M.deflection(rects, footprint, heading_x)
  local right = footprint.left + footprint.width
  local feet = footprint.top + footprint.height
  for _, obstacle in ipairs(rects) do
    if
      obstacle.type == M.HAZARD
      and spans(obstacle, footprint.left, right)
      and obstacle.y < feet
      and footprint.top < obstacle.y + obstacle.height
    then
      local overlap_from_left = right - obstacle.x
      local overlap_from_right = (obstacle.x + obstacle.width) - footprint.left
      local came_from_left
      if overlap_from_left == overlap_from_right then
        came_from_left = heading_x >= 0
      else
        came_from_left = overlap_from_left < overlap_from_right
      end

      if came_from_left then
        return { x = obstacle.x - footprint.width, heading_x = -1 }
      end
      return { x = obstacle.x + obstacle.width, heading_x = 1 }
    end
  end
  return nil
end

return M
