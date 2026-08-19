--- Pins the public API surface declared in `docs/plugin-api.md`.
---
--- A contract nothing checks is a promise. This spec is what makes the
--- stability policy enforceable: adding or removing anything on
--- `require("distract")` fails here, so a break has to be deliberate and
--- arrives with the version bump the contract requires.
---
--- If a change here is intentional, update `docs/plugin-api.md` in the same
--- commit and bump accordingly — removing or renaming anything in these lists
--- is a major.

require("tests.test_harness")

local distract = require("distract")

--- Every function `require("distract")` promises, and nothing else.
local PUBLIC_FUNCTIONS = {
  "action",
  "build",
  "clear",
  "download",
  "get_all_actions",
  "get_asset_names",
  "get_available_backends",
  "get_backend",
  "get_backend_capabilities",
  "get_plugin_names",
  "get_render",
  "is_overlay",
  "is_running",
  "register_asset",
  "register_obstacle_provider",
  "register_plugin",
  "set_backend",
  "set_render",
  "setup",
  "spawn",
  "start",
  "status",
  "stop",
  "unregister_obstacle_provider",
  "unregister_plugin",
}

--- The closed set of plugin hooks. An unknown key is refused, not ignored.
local PUBLIC_HOOKS = {
  "on_init",
  "on_tick",
  "on_state_change",
  "on_collision",
  "on_editor_event",
  "on_draw",
  "on_teardown",
}

--- What a hook's `world` handle offers.
local WORLD_HANDLE = {
  "apply_impulse",
  "despawn",
  "entities",
  "mark_dirty",
  "request_state",
}

local function sorted_keys(tbl, of_type)
  local keys = {}
  for key, value in pairs(tbl) do
    if of_type == nil or type(value) == of_type then
      table.insert(keys, key)
    end
  end
  table.sort(keys)
  return keys
end

describe("distract public API surface", function()
  it("exports exactly the functions the contract lists", function()
    assert.are.same(PUBLIC_FUNCTIONS, sorted_keys(distract, "function"))
  end)

  it("exports no public value other than config", function()
    local values = {}
    for key, value in pairs(distract) do
      if type(value) ~= "function" then
        table.insert(values, key)
      end
    end
    table.sort(values)
    assert.are.same({ "config" }, values)
  end)

  it("answers its query functions before setup has been called", function()
    -- A downstream plugin loads on a lazy trigger and cannot assume ordering.
    assert.is_true(type(distract.get_asset_names()) == "table")
    assert.is_true(type(distract.get_all_actions()) == "table")
    assert.is_true(type(distract.get_plugin_names()) == "table")
    assert.is_true(type(distract.get_backend()) == "string")
  end)

  it("sorts the names it reports, so a plugin can compare them", function()
    local names = distract.get_asset_names()
    local sorted = vim.deepcopy(names)
    table.sort(sorted)
    assert.are.same(sorted, names)
  end)
end)

describe("distract plugin hook contract", function()
  after_each(function()
    require("distract.plugins").reset()
  end)

  it("accepts every hook the contract lists", function()
    local spec = {}
    for _, hook in ipairs(PUBLIC_HOOKS) do
      spec[hook] = function() end
    end
    assert.has_no.errors(function()
      distract.register_plugin("probe_all_hooks", spec)
    end)
    assert.is_true(vim.tbl_contains(distract.get_plugin_names(), "probe_all_hooks"))
  end)

  it("refuses an unknown hook rather than ignoring it", function()
    local ok = pcall(distract.register_plugin, "probe_bad_hook", {
      on_init = function() end,
      on_frobnicate = function() end,
    })
    assert.is_false(ok)
  end)

  it("hands on_init a world handle carrying exactly the documented commands", function()
    local captured = nil
    distract.register_plugin("probe_world", {
      on_init = function(world)
        captured = world
      end,
    })
    require("distract.plugins").bind_world({
      backend = "halfblock",
      entities = function()
        return {}
      end,
    })

    assert.is_not_nil(captured)
    assert.are.same(WORLD_HANDLE, sorted_keys(captured, "function"))
    assert.are_equal("halfblock", captured.backend)
  end)

  it("unregisters by name and reports whether it did", function()
    distract.register_plugin("probe_unregister", { on_tick = function() end })
    assert.is_true(distract.unregister_plugin("probe_unregister"))
    assert.is_false(distract.unregister_plugin("probe_unregister"))
  end)
end)

describe("distract obstacle provider contract", function()
  after_each(function()
    require("distract.obstacles").reset()
  end)

  it("returns an id that unregisters exactly one provider", function()
    local first = distract.register_obstacle_provider(function()
      return {}
    end)
    local second = distract.register_obstacle_provider(function()
      return {}
    end)

    assert.are_not_equal(first, second)
    assert.is_true(distract.unregister_obstacle_provider(first))
    assert.is_false(distract.unregister_obstacle_provider(first))
    assert.is_true(distract.unregister_obstacle_provider(second))
  end)

  it("refuses a provider that is not a function", function()
    assert.is_false(pcall(distract.register_obstacle_provider, "not a function"))
  end)
end)

describe("distract backend capability contract", function()
  it("reports the three fields a plugin degrades against", function()
    local capabilities = distract.get_backend_capabilities()
    assert.are.same({ "alpha", "native_resolution", "scale" }, sorted_keys(capabilities))
    assert.is_true(type(capabilities.scale) == "boolean")
    assert.is_true(capabilities.alpha == "cell" or capabilities.alpha == "pixel")
  end)
end)
