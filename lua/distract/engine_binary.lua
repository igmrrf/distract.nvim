--- Finding and building the overlay engine binary.
---
--- Separate from the IPC client because it is a different concern with a
--- different failure mode: this is the only part that knows where a binary may
--- live and how to compile one, and it is reached before any process exists.

local M = {}

local asset_path = require("distract.asset_path")

local build_job = nil

local function plugin_root()
  return asset_path.plugin_root()
end

local function exe_suffix()
  return vim.fn.has("win32") == 1 and ".exe" or ""
end

--- Where a compiled engine binary may live, most preferred first.
---
--- `engine/bin` is where a binary downloaded from a GitHub release should be
--- placed. The release workflow publishes per-platform archives, but nothing
--- looked anywhere they could plausibly be installed, so the published binaries
--- were unreachable and every user fell through to building from source.
function M.candidates()
  local root = plugin_root()
  local ext = exe_suffix()
  return {
    root .. "/engine/bin/distract-engine" .. ext,
    root .. "/engine/target/release/distract-engine" .. ext,
    root .. "/engine/target/debug/distract-engine" .. ext,
  }
end

--- Locate the compiled Rust engine binary, or nil when none is installed.
function M.find()
  for _, path in ipairs(M.candidates()) do
    if vim.fn.executable(path) == 1 or vim.fn.filereadable(path) == 1 then
      return path
    end
  end
  return nil
end

function M.build_command()
  return { "cargo", "build", "--release", "--manifest-path", plugin_root() .. "/engine/Cargo.toml" }
end

--- Compiles the engine without blocking the editor.
---
--- This used to be `vim.fn.system(...)`, which made Neovim completely
--- unresponsive for however long a cold Rust build takes — minutes — with a
--- single notification beforehand and no progress.
--- @param on_success function|nil called after a successful build
function M.build(on_success)
  if build_job then
    vim.notify("[Distract] Engine build already in progress.", vim.log.levels.INFO)
    return
  end

  local cmd = M.build_command()
  vim.notify(
    "[Distract] Building the overlay engine in the background:\n  "
      .. table.concat(cmd, " ")
      .. "\nThis can take a few minutes on a cold build.",
    vim.log.levels.INFO
  )

  local stderr_tail = {}
  build_job = vim.fn.jobstart(cmd, {
    on_stderr = function(_, data)
      for _, line in ipairs(data or {}) do
        if line ~= "" then
          table.insert(stderr_tail, line)
          -- Keep the last few lines only; a full cargo log is not a useful
          -- notification.
          if #stderr_tail > 20 then
            table.remove(stderr_tail, 1)
          end
        end
      end
    end,
    on_exit = function(_, code)
      build_job = nil
      if code == 0 then
        vim.notify("[Distract] Engine built.", vim.log.levels.INFO)
        if on_success then
          on_success()
        end
      else
        vim.notify(
          "[Distract] Engine build failed (exit "
            .. tostring(code)
            .. "):\n"
            .. table.concat(stderr_tail, "\n"),
          vim.log.levels.ERROR
        )
      end
    end,
  })

  if build_job <= 0 then
    build_job = nil
    vim.notify("[Distract] Could not start cargo. Is Rust installed?", vim.log.levels.ERROR)
  end
end

return M
