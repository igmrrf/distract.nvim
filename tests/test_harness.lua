-- Lightweight test harness compatible with Plenary / Busted syntax
local M = {}

M.passed = 0
M.failed = 0
M.failures = {}

if not _G.describe then
  local current_suite = ""
  -- Per-suite hooks, matching the subset of the Busted API the specs use.
  -- Hooks are scoped to the enclosing `describe` and cleared when it ends, so
  -- one suite's cleanup cannot leak into the next.
  local before_hooks = {}
  local after_hooks = {}

  _G.describe = function(name, fn)
    local outer_before, outer_after = before_hooks, after_hooks
    before_hooks, after_hooks = {}, {}
    current_suite = name
    print(string.format("\n[%s]", name))
    fn()
    before_hooks, after_hooks = outer_before, outer_after
  end

  _G.before_each = function(fn)
    table.insert(before_hooks, fn)
  end

  _G.after_each = function(fn)
    table.insert(after_hooks, fn)
  end

  local function run_hooks(hooks)
    for _, hook in ipairs(hooks) do
      pcall(hook)
    end
  end

  _G.it = function(name, fn)
    run_hooks(before_hooks)
    local ok, err = pcall(fn)
    run_hooks(after_hooks)
    if ok then
      M.passed = M.passed + 1
      print(string.format("  ✓ %s", name))
    else
      M.failed = M.failed + 1
      table.insert(M.failures, { suite = current_suite, test = name, error = err })
      print(string.format("  ✗ %s\n    %s", name, tostring(err)))
    end
  end

  local custom_assert = {}
  setmetatable(custom_assert, {
    __call = function(_, val, msg)
      if not val then
        error(msg or "assertion failed!", 2)
      end
      return val
    end,
  })

  custom_assert.are = {
    same = function(expected, actual, msg)
      if vim.inspect(expected) ~= vim.inspect(actual) then
        error(
          string.format(
            "assert.are.same failed: %s (expected %s, got %s)",
            msg or "",
            vim.inspect(expected),
            vim.inspect(actual)
          ),
          2
        )
      end
    end,
  }

  custom_assert.are_equal = function(expected, actual, msg)
    if expected ~= actual then
      error(
        string.format(
          "assert.are_equal failed: %s (expected %s, got %s)",
          msg or "",
          tostring(expected),
          tostring(actual)
        ),
        2
      )
    end
  end

  custom_assert.are_not_equal = function(unexpected, actual, msg)
    if unexpected == actual then
      error(
        string.format("assert.are_not_equal failed: %s (both were %s)", msg or "", tostring(actual)),
        2
      )
    end
  end

  custom_assert.is_function = function(val, msg)
    if type(val) ~= "function" then
      error(string.format("assert.is_function failed: %s (got %s)", msg or "", type(val)), 2)
    end
  end

  custom_assert.is_true = function(val, msg)
    if val ~= true then
      error(string.format("assert.is_true failed: %s (got %s)", msg or "", tostring(val)), 2)
    end
  end

  custom_assert.is_false = function(val, msg)
    if val ~= false then
      error(string.format("assert.is_false failed: %s (got %s)", msg or "", tostring(val)), 2)
    end
  end

  custom_assert.is_not_nil = function(val, msg)
    if val == nil then
      error(string.format("assert.is_not_nil failed: %s", msg or ""), 2)
    end
  end

  custom_assert.is_nil = function(val, msg)
    if val ~= nil then
      error(string.format("assert.is_nil failed: %s (got %s)", msg or "", tostring(val)), 2)
    end
  end

  custom_assert.has_no = {
    errors = function(fn)
      local ok, err = pcall(fn)
      if not ok then
        error(string.format("assert.has_no.errors failed: %s", tostring(err)), 2)
      end
    end,
  }

  _G.assert = custom_assert
end

function M.report()
  print("\n==================================================")
  print(string.format("  Test Summary: %d Passed, %d Failed", M.passed, M.failed))
  print("==================================================")
  if M.failed > 0 then
    error(string.format("Test run completed with %d failures.", M.failed))
  end
end

return M
