require("tests.test_harness")

local engine_binary = require("distract.engine_binary")
local engine_download = require("distract.engine_download")

--- The `artifact_name` column of the release matrix in `.github/workflows/ci.yml`.
---
--- Pinned here rather than derived: a name that drifts from the workflow is a
--- 404 the user reads as "no release for my platform", and nothing else in the
--- suite compares the two.
local PUBLISHED_ARTIFACTS = {
  ["distract-engine-linux-x86_64.tar.gz"] = true,
  ["distract-engine-macos-x86_64.tar.gz"] = true,
  ["distract-engine-macos-aarch64.tar.gz"] = true,
  ["distract-engine-windows-x86_64.zip"] = true,
}

local written = {}

local function write_file(contents)
  local path = vim.fn.tempname()
  local handle = assert(io.open(path, "wb"))
  handle:write(contents)
  handle:close()
  table.insert(written, path)
  return path
end

describe("distract.engine_download platform detection", function()
  it("names an artifact the release workflow actually publishes", function()
    local artifact = engine_download.detect_platform_artifact()
    local system_name = vim.uv.os_uname().sysname:lower()

    if system_name == "darwin" or system_name:find("windows") then
      assert.is_not_nil(artifact)
    end
    if artifact then
      assert.is_true(PUBLISHED_ARTIFACTS[artifact] == true)
    end
  end)

  it("reports no artifact for a platform the release matrix does not cover", function()
    -- Linux on anything but x86_64 has no published binary, and saying so is
    -- what routes the user to :DistractBuild instead of a 404.
    local system_info = vim.uv.os_uname()
    if system_info.sysname:lower() == "linux" then
      local arch = system_info.machine:lower()
      local is_covered = arch == "x86_64" or arch == "amd64"
      assert.are_equal(is_covered, engine_download.detect_platform_artifact() ~= nil)
    end
  end)

  it("is not downloading before anything asks it to", function()
    assert.is_false(engine_download.is_downloading())
  end)
end)

describe("distract.engine_download archive verification", function()
  after_each(function()
    for _, path in ipairs(written) do
      vim.fn.delete(path)
    end
    written = {}
  end)

  it("accepts an archive whose digest matches its sidecar", function()
    local archive = write_file("engine bytes")
    local digest = write_file(vim.fn.sha256("engine bytes") .. "  distract-engine.tar.gz\n")

    local is_verified, err = engine_download.verify_archive(archive, digest)
    assert.is_true(is_verified)
    assert.is_nil(err)
  end)

  it("accepts an uppercase digest, which is how PowerShell writes one", function()
    local archive = write_file("engine bytes")
    local digest = write_file(vim.fn.sha256("engine bytes"):upper() .. "  archive.zip\n")

    assert.is_true(engine_download.verify_archive(archive, digest))
  end)

  it("rejects an archive whose bytes do not match the sidecar", function()
    local archive = write_file("substituted bytes")
    local digest = write_file(vim.fn.sha256("engine bytes") .. "  distract-engine.tar.gz\n")

    local is_verified, err = engine_download.verify_archive(archive, digest)
    assert.is_false(is_verified)
    assert.is_true(err:find("checksum mismatch", 1, true) ~= nil)
  end)

  it("rejects a sidecar that holds no digest", function()
    local archive = write_file("engine bytes")
    local digest = write_file("404: Not Found\n")

    local is_verified, err = engine_download.verify_archive(archive, digest)
    assert.is_false(is_verified)
    assert.is_true(err:find("SHA%-256") ~= nil)
  end)

  it("rejects a digest of the wrong length rather than truncating it", function()
    local archive = write_file("engine bytes")
    local digest = write_file("deadbeef  archive.tar.gz\n")

    assert.is_false(engine_download.verify_archive(archive, digest))
  end)

  it("reports a missing sidecar instead of treating it as verified", function()
    local archive = write_file("engine bytes")

    local is_verified, err = engine_download.verify_archive(archive, "/nonexistent/archive.sha256")
    assert.is_false(is_verified)
    assert.is_not_nil(err)
  end)

  it("reports a missing archive instead of treating it as verified", function()
    local digest = write_file(vim.fn.sha256("engine bytes") .. "  archive.tar.gz\n")

    local is_verified, err = engine_download.verify_archive("/nonexistent/archive.tar.gz", digest)
    assert.is_false(is_verified)
    assert.is_not_nil(err)
  end)

  it("hashes the whole archive, not just its first bytes", function()
    -- A read that stops at one chunk boundary would hash a prefix and pass a
    -- truncated download, which is the failure the chunked read invites.
    local long_body = string.rep("distract", 200000)
    local archive = write_file(long_body)
    local digest = write_file(vim.fn.sha256(long_body) .. "  archive.tar.gz\n")

    assert.is_true(engine_download.verify_archive(archive, digest))

    local truncated = write_file(long_body:sub(1, #long_body - 1))
    assert.is_false(engine_download.verify_archive(truncated, digest))
  end)
end)

describe("distract.engine_binary after the download split", function()
  it("still finds a binary in engine/bin first, where a download installs one", function()
    local candidates = engine_binary.candidates()
    assert.is_true(#candidates >= 3)
    assert.is_true(candidates[1]:find("engine/bin/distract-engine", 1, true) ~= nil)
  end)

  it("no longer carries the download concern", function()
    assert.is_nil(engine_binary.download)
    assert.is_nil(engine_binary.detect_platform_artifact)
  end)
end)
