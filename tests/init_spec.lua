require("tests.test_harness")
local distract = require("distract")

describe("distract.init configuration and lifecycle", function()
  it("should initialize default config and default assets", function()
    distract.setup()
    assert.are.same("halfblock", distract.config.backend)
    assert.are_equal(30, distract.config.fps)
    assert.are_equal(5000, distract.config.idle_timeout_ms)
    assert.are_equal(50, distract.config.debounce_ms)
    assert.is_not_nil(distract.config.assets.cat)
    assert.is_not_nil(distract.config.assets.crab)
    assert.is_not_nil(distract.config.assets.sun)
  end)

  it("should merge user configuration options", function()
    distract.setup({
      backend = "overlay",
      idle_timeout_ms = 9000,
      debounce_ms = 120,
      assets = {
        custom_bird = {
          name = "custom_bird",
          asset_type = "sprite",
          initial_state = "fly",
          custom_actions = { fly = { target_state = "fly" } }
        }
      }
    })
    assert.are.same("overlay", distract.get_backend())
    assert.are_equal(9000, distract.config.idle_timeout_ms)
    assert.are_equal(120, distract.config.debounce_ms)
    assert.is_not_nil(distract.config.assets.custom_bird)
    assert.is_not_nil(distract.config.assets.cat)
  end)

  it("supports dynamic backend switching across all options", function()
    distract.setup()
    for _, name in ipairs(distract.get_available_backends()) do
      distract.set_backend(name)
      assert.are.same(name, distract.get_backend())
    end
    distract.set_backend("halfblock")
  end)
end)

describe("distract.init backend availability", function()
  local renderer = require("distract.renderer")

  it("only advertises backends that are actually implemented", function()
    distract.setup()
    for _, name in ipairs(distract.get_available_backends()) do
      local implemented = (name == "overlay") or renderer.supports(name)
      assert(implemented, string.format(
        "backend '%s' is advertised but has no renderer implementation", name))
    end
  end)

  it("does not advertise the unimplemented kitty graphics backend", function()
    distract.setup()
    assert.is_false(vim.tbl_contains(distract.get_available_backends(), "kitty"),
      "kitty graphics protocol is not implemented and must not be offered")
  end)

  it("resolves a kitty request to an implemented backend rather than silent ASCII", function()
    distract.setup({ backend = "kitty" })
    local resolved = distract.get_backend()
    assert.are.same("halfblock", resolved)
    assert.is_true(renderer.supports(resolved))
    distract.setup({ backend = "halfblock" })
  end)

  it("tells the user when a requested backend was substituted", function()
    local messages = {}
    local original = vim.notify
    vim.notify = function(msg) table.insert(messages, msg) end
    local ok, err = pcall(distract.setup, { backend = "ghostty" })
    vim.notify = original
    assert(ok, tostring(err))

    local mentioned = false
    for _, msg in ipairs(messages) do
      if tostring(msg):lower():match("kitty") then mentioned = true end
    end
    assert.is_true(mentioned,
      "substituting an unavailable backend must be reported, not silent")
    distract.setup({ backend = "halfblock" })
  end)
end)

describe("distract.init asset & action query methods", function()
  it("get_asset_names returns sorted list of registered assets", function()
    distract.setup()
    local names = distract.get_asset_names()
    assert.is_true(#names >= 3)
    assert.is_true(vim.tbl_contains(names, "cat"))
    assert.is_true(vim.tbl_contains(names, "crab"))
    assert.is_true(vim.tbl_contains(names, "sun"))
  end)

  it("get_all_actions returns unique actions across all assets", function()
    distract.setup()
    local actions = distract.get_all_actions()
    assert.is_true(vim.tbl_contains(actions, "jump"))
    assert.is_true(vim.tbl_contains(actions, "yawn"))
    assert.is_true(vim.tbl_contains(actions, "clip"))
    assert.is_true(vim.tbl_contains(actions, "burrow"))
    assert.is_true(vim.tbl_contains(actions, "eclipse"))
    assert.is_true(vim.tbl_contains(actions, "rise"))
    assert.is_true(vim.tbl_contains(actions, "set"))
    assert.is_true(vim.tbl_contains(actions, "flare"))
  end)

  it("spawn, action, clear, and status functions can be invoked in in-terminal mode", function()
    assert.has_no.errors(function()
      distract.setup({ backend = "halfblock" })
      distract.spawn("cat")
      distract.spawn("crab")
      distract.spawn("sun")
      distract.action("jump", "cat")
      distract.action("clip", "crab")
      distract.action("eclipse", "sun")
      distract.status()
      distract.clear()
    end)
  end)
end)
