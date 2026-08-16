--- Whether the terminal on the other end speaks the kitty graphics protocol.
---
--- Confirmed against ghostty: it answers the `a=q` query but exposes no
--- `$KITTY_WINDOW_ID`, so keying on kitty's own variables would reject a
--- terminal that supports the protocol perfectly well. The environment is
--- therefore a fast path for terminals already confirmed by hand; the query is
--- the authority for everything else, and anything that does not answer `OK`
--- is treated as not supporting it.
---
--- Fails closed. A wrong "yes" fills the user's screen with placeholder
--- codepoints; a wrong "no" costs them the half-block renderer, which works
--- everywhere.

local M = {}

local protocol = require("distract.kitty.protocol")
local writer = require("distract.kitty.writer")

--- How long the terminal has to answer the query.
---
--- Paid once per session, and only when a kitty backend is actually asked for.
M.RESPONSE_TIMEOUT_MS = 200

--- `$TERM` values of terminals confirmed to implement the protocol.
local KNOWN_TERM = {
  ["xterm-kitty"] = true,
  ["xterm-ghostty"] = true,
  ["ghostty"] = true,
}

--- `$TERM_PROGRAM` values of the same.
local KNOWN_TERM_PROGRAM = {
  ghostty = true,
  WezTerm = true,
}

local answer = nil

--- Whether the environment names a terminal already confirmed by hand.
---@return boolean
function M.env_says_yes()
  if vim.env.KITTY_WINDOW_ID then
    return true
  end
  return KNOWN_TERM[vim.env.TERM or ""] == true
    or KNOWN_TERM_PROGRAM[vim.env.TERM_PROGRAM or ""] == true
end

--- Whether a terminal is attached at all.
---
--- `nvim --headless` has no UI, so there is nobody to answer the query and no
--- surface to draw on. Asking anyway would burn the timeout on every call.
---@return boolean
local function has_ui()
  return #vim.api.nvim_list_uis() > 0
end

--- Sends the query and waits for the terminal to answer it.
---@return boolean
local function query_terminal()
  local replied = false
  local group = vim.api.nvim_create_augroup("DistractKittyProbe", { clear = true })

  vim.api.nvim_create_autocmd("TermResponse", {
    group = group,
    callback = function(args)
      local sequence = type(args.data) == "table" and args.data.sequence or args.data
      if protocol.is_probe_ok(sequence) then
        replied = true
      end
      return replied
    end,
  })

  local sent = writer.write(protocol.probe())
  if sent then
    vim.wait(M.RESPONSE_TIMEOUT_MS, function()
      return replied
    end, 10)
  end

  vim.api.nvim_del_augroup_by_id(group)
  return replied
end

--- Whether the graphics protocol can be used, answered once per session.
---@return boolean
function M.is_available()
  if answer ~= nil then
    return answer
  end
  if not has_ui() then
    answer = false
  elseif M.env_says_yes() then
    answer = true
  else
    answer = query_terminal()
  end
  return answer
end

--- Forces the next `is_available` to ask again. For tests, and for a user who
--- has changed terminal without restarting Neovim.
function M.reset()
  answer = nil
end

--- Overrides the answer without asking the terminal. For tests, and for a
--- config that knows better than the probe.
---@param available boolean
function M.override(available)
  if type(available) ~= "boolean" then
    error("distract.kitty.detect.override: expected a boolean")
  end
  answer = available
end

return M
