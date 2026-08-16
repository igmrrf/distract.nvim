require("tests.test_harness")

local highlights = require("distract.highlights")

local function reset()
  highlights.on_evict(nil)
  highlights.configure({ max_groups = highlights.DEFAULT_MAX_GROUPS })
  highlights.reset()
end

describe("distract.highlights", function()
  before_each(reset)
  after_each(reset)

  it("defines a group once per colour pair", function()
    local first = highlights.group({ 1, 2, 3 }, { 4, 5, 6 }, "cat")
    local again = highlights.group({ 1, 2, 3 }, { 4, 5, 6 }, "cat")

    assert.are_equal(first, again)
    assert.are_equal(1, highlights.count())
  end)

  it("gives each owner its own group, so evicting one cannot blank another", function()
    local cat = highlights.group({ 1, 2, 3 }, nil, "cat")
    local crab = highlights.group({ 1, 2, 3 }, nil, "crab")

    assert.are_not_equal(cat, crab)
    assert.are_equal(2, highlights.count())
  end)

  it("really defines the group in Neovim", function()
    local name = highlights.group({ 17, 34, 51 }, nil, "cat")
    local definition = vim.api.nvim_get_hl(0, { name = name })

    assert.are_equal(0x112233, definition.fg)
  end)

  it("evicts the least recently drawn owner when the ceiling is reached", function()
    highlights.configure({ max_groups = 2 })
    local evicted = {}
    highlights.on_evict(function(owner)
      evicted[#evicted + 1] = owner
    end)

    highlights.group({ 1, 1, 1 }, nil, "old")
    highlights.group({ 2, 2, 2 }, nil, "recent")
    highlights.group({ 3, 3, 3 }, nil, "newcomer")

    assert.are.same({ "old" }, evicted)
    assert.are_equal(2, highlights.count())
  end)

  it("never evicts the owner it is drawing for", function()
    highlights.configure({ max_groups = 1 })
    local evicted = {}
    highlights.on_evict(function(owner)
      evicted[#evicted + 1] = owner
    end)

    highlights.group({ 1, 1, 1 }, nil, "solo")
    highlights.group({ 2, 2, 2 }, nil, "solo")

    assert.are.same({}, evicted)
    assert.are_equal(2, highlights.count(), "a lone owner keeps its own colours")
  end)

  it("clears an owner's groups on release", function()
    local name = highlights.group({ 9, 9, 9 }, nil, "doomed")
    highlights.release("doomed")

    assert.are_equal(0, highlights.count())
    assert.is_nil(vim.api.nvim_get_hl(0, { name = name }).fg)
  end)

  it("refuses a ceiling below one", function()
    assert.is_false(pcall(highlights.configure, { max_groups = 0 }))
  end)

  it("forgets everything on reset, as `:hi clear` does", function()
    highlights.group({ 1, 2, 3 }, nil, "cat")
    highlights.reset()

    assert.are_equal(0, highlights.count())
  end)
end)
