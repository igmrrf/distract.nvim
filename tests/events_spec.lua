require("tests.test_harness")
local events = require("distract.events")

describe("distract.events autocmd lifecycle and throttling", function()
  it("registers expected autocmds upon setup", function()
    events.setup({ idle_timeout_ms = 3000, debounce_ms = 30 })

    local cmds = vim.api.nvim_get_autocmds({ group = "DistractEvents" })
    assert.is_true(#cmds >= 4)

    local registered = {}
    for _, cmd in ipairs(cmds) do
      registered[cmd.event] = true
    end

    assert.is_true(registered["TextChanged"] or registered["TextChangedI"])
    assert.is_true(registered["WinScrolled"])
    assert.is_true(registered["CursorMoved"] or registered["CursorMovedI"])
    assert.is_true(registered["VimResized"])
  end)

  it("emit_debounced queues and debounces events without throwing errors", function()
    assert.has_no.errors(function()
      events.emit_debounced("typing")
      events.emit_debounced("typing")
      events.emit_debounced("moving")
      events.emit_debounced("scrolling")
    end)
  end)

  it("reset_idle_timer resets timer without error", function()
    assert.has_no.errors(function()
      events.reset_idle_timer()
    end)
  end)

  it("teardown cleanly unregisters all autocmds", function()
    events.teardown()
    local cmds = vim.api.nvim_get_autocmds({ group = "DistractEvents" })
    assert.are_equal(0, #cmds)
  end)
end)
