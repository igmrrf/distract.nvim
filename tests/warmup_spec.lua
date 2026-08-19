require("tests.test_harness")

local sprite_sources = require("distract.sprite_sources")
local warmup = require("distract.warmup")
local gif = require("distract.gif")
local builder = require("tests.gif_builder")

describe("distract.warmup background worker", function()
  after_each(function()
    warmup.reset()
  end)

  it("validates that job key is a non-empty string", function()
    local ok_nil = pcall(warmup.request, nil, function() end)
    local ok_empty = pcall(warmup.request, "", function() end)
    local ok_number = pcall(warmup.request, 123, function() end)

    assert.is_false(ok_nil)
    assert.is_false(ok_empty)
    assert.is_false(ok_number)
  end)

  it("validates that job is a function", function()
    local ok_nil = pcall(warmup.request, "test_key", nil)
    local ok_string = pcall(warmup.request, "test_key", "not_a_function")

    assert.is_false(ok_nil)
    assert.is_false(ok_string)
  end)

  it("deduplicates requests with the same key", function()
    local run_counter = 0
    warmup.request("duplicate_key", function()
      run_counter = run_counter + 1
    end)
    warmup.request("duplicate_key", function()
      run_counter = run_counter + 1
    end)

    assert.are_equal(1, warmup.pending_count())
    assert.is_true(warmup.is_pending("duplicate_key"))
    warmup.drain()
    assert.are_equal(1, run_counter)
    assert.are_equal(0, warmup.pending_count())
    assert.is_false(warmup.is_pending("duplicate_key"))
  end)

  it("runs queued jobs to completion on drain", function()
    local results = {}
    warmup.request("job_first", function()
      table.insert(results, "first_start")
      coroutine.yield()
      table.insert(results, "first_end")
    end)
    warmup.request("job_second", function()
      table.insert(results, "second_done")
    end)

    assert.are_equal(2, warmup.pending_count())
    warmup.drain()
    assert.are_equal(0, warmup.pending_count())
    assert.are.same({ "first_start", "first_end", "second_done" }, results)
  end)

  it("drops outstanding jobs on reset", function()
    local ran = false
    warmup.request("job_cancel", function()
      ran = true
    end)
    assert.are_equal(1, warmup.pending_count())
    warmup.reset()
    assert.are_equal(0, warmup.pending_count())
    assert.is_false(warmup.is_pending("job_cancel"))
    assert.is_false(ran)
  end)

  it("surfaces errors when drain runs a failing job", function()
    warmup.request("job_error", function()
      error("job failure deliberate")
    end)
    local ok, error_message = pcall(warmup.drain)
    assert.is_false(ok)
    assert(tostring(error_message):match("job failure deliberate"))
  end)

  it("allows GIF decoding to yield per frame via on_frame", function()
    local frame_indices = {}
    local raw_bytes = builder.header({
      width = 2,
      height = 2,
      palette = { { 255, 0, 0 }, { 0, 255, 0 } },
    }) .. builder.image({ width = 2, height = 2, indices = { 0, 1, 0, 1 } }) .. builder.image({
      width = 2,
      height = 2,
      indices = { 1, 0, 1, 0 },
    }) .. builder.TRAILER

    local decoded, decode_error = gif.decode_bytes(raw_bytes, {
      on_frame = function(frame_index)
        table.insert(frame_indices, frame_index)
      end,
    })

    assert.is_nil(decode_error)
    assert.is_not_nil(decoded)
    assert.are_equal(2, #decoded.frames)
    assert.are.same({ 1, 2, 1, 2 }, frame_indices)
  end)
end)

describe("distract.warmup cancellation and re-binding", function()
  local GIF_PATH = vim.fn.getcwd() .. "/tests/fixtures/physics/frame_delays.gif"

  local function manifest()
    return { spritesheet = { path = GIF_PATH, format = "gif" } }
  end

  after_each(function()
    warmup.reset()
    sprite_sources.unbind_manifest("probe_warm_gif")
  end)

  it("re-queues a decode that stop() cancelled part-way", function()
    sprite_sources.bind_manifest("probe_warm_gif", manifest())
    assert.is_true(warmup.is_pending("gif:probe_warm_gif"))

    -- What `distract.stop()` does: drop the queue without running it.
    warmup.reset()
    assert.is_false(warmup.is_pending("gif:probe_warm_gif"))

    -- What a restart does: re-read the same manifest. The source has not
    -- changed, so a source-gated warm-up would never queue again and the first
    -- draw would decode the whole GIF synchronously.
    sprite_sources.bind_manifest("probe_warm_gif", manifest())
    assert.is_true(warmup.is_pending("gif:probe_warm_gif"))
  end)

  it("does not re-queue a decode that already finished", function()
    sprite_sources.bind_manifest("probe_warm_gif", manifest())
    warmup.drain()
    assert.are_equal(0, warmup.pending_count())

    sprite_sources.bind_manifest("probe_warm_gif", manifest())
    assert.is_false(warmup.is_pending("gif:probe_warm_gif"))
    assert.are_equal(0, warmup.pending_count())
  end)
end)
