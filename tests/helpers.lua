local distract = require("distract")
local events = require("distract.events")
local external = require("distract.external")

local M = {}

--- Resets plugin state, unregisters autocmds, and cancels timers between tests.
function M.reset()
  pcall(distract.stop)
  pcall(events.teardown)
  distract.setup()
  external.setup(distract.config)
end

--- Mock IPC message helper.
function M.feed_ipc(tbl)
  local json_str = vim.fn.json_encode(tbl)
  external.handle_ipc_message(json_str)
end

return M
