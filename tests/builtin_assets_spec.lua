require("tests.test_harness")

local distract = require("distract")
local engine = require("distract.engine")
local locomotion = require("distract.locomotion")
local renderer = require("distract.renderer")
local sprites = require("distract.terminal_sprites")

-- Every asset the plugin ships. Read from the plugin rather than listed here, so
-- a new built-in is covered the moment it is added: a manifest that names a
-- missing art file, or a state pointing past the end of its frame list, would
-- otherwise fail for the first user who spawned it rather than here.
local function builtin_names()
  local names = {}
  for _, name in ipairs(distract.get_asset_names()) do
    -- A spec that registered its own asset earlier in the run would otherwise be
    -- treated as a built-in; only the ones with a manifest module are.
    if pcall(require, "distract.manifests." .. name) then
      table.insert(names, name)
    end
  end
  return names
end

--- Binds every built-in's art, which is what a spawn does.
---
--- `sprite_sources` resolves a manifest's spritesheet on binding, not on setup, so
--- asking for an imported asset's frames before anything has spawned it reports the
--- procedural fallback -- 29 cat frames at 24x16 rather than the asset's own. That
--- fooled the first version of this suite into "reporting" a manifest bug in
--- `cat_walking`, whose 32 frames are correct.
local function bind_every_builtin()
  require("distract").setup({ backend = "halfblock" })
  for _, name in ipairs(builtin_names()) do
    sprites.bind_manifest(name, distract.config.assets[name])
  end
end

local function quietly(fn)
  local notify = vim.notify
  vim.notify = function() end
  local ok, err = pcall(fn)
  vim.notify = notify
  if not ok then
    error(err, 0)
  end
end

describe("built-in assets", function()
  it("ships more than the three procedural ones", function()
    local names = builtin_names()
    assert.is_true(#names >= 4, "expected the shipped asset set, got: " .. table.concat(names, ","))
  end)

  it("resolves a manifest for every one", function()
    bind_every_builtin()
    for _, name in ipairs(builtin_names()) do
      local manifest = distract.config.assets[name]
      assert.is_not_nil(manifest, name .. " has no manifest")
      assert.is_not_nil(manifest.initial_state, name .. " declares no initial_state")
      assert.is_not_nil(
        manifest.states[manifest.initial_state],
        string.format("%s's initial_state '%s' is not declared", name, manifest.initial_state)
      )
    end
  end)

  it("draws art for every one, at a footprint that fits a terminal", function()
    bind_every_builtin()
    for _, name in ipairs(builtin_names()) do
      local frames = sprites.get_pixel_frames(name, { native_resolution = false })
      assert.is_true(#frames > 0, name .. " produced no frames")

      local ok, width, height = pcall(sprites.get_dimensions, name)
      assert.is_true(ok, name .. " has no dimensions")
      -- Sprite pixels: one cell wide, half a cell tall. 48x64 is 48 columns by
      -- 32 rows, past what an 80x24 terminal can show at all.
      assert.is_true(
        width > 0 and width <= 48,
        string.format("%s is %d sprite pixels wide", name, width)
      )
      assert.is_true(
        height > 0 and height <= 64,
        string.format("%s is %d sprite pixels tall", name, height)
      )
    end
  end)

  it("points every state at frames that exist", function()
    bind_every_builtin()
    for _, name in ipairs(builtin_names()) do
      local manifest = distract.config.assets[name]
      local frame_count = #sprites.get_pixel_frames(name, { native_resolution = false })

      for state_name, definition in pairs(manifest.states) do
        local frames = definition.animation and definition.animation.frames
        assert.is_true(
          frames ~= nil and #frames > 0,
          string.format("%s state '%s' declares no frames", name, state_name)
        )
        for _, sheet_index in ipairs(frames) do
          assert.is_true(
            sheet_index >= 0 and sheet_index < frame_count,
            string.format(
              "%s state '%s' names frame %d of %d",
              name,
              state_name,
              sheet_index,
              frame_count
            )
          )
        end
      end
    end
  end)

  it("declares nothing its own capabilities refuse", function()
    for _, name in ipairs(builtin_names()) do
      local manifest = distract.config.assets[name]
      assert.is_nil(
        locomotion.validate(manifest),
        string.format(
          "%s is refused by its own capability gate: %s",
          name,
          tostring(locomotion.validate(manifest))
        )
      )
    end
  end)

  it("spawns, animates and draws every one", function()
    bind_every_builtin()
    for _, name in ipairs(builtin_names()) do
      engine.clear()
      quietly(function()
        engine.spawn(name)
      end)

      local entity = engine.get_entities()[1]
      assert.is_not_nil(entity, name .. " did not spawn")

      quietly(function()
        for _ = 1, 30 do
          engine.step(1 / 30, { columns = 120, lines = 40 })
        end
        engine.tick()
      end)

      assert.is_not_nil(
        renderer.window_state(entity.id),
        name .. " spawned but nothing was drawn for it"
      )
    end
    engine.clear()
  end)
end)
