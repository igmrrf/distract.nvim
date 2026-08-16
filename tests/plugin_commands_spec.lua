require("tests.test_harness")
local distract = require("distract")
local engine = require("distract.engine")

--- Runs `fn` with notifications suppressed, returning the count at WARN+.
local function quiet_warnings(fn)
  local original = vim.notify
  local warnings = 0
  vim.notify = function(_, level)
    if level and level >= vim.log.levels.WARN then
      warnings = warnings + 1
    end
  end
  local ok, err = pcall(fn)
  vim.notify = original
  if not ok then
    error(err, 0)
  end
  return warnings
end

--- The single entity left alive after running `cmd`.
local function spawn_via_command(cmd)
  distract.setup({ backend = "halfblock" })
  quiet_warnings(function()
    engine.clear()
    vim.cmd(cmd)
  end)
  local entities = engine.get_entities()
  return entities[#entities]
end

-- `:DistractSpawn` used to call `spawn(pet_type)` and drop everything else on
-- the line, so there was no way to place an entity from the command line at
-- all -- the documented `x`/`y` options were reachable only from Lua.
describe("distract.plugin DistractSpawn options", function()
  it("places an entity at the x and y it was given", function()
    local e = spawn_via_command("DistractSpawn cat x=10 y=5")
    assert.are_equal(10, e.x, "x= was not forwarded to spawn")
    assert.are_equal(5, e.y, "y= was not forwarded to spawn")
    engine.clear()
  end)

  -- `z` and `anchor` were rejected until the floor work reached both backends
  -- together, because a flag that worked in the terminal and did nothing on the
  -- overlay is worse than one that does not exist.
  it("takes the depth it was given as the draw order", function()
    local e = spawn_via_command("DistractSpawn cat z=42")
    assert.are_equal(42, e.z, "z= was not forwarded to spawn")
    assert.are_equal(42, e.z_index, "z overrides the manifest's z_index")
    engine.clear()
  end)

  it("stands an entity on the floor when anchored to the bottom", function()
    local e = spawn_via_command("DistractSpawn cat anchor=bottom")
    assert.are_equal(e.ground_y, e.y, "a bottom anchor starts on the floor")
    engine.clear()
  end)

  it("starts an entity at the top when anchored there", function()
    local e = spawn_via_command("DistractSpawn cat anchor=top")
    assert.are_equal(0, e.y, "a top anchor starts at row zero")
    engine.clear()
  end)

  it("warns about an anchor that names nothing", function()
    distract.setup({ backend = "halfblock" })
    local warnings = quiet_warnings(function()
      engine.clear()
      vim.cmd("DistractSpawn cat anchor=sideways")
    end)
    assert(warnings > 0, "an unknown anchor should be reported, not ignored")
    engine.clear()
  end)

  it("spawns facing left when told to", function()
    local e = spawn_via_command("DistractSpawn cat flip_x=true")
    assert.is_true(e.flip_x, "flip_x= was not forwarded")
    assert.are_equal(-1, e.heading_x, "a flipped spawn must head left")
    engine.clear()
  end)

  it("still spawns with no options at all", function()
    local e = spawn_via_command("DistractSpawn cat")
    assert.is_not_nil(e, "the bare form must keep working")
    assert.are_equal("cat", e.asset_name)
    engine.clear()
  end)

  it("warns about an unparseable option instead of spawning nothing", function()
    local warnings
    distract.setup({ backend = "halfblock" })
    warnings = quiet_warnings(function()
      engine.clear()
      vim.cmd("DistractSpawn cat x=banana")
    end)
    assert(warnings > 0, "a bad option value should be reported")
    engine.clear()
  end)
end)

describe("distract.plugin user commands", function()
  it("registers all expected Neovim user commands", function()
    local cmds = vim.api.nvim_get_commands({})
    assert.is_not_nil(cmds["DistractStart"])
    assert.is_not_nil(cmds["DistractStop"])
    assert.is_not_nil(cmds["DistractToggle"])
    assert.is_not_nil(cmds["DistractBackend"])
    assert.is_not_nil(cmds["DistractSpawn"])
    assert.is_not_nil(cmds["DistractAction"])
    assert.is_not_nil(cmds["DistractClear"])
    assert.is_not_nil(cmds["DistractStatus"])
  end)

  it("can invoke user commands without unhandled exceptions", function()
    assert.has_no.errors(function()
      vim.cmd("DistractStart")
      vim.cmd("DistractBackend halfblock")
      vim.cmd("DistractSpawn cat")
      vim.cmd("DistractSpawn crab")
      vim.cmd("DistractSpawn sun")
      vim.cmd("DistractAction jump cat")
      vim.cmd("DistractAction clip crab")
      vim.cmd("DistractAction eclipse sun")
      vim.cmd("DistractStatus")
      vim.cmd("DistractClear")
      vim.cmd("DistractToggle")
      vim.cmd("DistractStop")
    end)
  end)
end)

describe("distract.plugin command completions", function()
  it("DistractSpawn autocompletes available asset names", function()
    local names = distract.get_asset_names()
    assert.is_true(vim.tbl_contains(names, "cat"))
    assert.is_true(vim.tbl_contains(names, "crab"))
    assert.is_true(vim.tbl_contains(names, "sun"))
  end)

  it("DistractAction autocompletes custom actions across assets", function()
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
end)
