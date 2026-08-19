require("tests.test_harness")

local health = require("distract.health")
local engine_binary = require("distract.engine_binary")

describe("distract.health checkhealth implementation", function()
  it("runs health.check() without errors", function()
    assert.has_no.errors(function()
      health.check()
    end)
  end)

  it("detects binary candidates list with preferred locations", function()
    local candidates = engine_binary.candidates()
    assert.is_true(#candidates >= 3)
    assert.is_true(candidates[1]:find("engine/bin/distract-engine", 1, true) ~= nil)
  end)

  it("detects a valid release artifact name for the host system", function()
    local artifact = engine_binary.detect_platform_artifact()
    local system_name = vim.uv.os_uname().sysname:lower()
    if system_name == "darwin" or system_name == "linux" or system_name:find("windows") then
      assert.is_not_nil(artifact)
      assert.is_true(artifact:find("%.tar%.gz$") ~= nil or artifact:find("%.zip$") ~= nil)
    end
  end)
end)
