--- The Neovim highlight groups sprite colours are painted with, and a ceiling
--- on how many of them exist at once.
---
--- `nvim_set_hl` defines a global group and there is no call that removes one,
--- so every distinct colour pair ever drawn used to stay defined for the life
--- of the session -- 1,909 of them for the three built-in assets alone, and
--- unbounded once imported art arrives.
---
--- Groups are therefore owned. A group belongs to the asset that asked for it,
--- its name says so, and when the ceiling is reached the least recently drawn
--- asset's groups are cleared and whatever cached them is told to rebuild. An
--- asset currently being drawn is never evicted, so the eviction cannot thrash
--- against the frame it was triggered by.

local M = {}

--- Enough for every built-in asset at full palette, so nothing evicts in a
--- normal session; the cap exists for imported art, which has no natural bound.
M.DEFAULT_MAX_GROUPS = 4096

--- Owner for callers that are not drawing a particular asset.
M.SHARED_OWNER = "shared"

local owners = {}
local ordinal = 0
local total = 0
local max_groups = M.DEFAULT_MAX_GROUPS
local evict_handler = nil

local function sanitise(owner)
  return (tostring(owner):gsub("[^%w]", "_"))
end

local function hex(rgb)
  return rgb and string.format("%02x%02x%02x", rgb[1], rgb[2], rgb[3]) or "none"
end

local function owner_record(owner)
  local record = owners[owner]
  if not record then
    record = { names = {}, used_at = 0 }
    owners[owner] = record
  end
  ordinal = ordinal + 1
  record.used_at = ordinal
  return record
end

--- Clears every group an owner defined and forgets it.
---
--- The definitions cannot be deleted -- Neovim has no such call -- so they are
--- cleared, which releases their attributes and makes a stale reference render
--- as unstyled text rather than as the wrong colour.
---@param owner string
function M.release(owner)
  local record = owners[owner]
  if not record then
    return
  end
  for name, _ in pairs(record.names) do
    pcall(vim.api.nvim_set_hl, 0, name, {})
    total = total - 1
  end
  owners[owner] = nil
end

local function evict_one(protected_owner)
  local victim, victim_ordinal = nil, nil
  for owner, record in pairs(owners) do
    if owner ~= protected_owner and (victim_ordinal == nil or record.used_at < victim_ordinal) then
      victim, victim_ordinal = owner, record.used_at
    end
  end
  if not victim then
    return false
  end

  M.release(victim)
  if evict_handler then
    evict_handler(victim)
  end
  return true
end

--- Called with an owner whose groups have just been cleared, so whoever cached
--- rendered frames under that name can drop them.
---@param handler fun(owner: string)|nil
function M.on_evict(handler)
  evict_handler = handler
end

--- Sets the ceiling on live groups.
---@param opts table `{ max_groups = integer }`
function M.configure(opts)
  local requested = opts and opts.max_groups
  if requested == nil then
    return
  end
  if type(requested) ~= "number" or requested < 1 then
    error("distract.highlights: max_groups must be a positive number")
  end
  max_groups = math.floor(requested)
end

--- A highlight group painting `fg_rgb` on `bg_rgb`, created on first use.
---@param fg_rgb integer[]|nil
---@param bg_rgb integer[]|nil
---@param owner string|nil the asset the colours belong to
---@return string group_name
function M.group(fg_rgb, bg_rgb, owner)
  owner = owner or M.SHARED_OWNER
  local record = owner_record(owner)
  local name = string.format("Distract_%s_%s_%s", sanitise(owner), hex(fg_rgb), hex(bg_rgb))

  if record.names[name] then
    return name
  end

  while total >= max_groups do
    if not evict_one(owner) then
      break
    end
  end

  local options = {}
  if fg_rgb then
    options.fg = "#" .. hex(fg_rgb)
  end
  if bg_rgb then
    options.bg = "#" .. hex(bg_rgb)
  end
  vim.api.nvim_set_hl(0, name, options)

  record.names[name] = true
  total = total + 1
  return name
end

--- How many groups this module currently believes are defined.
---@return integer
function M.count()
  return total
end

--- Forgets every group.
---
--- `:colorscheme` runs `:hi clear`, which deletes them all; without this the
--- registry still claims they are defined and every sprite renders in the
--- default foreground until Neovim restarts.
function M.reset()
  owners = {}
  total = 0
  ordinal = 0
end

return M
