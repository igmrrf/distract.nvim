local Cat = {}
Cat.__index = Cat

local frames = {
  idle = { "(^・ω・^ )", "( ^・ω・^)", "(  ^・ω・)" },
  walk_right = { " ∫(^・ω・^ )", " ∫( ^・ω・^)", " ∫(  ^・ω・)" },
  walk_left = { "( ^・ω・^)∫ ", "(^・ω・^ )∫ ", "(・ω・^  )∫ " },
}

function Cat.new(id, x, y)
  local self = setmetatable({}, Cat)
  self.id = id
  self.x = x or 0
  self.y = y or 0
  self.tick_count = 0
  self.frame_idx = 1
  self.state = "walk_right"
  return self
end

function Cat:update()
  self.tick_count = self.tick_count + 1
  
  -- Animate 3 frames per second (since tick is 30fps)
  if self.tick_count % 10 == 0 then
    self.frame_idx = self.frame_idx + 1
  end

  -- Move logic
  if self.state == "walk_right" then
    self.x = self.x + 0.2 -- Move speed per tick
    if self.x > vim.o.columns - 15 then
      self.state = "walk_left"
    end
  elseif self.state == "walk_left" then
    self.x = self.x - 0.2
    if self.x < 2 then
      self.state = "walk_right"
    end
  end
end

function Cat:get_render_state()
  local f = frames[self.state]
  local sprite = f[((self.frame_idx - 1) % #f) + 1]
  return {
    x = self.x,
    y = self.y,
    sprite = sprite
  }
end

return Cat
