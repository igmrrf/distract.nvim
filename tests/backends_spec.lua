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
    local available = distract.get_available_backends()
    table.sort(available)
    assert.are.same({ "halfblock", "overlay" }, available)
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
    assert.are.same(
      { scale = false, alpha = "cell", native_resolution = false },
      backends.capabilities("halfblock")
    )
    assert.are.same(
      { scale = true, alpha = "pixel", native_resolution = true },
      backends.capabilities("overlay")
    )
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

  it("halfblock and overlay report native_resolution explicitly", function()
    assert.is_false(backends.capabilities(backends.HALFBLOCK).native_resolution)
    assert.is_true(backends.capabilities(backends.OVERLAY).native_resolution)
  end)

  it("register requires native_resolution alongside scale and alpha", function()
    assert.is_false(pcall(backends.register, "missing_field", { scale = true, alpha = "pixel" }))
    backends.reset()
  end)

  it("lets a backend register itself out of being a substitution", function()
    backends.register(
      "kitty",
      { scale = true, alpha = "pixel", native_resolution = true },
      { "ghostty" }
    )

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

describe("the backend a session gets by default", function()
  local kitty = require("distract.kitty")
  local detect = require("distract.kitty.detect")

  --- Runs `fn` as though the terminal had answered the graphics-protocol query.
  --- The registries are process-wide, so everything is put back afterwards.
  local function with_kitty_available(fn)
    local truecolor = vim.o.termguicolors
    vim.o.termguicolors = true
    detect.override(true)
    kitty.setup()

    local ok, err = pcall(fn)

    kitty.reset()
    backends.reset()
    vim.o.termguicolors = truecolor
    distract.config.backend = nil
    distract.setup()
    if not ok then
      error(err)
    end
  end

  before_each(function()
    distract.config.backend = nil
  end)

  it("draws through the graphics protocol where the terminal has one", function()
    with_kitty_available(function()
      distract.setup()
      assert.are_equal("kitty", distract.get_backend())
    end)
  end)

  it("still honours a backend the user named", function()
    with_kitty_available(function()
      distract.setup({ backend = "halfblock" })
      assert.are_equal("halfblock", distract.get_backend())
    end)
  end)

  it("falls back to half-blocks where there is no graphics protocol", function()
    distract.setup()
    assert.are_equal("halfblock", distract.get_backend())
  end)

  it("keeps a chosen backend across a later setup that names none", function()
    distract.setup({ backend = "overlay" })
    distract.setup()
    assert.are_equal("overlay", distract.get_backend())
    distract.config.backend = nil
    distract.setup()
  end)
end)
