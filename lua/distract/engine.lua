local M = {}
local uv = vim.uv or vim.loop
local renderer = require("distract.renderer")

local timer = nil
local entities = {}
local entity_counter = 0
local fps = 30
local tick_rate = math.floor(1000 / fps)
local is_running = false

function M.start()
  if is_running then return end
  is_running = true
  timer = uv.new_timer()
  timer:start(0, tick_rate, vim.schedule_wrap(function()
    M.tick()
  end))
end

function M.stop()
  if timer then
    timer:stop()
    timer:close()
    timer = nil
  end
  is_running = false
  renderer.clear_all()
  entities = {}
end

function M.tick()
  for _, entity in ipairs(entities) do
    if entity.update then
      entity:update()
    end
  end
  renderer.draw(entities)
end

function M.spawn(pet_type)
  pet_type = pet_type or "cat"
  local ok, pet_module = pcall(require, "distract.pets." .. pet_type)
  if not ok then
    vim.notify("Distract: Unknown pet type '" .. pet_type .. "'", vim.log.levels.ERROR)
    return
  end
  
  entity_counter = entity_counter + 1
  
  -- Spawn in middle of screen
  local start_x = math.floor(vim.o.columns / 2)
  local start_y = math.floor(vim.o.lines / 2)
  
  local pet = pet_module.new(entity_counter, start_x, start_y)
  table.insert(entities, pet)
  
  if not is_running then
    M.start()
  end
end

return M
