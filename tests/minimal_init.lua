local test_sandbox = vim.fn.tempname() .. "-distract-test-sandbox"
vim.fn.mkdir(test_sandbox .. "/data", "p")
vim.fn.mkdir(test_sandbox .. "/config", "p")
vim.fn.mkdir(test_sandbox .. "/state", "p")

vim.env.XDG_DATA_HOME = test_sandbox .. "/data"
vim.env.XDG_CONFIG_HOME = test_sandbox .. "/config"
vim.env.XDG_STATE_HOME = test_sandbox .. "/state"

local root = vim.fn.getcwd()
vim.opt.runtimepath:prepend(root)

local plenary_dir = root .. "/.tests/site/pack/deps/start/plenary.nvim"
if vim.fn.isdirectory(plenary_dir) == 0 then
  vim.fn.system({
    "git",
    "clone",
    "--depth=1",
    "https://github.com/nvim-lua/plenary.nvim",
    plenary_dir,
  })
end
vim.opt.runtimepath:prepend(plenary_dir)
vim.cmd("runtime plugin/plenary.vim")
vim.cmd("runtime plugin/distract.lua")
