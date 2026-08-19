--- Installing a prebuilt overlay engine from a GitHub release.
---
--- Separate from `engine_binary` because it is a different concern with a
--- different failure mode: that module knows where a binary may live and how to
--- compile one, this one fetches a binary somebody else compiled. Fetching an
--- executable off the network and marking it runnable is the riskiest thing this
--- plugin does, so every step here reports rather than assumes.

local M = {}

local asset_path = require("distract.asset_path")

local uv = vim.uv or vim.loop

--- Where the release workflow publishes, and under what names.
---
--- The archive names are the `artifact_name` column of the release matrix in
--- `.github/workflows/ci.yml`. They must stay in step with it: a name that does
--- not match is a 404 the user reads as "no release for my platform".
local RELEASE_URL_TEMPLATE = "https://github.com/igmrrf/distract.nvim/releases/latest/download/%s"

--- The suffix the release workflow gives each archive's checksum sidecar.
local DIGEST_SUFFIX = ".sha256"

--- How much of a downloaded archive is read at a time, in bytes.
local READ_CHUNK_BYTES = 1024 * 1024

--- Characters in a SHA-256 digest written as hex.
local DIGEST_HEX_LENGTH = 64

local is_downloading = false

local function notify(message, level)
  vim.notify("[Distract] " .. message, level)
end

--- The release archive for the host platform, or nil when none is published.
---
--- A nil here is not a failure to detect: the release matrix publishes four
--- targets, and a platform outside it genuinely has no prebuilt binary and has
--- to build from source.
---@return string|nil artifact
function M.detect_platform_artifact()
  local system_info = uv.os_uname()
  local system_name = system_info.sysname:lower()
  local machine_arch = system_info.machine:lower()

  if system_name == "darwin" then
    if machine_arch == "arm64" or machine_arch == "aarch64" then
      return "distract-engine-macos-aarch64.tar.gz"
    end
    return "distract-engine-macos-x86_64.tar.gz"
  end

  if system_name == "linux" then
    if machine_arch == "x86_64" or machine_arch == "amd64" then
      return "distract-engine-linux-x86_64.tar.gz"
    end
    return nil
  end

  if system_name:find("windows") or system_name:find("mingw") then
    return "distract-engine-windows-x86_64.zip"
  end

  return nil
end

--- The argv that fetches one URL to one path, or `nil, err` when nothing can.
---@param url string
---@param destination string
---@return string[]|nil command
---@return string|nil error_message
local function fetch_command(url, destination)
  if vim.fn.executable("curl") == 1 then
    return { "curl", "-fsSL", "-o", destination, url }
  end
  if vim.fn.executable("wget") == 1 then
    return { "wget", "-q", "-O", destination, url }
  end
  return nil, "neither curl nor wget was found on PATH"
end

--- Runs a command to completion, reporting a launch failure rather than hanging.
---
--- `jobstart` returns a non-positive id when the executable is missing, and in
--- that case `on_exit` never fires. A caller that only waits for `on_exit` waits
--- forever and says nothing, which is the silent failure this wrapper exists to
--- prevent.
---@param command string[]
---@param on_done fun(error_message: string|nil)
local function run(command, on_done)
  if vim.fn.executable(command[1]) ~= 1 then
    on_done(string.format("'%s' was not found on PATH", command[1]))
    return
  end

  local job = vim.fn.jobstart(command, {
    on_exit = function(_, exit_code)
      if exit_code == 0 then
        on_done(nil)
      else
        on_done(string.format("'%s' exited with code %d", command[1], exit_code))
      end
    end,
  })

  if job <= 0 then
    on_done(string.format("could not launch '%s'", command[1]))
  end
end

--- Reads a whole file as a byte string, or `nil, err`.
---@param path string
---@return string|nil contents
---@return string|nil error_message
local function read_file(path)
  local handle, open_err = uv.fs_open(path, "r", 438)
  if not handle then
    return nil, tostring(open_err)
  end

  local chunks = {}
  local offset = 0
  while true do
    local chunk, read_err = uv.fs_read(handle, READ_CHUNK_BYTES, offset)
    if chunk == nil then
      uv.fs_close(handle)
      return nil, tostring(read_err)
    end
    if #chunk == 0 then
      break
    end
    table.insert(chunks, chunk)
    offset = offset + #chunk
  end

  uv.fs_close(handle)
  return table.concat(chunks), nil
end

--- The digest a `shasum`-style sidecar declares, lowercased.
---@param digest_path string
---@return string|nil digest
---@return string|nil error_message
local function declared_digest(digest_path)
  local contents, read_err = read_file(digest_path)
  if not contents then
    return nil, string.format("could not read the checksum file: %s", read_err)
  end

  local digest = contents:match("^%s*(%x+)")
  if not digest or #digest ~= DIGEST_HEX_LENGTH then
    return nil, "the checksum file did not contain a SHA-256 digest"
  end
  return digest:lower(), nil
end

--- Whether the archive on disk is the one the release published.
---@param archive_path string
---@param digest_path string
---@return boolean is_verified
---@return string|nil error_message
function M.verify_archive(archive_path, digest_path)
  local expected, digest_err = declared_digest(digest_path)
  if not expected then
    return false, digest_err
  end

  local contents, read_err = read_file(archive_path)
  if not contents then
    return false, string.format("could not read the downloaded archive: %s", read_err)
  end

  local actual = vim.fn.sha256(contents):lower()
  if actual ~= expected then
    return false, string.format("checksum mismatch\n  expected %s\n  actual   %s", expected, actual)
  end
  return true, nil
end

local function discard(paths)
  for _, path in ipairs(paths) do
    if vim.fn.filereadable(path) == 1 then
      vim.fn.delete(path)
    end
  end
end

