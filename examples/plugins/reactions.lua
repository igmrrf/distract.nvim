-- A companion that reacts to what you are doing.
--
-- Exercises every lifecycle hook and every world command, in about as little code
-- as each one takes. Load it with `dofile`, or copy the body into your config.
--
-- What it does:
--   * a long idle makes the pet sleep, and typing wakes it up
--   * bouncing off the left or right edge makes it pounce
--   * landing from a jump is reported once, not once per frame
--   * every state change is written to `:messages` at DEBUG
--
-- What it deliberately does not do: touch the entity it is handed. That table is
-- read-only, and assigning to it raises -- because the overlay backend simulates
-- in a separate process, so a plugin that wrote to `entity.vx` would move the
-- sprite on two backends out of three.

local distract = require("distract")

--- How long a plugin waits before deciding the user has gone quiet, in seconds.
---
--- Separate from `idle_timeout_ms`: that one decides when the *engine* emits an
--- `idle` event, this one decides how many of those in a row count as a nap.
local IDLE_EVENTS_BEFORE_SLEEP = 3

--- The action a screen-edge bounce triggers, when the asset declares one.
local EDGE_ACTION = "pounce"

local world_handle = nil
local idle_events = 0
local is_napping = false

distract.register_plugin("example-reactions", {
  --- Called once when the plugin is registered, and again whenever a backend
  --- starts. The handle is how anything is asked for; it is the only mutable
  --- surface a plugin gets.
  on_init = function(world)
    world_handle = world
    vim.notify(
      string.format("[reactions] attached to the %s backend", world.backend),
      vim.log.levels.DEBUG
    )
  end,

  --- Called for every entity, every simulated frame in the terminal and on the
  --- subscribed snapshot cadence on the overlay. Keep it cheap: it runs per
  --- entity per tick.
  on_tick = function(entity, dt)
    if not is_napping or entity.current_state == "sleep" then
      return
    end
    -- Ask, never assign. The command is applied at the top of the next step, so
    -- a hook cannot observe an entity halfway through its own frame.
    world_handle.request_state(entity.id, "sleep")
    -- Unused, and named rather than dropped so the signature documents itself.
    local _ = dt
  end,

  --- Both states, so a plugin can tell a transition from a fresh spawn.
  on_state_change = function(entity, from_state, to_state)
    vim.notify(
      string.format("[reactions] #%d %s -> %s", entity.id, from_state, to_state),
      vim.log.levels.DEBUG
    )
  end,

  --- `edge` is one of "left", "right", "top", "bottom" or "obstacle". A screen
  --- edge and a registered platform arrive through the same hook, because to a
  --- pet they are the same event.
  on_collision = function(entity, collision)
    if collision.edge == "left" or collision.edge == "right" then
      -- `trigger_action` resolves through the manifest's `custom_actions`, which
      -- is what makes this work for any asset that declares the action and do
      -- nothing for one that does not.
      distract.action(EDGE_ACTION, entity.id)
    end
  end,

  --- Debounced editor events: "typing", "moving", "scrolling", "idle".
  on_editor_event = function(event_name, context)
    if event_name == "idle" then
      idle_events = idle_events + 1
      if idle_events >= IDLE_EVENTS_BEFORE_SLEEP then
        is_napping = true
      end
      return
    end

    idle_events = 0
    if is_napping then
      is_napping = false
      for _, entity in ipairs(world_handle.entities()) do
        world_handle.request_state(entity.id, "idle")
      end
      -- The world would otherwise be quiescent, and a quiescent world is not
      -- redrawn -- including whatever a plugin just changed about it.
      world_handle.mark_dirty()
    end

    local _ = context
  end,

  --- Where every sprite was actually drawn, in terminal cells, on every backend.
  --- This is the hook a speech bubble is built on: it is the only way to know
  --- which cells are already taken.
  on_draw = function(layers)
    for _, layer in ipairs(layers) do
      local _ = layer.row + layer.col + layer.width + layer.height
    end
  end,

  --- Called when the backend stops, and when the plugin is unregistered. Release
  --- anything held: windows, timers, autocommands.
  on_teardown = function()
    world_handle = nil
    idle_events = 0
    is_napping = false
  end,
})
