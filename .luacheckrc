-- luacheck configuration for distract.nvim
std = "lua51"
cache = true

-- `vim` is writable, not read-only: the plugin sets `vim.g.loaded_distract`,
-- the test bootstrap sets `vim.env.XDG_*`, and specs stub `vim.notify` and
-- individual `vim.api` functions to observe calls.
globals = {
  "vim",
  -- tests/test_harness.lua installs these when Plenary is not present.
  "describe",
  "it",
  "before_each",
  "after_each",
  "assert",
}

-- Warnings that are noise in this codebase rather than defects.
ignore = {
  "212", -- unused argument: jobstart callbacks take (job_id, data) and rarely need the id
}

exclude_files = {
  ".tests/",
}
