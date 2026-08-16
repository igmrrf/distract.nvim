--- Getting bytes past Neovim and onto the terminal.
---
--- Settled by measurement, not by reading: driving a real `nvim` TUI under a
--- pty and capturing the byte stream showed that
--- `nvim_chan_send(nvim_list_uis()[1].chan, ...)` -- the obvious route -- fails
--- outright, because on 0.12 that entry is an RPC channel and rejects raw data.
---
--- `vim.v.stderr` is the primary. Neovim's own TUI renders through stdout, so
--- writing escapes there risks landing in the middle of one of its sequences;
--- stderr reaches the same terminal without sharing a stream with the renderer.
--- `io.stdout` is the fallback for the case where stderr is not a channel.

local M = {}

local override = nil

local function write_to_terminal(sequence)
  local channel = vim.v.stderr
  if type(channel) == "number" and channel > 0 then
    local sent = pcall(vim.api.nvim_chan_send, channel, sequence)
    if sent then
      return true
    end
  end

  local handle = io.stdout
  if not handle then
    return false
  end
  return pcall(function()
    handle:write(sequence)
    handle:flush()
  end)
end

--- Sends one escape sequence to the terminal.
---@param sequence string
---@return boolean written
function M.write(sequence)
  if type(sequence) ~= "string" or sequence == "" then
    error("distract.kitty.writer: nothing to write")
  end
  return (override or write_to_terminal)(sequence)
end

--- Sends several sequences that must not be interleaved with anything else.
---
--- A chunked image transmission is one protocol command split across many
--- escapes; another command arriving between two of its chunks aborts it. There
--- is no locking to be had here, only the guarantee that this loop does not
--- yield, so keep the whole transmission in one call.
---@param sequences string[]
---@return boolean written every sequence reached the terminal
function M.write_all(sequences)
  local ok = true
  for _, sequence in ipairs(sequences) do
    ok = M.write(sequence) and ok
  end
  return ok
end

--- Replaces the destination, and returns the previous one.
---
--- The seam the tests use: escape generation is pure, so capturing the stream
--- here is enough to assert on chunk boundaries and payloads with no tty
--- involved.
---@param sink fun(sequence: string): boolean
---@return fun(sequence: string): boolean|nil previous
function M.set_writer(sink)
  if type(sink) ~= "function" then
    error("distract.kitty.writer.set_writer: sink must be a function")
  end
  local previous = override
  override = sink
  return previous
end

--- Restores the real terminal as the destination.
function M.reset_writer()
  override = nil
end

return M
