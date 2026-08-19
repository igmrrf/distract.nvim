--- Doing expensive first-draw work early, in slices, instead of all at once on
--- the frame that needs it.
---
--- Two costs land on the first frame that needs them and are then cached
--- forever: decoding an imported GIF (~390ms for a 32-frame 1920x1080 source)
--- and rasterising a voxel pose (~17ms for a dense model, once per
--- `(asset, frame, facing)`). Both are one-off, and both are long enough to be
--- seen -- 390ms is a visible freeze, 17ms is a dropped frame at 60 FPS.
---
--- A job is a function that calls `coroutine.yield()` at its own natural
--- boundaries -- per GIF frame, per pose. This drives it on a timer, resuming
--- until the slice budget is spent, so no single tick blocks longer than a
--- frame. Nothing here changes what is drawn: if a draw arrives before the
--- warmup finishes, the synchronous path runs and caches as it always did, and
--- the job finds the work done and stops.

local M = {}

local uv = vim.uv or vim.loop

--- How long one slice may run before yielding the main loop back.
---
--- Under a 60 FPS frame with room to spare for the tick and the draw that share
--- it. A GIF frame costs ~12ms and a pose ~0.25ms, so this is one GIF frame or
--- many poses per slice.
local SLICE_BUDGET_MS = 8

--- How often to resume. Matched to a 60 FPS frame rather than to the render
--- cadence: warming is background work and should not pace the animation.
local SLICE_INTERVAL_MS = 16

---@type table[] queued jobs, oldest first
local queue = {}
---@type table<string, boolean> job keys already queued or running
local queued_keys = {}
local timer = nil

local function stop_timer()
  if timer then
    timer:stop()
    timer:close()
    timer = nil
  end
end

--- Resumes jobs until the slice budget is spent or the queue empties.
local function run_slice()
  local deadline = uv.hrtime() + SLICE_BUDGET_MS * 1e6

  while #queue > 0 and uv.hrtime() < deadline do
    local job = queue[1]
    local ok, err = coroutine.resume(job.thread)

    if not ok then
      table.remove(queue, 1)
      queued_keys[job.key] = nil
      -- Warming is optional work, so a failure must not take the session with
      -- it; the synchronous path will run and report whatever went wrong in the
      -- terms the user can act on.
      vim.notify(
        string.format("[Distract] Warming '%s' failed: %s", job.key, tostring(err)),
        vim.log.levels.DEBUG
      )
    elseif coroutine.status(job.thread) == "dead" then
      table.remove(queue, 1)
      queued_keys[job.key] = nil
    end
  end

  if #queue == 0 then
    stop_timer()
  end
end

local function ensure_timer()
  if timer then
    return
  end
  timer = uv.new_timer()
  timer:start(
    SLICE_INTERVAL_MS,
    SLICE_INTERVAL_MS,
    vim.schedule_wrap(function()
      run_slice()
    end)
  )
end

--- Queues work to be done in slices before something needs it.
---
--- The key deduplicates: asking twice for the same work while it is still
--- queued or running is a no-op rather than a second pass.
---@param key string
---@param job fun() a function that calls `coroutine.yield()` at its own boundaries
function M.request(key, job)
  if type(key) ~= "string" or key == "" then
    error("distract.warmup: a job needs a non-empty key", 2)
  end
  if type(job) ~= "function" then
    error("distract.warmup: a job must be a function", 2)
  end
  if queued_keys[key] then
    return
  end

  queued_keys[key] = true
  table.insert(queue, { key = key, thread = coroutine.create(job) })
  ensure_timer()
end

--- Whether this key is queued or part-way through.
---@param key string
---@return boolean
function M.is_pending(key)
  return queued_keys[key] == true
end

--- How many jobs are outstanding.
---@return integer
function M.pending_count()
  return #queue
end

--- Runs every queued job to completion now.
---
--- For tests and for a teardown that must not leave a half-decoded asset behind.
--- Ignores the slice budget by definition: the point is to finish.
function M.drain()
  while #queue > 0 do
    local job = queue[1]
    local ok, err = coroutine.resume(job.thread)
    if not ok or coroutine.status(job.thread) == "dead" then
      table.remove(queue, 1)
      queued_keys[job.key] = nil
      if not ok then
        error(err, 0)
      end
    end
  end
  stop_timer()
end

--- Drops everything outstanding without running it.
function M.reset()
  queue = {}
  queued_keys = {}
  stop_timer()
end

return M
