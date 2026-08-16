--- Locomotion classes and the manifest capability gate.
---
--- Mirrors the constants and `AssetManifest::validate_capabilities` in
--- `engine/src/manifest.rs`. It lives in its own module rather than in
--- `engine.lua` because both backends need it: the terminal engine checks a
--- manifest before it builds an entity, and `external.lua` checks one before it
--- puts it on the wire, so the same manifest is refused with the same words
--- whichever renderer is running. Requiring `engine.lua` from `external.lua`
--- would have worked, and would have made every overlay user pay for the
--- terminal sprite generation it pulls in.

local M = {}

M.GROUNDED = "grounded"
M.BALLISTIC = "ballistic"
M.OMNIDIRECTIONAL = "omnidirectional"

local CLASSES = { M.GROUNDED, M.BALLISTIC, M.OMNIDIRECTIONAL }

--- Paths that move y at most, and so do not need a floor-free state.
local FLOOR_SAFE_PATHS = { linear = true, sine = true }

--- A state's locomotion class, derived when the manifest omits it.
---
--- Mirrors `PhysicsConfig::effective_locomotion`. No manifest written before
--- the field existed sets it, so the derived value has to be the behaviour
--- those manifests already had: a floor when there is gravity to fall under,
--- free movement otherwise.
function M.effective_locomotion(phys)
  if phys.locomotion then
    return phys.locomotion
  end
  return (phys.gravity or 0) > 0 and M.GROUNDED or M.OMNIDIRECTIONAL
end

--- The locomotion class a state runs under, within its manifest.
---
--- Mirrors `AssetManifest::locomotion_for`. The state's own value wins, then
--- the asset-level default, then the derivation above. Without the asset-level
--- default every gravity-free state would derive `omnidirectional`, so a
--- walking cat would violate its own declaration.
function M.locomotion_for(manifest, state_def)
  local phys = (state_def and state_def.physics) or {}
  return phys.locomotion or manifest.locomotion or M.effective_locomotion(phys)
end

--- Checks every state against the asset's declared capabilities.
---
--- Mirrors `AssetManifest::validate_capabilities`. Returns the first violation
--- as a message, or nil when the manifest is sound. Run once at load rather
--- than per frame: a manifest that cannot work is worth one message when it
--- arrives, not thirty a second forever.
---
--- Permissive when `capabilities` is omitted, so no manifest written before
--- this existed can newly fail to load.
function M.validate(manifest)
  local names = vim.tbl_keys(manifest.states or {})
  -- Lua table order is arbitrary, and an error that names a different state on
  -- every run is an error nobody can reproduce.
  table.sort(names)

  local allowed = manifest.capabilities and manifest.capabilities.locomotion

  for _, name in ipairs(names) do
    local state_def = manifest.states[name]
    local phys = (state_def and state_def.physics) or {}
    local locomotion = M.locomotion_for(manifest, state_def)

    if not vim.tbl_contains(CLASSES, locomotion) then
      return string.format(
        "state '%s' declares an unknown locomotion '%s'; expected one of %s",
        name,
        locomotion,
        table.concat(CLASSES, ", ")
      )
    end

    if locomotion == M.OMNIDIRECTIONAL and (phys.gravity or 0) > 0 then
      return string.format(
        "state '%s' declares '%s' locomotion but sets gravity %s; gravity brings a "
          .. "floor with it, so the state would clamp to a floor it claims not to have",
        name,
        M.OMNIDIRECTIONAL,
        tostring(phys.gravity)
      )
    end

    if
      phys.path_type
      and not FLOOR_SAFE_PATHS[phys.path_type]
      and locomotion ~= M.OMNIDIRECTIONAL
    then
      return string.format(
        "state '%s' uses the '%s' path, which writes x directly and needs '%s' "
          .. "locomotion, but the state is '%s'",
        name,
        phys.path_type,
        M.OMNIDIRECTIONAL,
        locomotion
      )
    end

    if allowed and not vim.tbl_contains(allowed, locomotion) then
      return string.format(
        "state '%s' uses '%s' locomotion, which '%s' does not declare; "
          .. "capabilities.locomotion allows %s",
        name,
        locomotion,
        tostring(manifest.name),
        table.concat(allowed, ", ")
      )
    end
  end

  return nil
end

return M
