--- What each rendering backend can do, and how a requested name resolves.
---
--- This replaces the ad-hoc alias tables that lived in `init.lua`. A backend
--- registers what it is capable of; everything that has to degrade -- parallax
--- on a backend that cannot scale a sprite, an alias for a backend that does
--- not exist yet -- reads the table rather than naming the backend in an `if`.
--- The kitty backend then arrives as a `register` call rather than as another
--- special case.

local M = {}

M.HALFBLOCK = "halfblock"
M.OVERLAY = "overlay"

--- Capabilities of a rendering backend.
---
--- `scale` is whether the backend can draw a sprite at a size other than its
--- authored one. Parallax is derived from it rather than stored beside it: a
--- backend that cannot scale cannot show depth, and two fields that must agree
--- are two fields that can disagree.
---
--- `alpha` is the finest transparency the backend resolves -- `"cell"` for the
--- half-block renderer, whose smallest addressable unit is half a terminal
--- cell, `"pixel"` for a surface with a real alpha channel.
---@class DistractBackendCapabilities
---@field scale boolean
---@field alpha "cell"|"pixel"

---@type table<string, DistractBackendCapabilities>
local BUILT_IN_CAPABILITIES = {
  [M.HALFBLOCK] = { scale = false, alpha = "cell" },
  [M.OVERLAY] = { scale = true, alpha = "pixel" },
}

--- Names that resolve to a backend which is genuinely implemented.
local BUILT_IN_ALIASES = {
  halfblock = M.HALFBLOCK,
  tui = M.HALFBLOCK,
  terminal = M.HALFBLOCK,
  truecolor = M.HALFBLOCK,
  overlay = M.OVERLAY,
  external = M.OVERLAY,
  gpu = M.OVERLAY,
  wgpu = M.OVERLAY,
}

local capabilities = vim.deepcopy(BUILT_IN_CAPABILITIES)
local aliases = vim.deepcopy(BUILT_IN_ALIASES)

--- Names that no longer name a backend of their own, and what they became.
---
--- Both groups used to resolve to something that silently drew the wrong
--- thing. They resolve to halfblock now, and the substitution is reported.
local ART_BACKEND_REMOVED = "the ASCII backend was removed; sprites are truecolor pixel art now"
local KITTY_UNIMPLEMENTED = "the Kitty graphics protocol backend is not implemented yet"

local BUILT_IN_SUBSTITUTIONS = {
  float = { to = M.HALFBLOCK, why = ART_BACKEND_REMOVED },
  ascii = { to = M.HALFBLOCK, why = ART_BACKEND_REMOVED },
  lua = { to = M.HALFBLOCK, why = ART_BACKEND_REMOVED },
  window = { to = M.HALFBLOCK, why = ART_BACKEND_REMOVED },
  kitty = { to = M.HALFBLOCK, why = KITTY_UNIMPLEMENTED },
  ghostty = { to = M.HALFBLOCK, why = KITTY_UNIMPLEMENTED },
  wezterm = { to = M.HALFBLOCK, why = KITTY_UNIMPLEMENTED },
}

local substitutions = vim.deepcopy(BUILT_IN_SUBSTITUTIONS)

--- Warnings already issued, so a degradation is reported once rather than on
--- every spawn.
local warned = {}

--- Registers a backend and the aliases that should reach it.
---
--- A backend that registers stops being a substitution: `kitty` resolves to
--- itself once the kitty renderer registers under that name.
---@param name string canonical backend name
---@param caps DistractBackendCapabilities
---@param backend_aliases string[]|nil extra names resolving to `name`
function M.register(name, caps, backend_aliases)
  if type(name) ~= "string" or name == "" then
    error("distract.backends.register: name must be a non-empty string")
  end
  if type(caps) ~= "table" or type(caps.scale) ~= "boolean" or caps.alpha == nil then
    error("distract.backends.register: capabilities need `scale` and `alpha`")
  end

  capabilities[name] = { scale = caps.scale, alpha = caps.alpha }
  aliases[name] = name
  substitutions[name] = nil
  for _, alias in ipairs(backend_aliases or {}) do
    aliases[alias] = name
    substitutions[alias] = nil
  end
end

--- Backends that exist and can be selected, sorted.
---@return string[]
function M.names()
  local names = vim.tbl_keys(capabilities)
  table.sort(names)
  return names
end

--- What a backend can do, or nil when nothing is registered under that name.
---@param name string canonical backend name
---@return DistractBackendCapabilities|nil
function M.capabilities(name)
  local caps = capabilities[name]
  return caps and vim.deepcopy(caps) or nil
end

--- Whether a backend can show depth by scaling a sprite.
---@param name string canonical backend name
---@return boolean
function M.supports_parallax(name)
  local caps = capabilities[name]
  return caps ~= nil and caps.scale
end

--- Resolves a requested backend name or alias to one that exists.
---
--- Reports a substitution once per name rather than silently drawing something
--- else. An unknown name falls back to the half-block renderer, which is the
--- only backend that needs no build step.
---@param requested string|nil
---@param quiet boolean|nil suppress the substitution notice
---@return string canonical backend name
function M.resolve(requested, quiet)
  if not requested then
    return M.HALFBLOCK
  end
  local name = string.lower(vim.trim(requested))

  local substitute = substitutions[name]
  if not substitute then
    return aliases[name] or M.HALFBLOCK
  end

  if not quiet and not warned[name] then
    warned[name] = true
    vim.notify(
      string.format(
        "[Distract] Backend '%s' is unavailable: %s. Using '%s' instead.",
        name,
        substitute.why,
        substitute.to
      ),
      vim.log.levels.WARN
    )
  end
  return substitute.to
end

--- Reports, once, that a backend will honour draw order but not parallax.
---
--- A declared degradation rather than a silent divergence: the same `z` still
--- sorts sprites here, it just cannot make a distant one smaller or slower.
---@param name string canonical backend name
function M.warn_parallax_unsupported(name)
  local key = "parallax:" .. name
  if warned[key] then
    return
  end
  warned[key] = true
  vim.notify(
    string.format(
      "[Distract] Backend '%s' cannot scale sprites, so position.parallax is "
        .. "ignored there; `z` still sets draw order.",
      name
    ),
    vim.log.levels.WARN
  )
end

--- Clears the warned-once state. For tests.
function M.reset_warnings()
  warned = {}
end

--- Restores the built-in registry, discarding anything registered since.
--- For tests: the registry is process-wide, so one that registers a backend
--- would otherwise leave it visible to every spec that runs after it.
function M.reset()
  capabilities = vim.deepcopy(BUILT_IN_CAPABILITIES)
  aliases = vim.deepcopy(BUILT_IN_ALIASES)
  substitutions = vim.deepcopy(BUILT_IN_SUBSTITUTIONS)
  warned = {}
end

return M
