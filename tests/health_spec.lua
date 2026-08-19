require("tests.test_harness")

local detect = require("distract.kitty.detect")
local health = require("distract.health")

describe("distract.health checkhealth implementation", function()
  -- `health.check()` asks the kitty backend whether it is available, and that
  -- answer is cached process-wide once given. Left in place it decides the
  -- backend for every later spec, so it is put back after each test here.
  after_each(function()
    detect.reset()
  end)

  it("runs health.check() without errors", function()
    assert.has_no.errors(function()
      health.check()
    end)
  end)

  it("resolves a reporter under whichever name this Neovim spells it", function()
    assert.is_true(
      type(vim.health.start) == "function" or type(vim.health.report_start) == "function"
    )
    assert.is_true(type(vim.health.ok) == "function" or type(vim.health.report_ok) == "function")
  end)
end)
