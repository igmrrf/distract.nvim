require("tests.test_harness")

local backends = require("distract.backends")
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

-- The capability table replaced an alias table that named backends in `if`
-- branches, so the kitty renderer could not arrive without editing them.
describe("distract.backends capability table", function()
  it("says what each backend can do with a sprite", function()
    assert.are.same({ scale = false, alpha = "cell" }, backends.capabilities("halfblock"))
    assert.are.same({ scale = true, alpha = "pixel" }, backends.capabilities("overlay"))
    assert.is_nil(backends.capabilities("nothing_registered_here"))
  end)

  it("derives parallax from whether a backend can scale at all", function()
    assert.is_false(backends.supports_parallax("halfblock"))
    assert.is_true(backends.supports_parallax("overlay"))
  end)

  it("hands back a copy, so a caller cannot rewrite the table", function()
    local caps = backends.capabilities("halfblock")
    caps.scale = true
    assert.is_false(backends.capabilities("halfblock").scale)
  end)

  it("resolves aliases without touching the substitution path", function()
    assert.are_equal("halfblock", backends.resolve("truecolor"))
    assert.are_equal("overlay", backends.resolve("gpu"))
    assert.are_equal("halfblock", backends.resolve(nil))
    assert.are_equal("overlay", backends.resolve("  OVERLAY  "))
  end)

  it("reports a substituted backend once", function()
    backends.reset_warnings()
    local warnings = 0
    local original = vim.notify
    vim.notify = function(_, level)
      if level and level >= vim.log.levels.WARN then
        warnings = warnings + 1
      end
    end
    local first = backends.resolve("kitty")
    local second = backends.resolve("kitty")
    vim.notify = original

    assert.are_equal("halfblock", first)
    assert.are_equal("halfblock", second)
    assert.are_equal(1, warnings, "one notice per name, not one per call")
  end)

  it("lets a backend register itself out of being a substitution", function()
    backends.register("kitty", { scale = true, alpha = "pixel" }, { "ghostty" })

    assert.are_equal("kitty", backends.resolve("kitty", true))
    assert.are_equal("kitty", backends.resolve("ghostty", true))
    assert.is_true(backends.supports_parallax("kitty"))
    assert.is_true(vim.tbl_contains(backends.names(), "kitty"))

    -- The registry is process-wide, so a spec that adds to it puts it back.
    backends.reset()
    assert.are_equal("halfblock", backends.resolve("kitty", true))
  end)

  it("refuses a registration that declares nothing", function()
    assert.is_false(pcall(backends.register, "broken", { alpha = "pixel" }))
    assert.is_false(pcall(backends.register, "", { scale = true, alpha = "pixel" }))
  end)
end)
