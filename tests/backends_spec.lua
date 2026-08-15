require("tests.test_harness")

local distract = require("distract")
local renderer = require("distract.renderer")
local sprites = require("distract.terminal_sprites")

describe("distract ASCII backend removal", function()
  it("no longer exposes an ASCII sprite lookup", function()
    assert.is_nil(
      sprites.get_ascii_sprite,
      "ASCII sprite art was removed; the accessor must be gone too"
    )
  end)

  it("no longer implements a float backend", function()
    assert.is_false(renderer.supports("float"), "the ASCII float backend was removed")
  end)

  it("does not advertise a float backend", function()
    distract.setup()
    assert.is_false(vim.tbl_contains(distract.get_available_backends(), "float"))
  end)

  it("advertises exactly the truecolor and GPU backends", function()
    distract.setup()
    local backends = distract.get_available_backends()
    table.sort(backends)
    assert.are.same({ "halfblock", "overlay" }, backends)
  end)

  it("resolves a legacy float request to the truecolor renderer", function()
    distract.setup({ backend = "float" })
    assert.are.same("halfblock", distract.get_backend())
    distract.setup({ backend = "halfblock" })
  end)

  it("renders every asset through the truecolor backend only", function()
    distract.setup({ backend = "halfblock" })
    local engine = require("distract.engine")
    engine.clear()
    for _, name in ipairs({ "cat", "crab", "sun" }) do
      engine.spawn(name)
    end
    local ok, err = pcall(engine.tick)
    assert(ok, string.format("truecolor tick raised: %s", tostring(err)))
    engine.clear()
  end)
end)
