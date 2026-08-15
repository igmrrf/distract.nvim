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
      backend = "kitty",
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
    assert.are.same("kitty", distract.get_backend())
    assert.are_equal(9000, distract.config.idle_timeout_ms)
    assert.are_equal(120, distract.config.debounce_ms)
    assert.is_not_nil(distract.config.assets.custom_bird)
    assert.is_not_nil(distract.config.assets.cat)
  end)

  it("supports dynamic backend switching across all options", function()
    distract.setup()
    local backends = distract.get_available_backends()
    assert.are_equal(4, #backends)

    distract.set_backend("halfblock")
    assert.are.same("halfblock", distract.get_backend())

    distract.set_backend("kitty")
    assert.are.same("kitty", distract.get_backend())

    distract.set_backend("float")
    assert.are.same("float", distract.get_backend())

    distract.set_backend("overlay")
    assert.are.same("overlay", distract.get_backend())
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