--- Makes the installed binary executable, reporting a refusal rather than hiding it.
---
--- A binary that unpacked but could not be marked runnable is indistinguishable
--- from a missing one at the point it is spawned, so it is worth its own message
--- here where the cause is still known.
---@param binary_path string
---@return boolean is_runnable
---@return string|nil error_message
local function grant_execute_permission(binary_path)
  if vim.fn.has("win32") == 1 then
    return true, nil
  end

  local ok, err = pcall(vim.fn.setfperm, binary_path, "rwxr-xr-x")
  if not ok then
    return false, string.format("could not make %s executable: %s", binary_path, tostring(err))
  end
  return true, nil
end

--- Unpacks a verified archive and confirms the binary it was supposed to hold.
---
--- Exit code 0 from the unpacker is not evidence that the binary arrived: an
--- archive laid out differently unpacks cleanly and leaves nothing where the
--- loader looks. The presence check is what turns that into a message.
---@param context table
local function unpack_and_verify(context)
  run({ "tar", "-xf", context.archive_path, "-C", context.target_dir }, function(unpack_err)
    discard({ context.archive_path })

    if unpack_err then
      return context.fail(string.format("could not unpack the archive: %s", unpack_err))
    end
    if vim.fn.filereadable(context.binary_path) ~= 1 then
      return context.fail(
        string.format("the archive unpacked but left no binary at %s", context.binary_path)
      )
    end

    local is_runnable, permission_err = grant_execute_permission(context.binary_path)
    if not is_runnable then
      return context.fail(permission_err)
    end

    context.succeed()
  end)
end

--- Fetches the checksum sidecar, checks the archive against it, then installs.
---@param context table
local function verify_and_install(context)
  local command, command_err = fetch_command(context.digest_url, context.digest_path)
  if not command then
    discard({ context.archive_path })
    return context.fail(string.format("could not fetch the checksum: %s", command_err))
  end

  run(command, function(fetch_err)
    if fetch_err then
      discard({ context.archive_path, context.digest_path })
      return context.fail(
        string.format(
          "could not download the checksum from %s (%s); refusing to install an unverified binary",
          context.digest_url,
          fetch_err
        )
      )
    end

    local is_verified, verify_err = M.verify_archive(context.archive_path, context.digest_path)
    discard({ context.digest_path })
    if not is_verified then
      discard({ context.archive_path })
      return context.fail(string.format("refusing to install this download: %s", verify_err))
    end

    unpack_and_verify(context)
  end)
end

--- The two terminal outcomes, each of which releases the in-progress lock.
---
--- Every path out of the chain goes through one of these. A path that returned
--- without releasing would leave `:DistractDownload` permanently convinced an
--- install was still running.
---@param binary_path string
---@param on_success function|nil
---@return function fail
---@return function succeed
local function build_outcomes(binary_path, on_success)
  local function fail(reason)
    is_downloading = false
    notify(reason .. ".", vim.log.levels.ERROR)
  end

  local function succeed()
    is_downloading = false
    notify(
      string.format("Engine binary verified and installed to %s.", binary_path),
      vim.log.levels.INFO
    )
    if on_success then
      on_success()
    end
  end

  return fail, succeed
end

--- Downloads, verifies and installs the prebuilt overlay engine.
---
--- The checksum is not optional and there is no flag to skip it. The release
--- workflow publishes a `.sha256` beside every archive, so an archive that
--- cannot be checked is a broken release or a substituted file, and neither is
--- something to mark executable.
---@param on_success function|nil called after a verified install
function M.download(on_success)
  if is_downloading then
    notify("Engine download already in progress.", vim.log.levels.INFO)
    return
  end

  local artifact = M.detect_platform_artifact()
  if not artifact then
    local system_info = uv.os_uname()
    notify(
      string.format(
        "No prebuilt binary is published for '%s %s'. Build one with :DistractBuild.",
        system_info.sysname,
        system_info.machine
      ),
      vim.log.levels.ERROR
    )
    return
  end

  local target_dir = asset_path.plugin_root() .. "/engine/bin"
  local made_dir, mkdir_err = pcall(vim.fn.mkdir, target_dir, "p")
  if not made_dir then
    notify(
      string.format("Could not create %s: %s.", target_dir, tostring(mkdir_err)),
      vim.log.levels.ERROR
    )
    return
  end

  local archive_url = string.format(RELEASE_URL_TEMPLATE, artifact)
  local binary_path = target_dir
    .. "/distract-engine"
    .. (vim.fn.has("win32") == 1 and ".exe" or "")
  local fail, succeed = build_outcomes(binary_path, on_success)

  local context = {
    archive_path = string.format("%s/%s", target_dir, artifact),
    digest_path = string.format("%s/%s%s", target_dir, artifact, DIGEST_SUFFIX),
    digest_url = archive_url .. DIGEST_SUFFIX,
    target_dir = target_dir,
    binary_path = binary_path,
    fail = fail,
    succeed = succeed,
  }

  local command, command_err = fetch_command(archive_url, context.archive_path)
  if not command then
    notify(string.format("Could not download the engine: %s.", command_err), vim.log.levels.ERROR)
    return
  end

  notify(string.format("Downloading the prebuilt engine:\n  %s", archive_url), vim.log.levels.INFO)

  -- Held across the whole chain rather than released at the first `on_exit`:
  -- a second invocation during verification would write the same paths the
  -- first is still reading.
  is_downloading = true

  run(command, function(fetch_err)
    if fetch_err then
      discard({ context.archive_path })
      return fail(string.format("could not download %s (%s)", artifact, fetch_err))
    end
    verify_and_install(context)
  end)
end

--- Whether an install is part-way through, for tests and diagnostics.
---@return boolean
function M.is_downloading()
  return is_downloading
end

return M
