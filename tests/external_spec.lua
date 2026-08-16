require("tests.test_harness")
local external = require("distract.external")
local distract = require("distract")

describe("distract.external IPC message handling", function()
  it("should handle ready response", function()
    external.setup(distract.config)
    assert.has_no.errors(function()
      external.handle_ipc_message('{"status":"ready","version":"0.2.0"}')
    end)
  end)

  it("should handle spawned response", function()
    assert.has_no.errors(function()
      external.handle_ipc_message('{"status":"spawned","id":1,"asset_name":"cat","state":"idle"}')
    end)
  end)

  it("should handle action_triggered response", function()
    assert.has_no.errors(function()
      external.handle_ipc_message(
        '{"status":"action_triggered","id":1,"asset_name":"cat","action":"jump","state":"jump"}'
      )
    end)
  end)

  it("should handle despawned and cleared responses", function()
    assert.has_no.errors(function()
      external.handle_ipc_message('{"status":"despawned","id":1}')
      external.handle_ipc_message('{"status":"cleared"}')
    end)
  end)

  it("should handle status_report responses (empty and populated)", function()
    assert.has_no.errors(function()
      external.handle_ipc_message('{"status":"status_report","count":0,"entities":[]}')
      external.handle_ipc_message(
        '{"status":"status_report","count":2,"entities":[{"id":1,"asset_name":"cat","state":"walk","x":100.0,"y":50.0},{"id":2,"asset_name":"sun","state":"shining","x":400.0,"y":100.0}]}'
      )
    end)
  end)

  it("should handle error responses gracefully", function()
    assert.has_no.errors(function()
      external.handle_ipc_message(
        '{"status":"error","code":"SPAWN_FAILED","message":"Unknown asset"}'
      )
    end)
  end)

  it("should handle malformed JSON without raising errors", function()
    assert.has_no.errors(function()
      external.handle_ipc_message("")
      external.handle_ipc_message("not a valid json")
      external.handle_ipc_message("{ incomplete json")
      external.handle_ipc_message("12345")
    end)
  end)
end)

describe("distract.external command dispatchers", function()
  it("spawn, trigger_action, despawn, clear, get_status methods function correctly", function()
    assert.has_no.errors(function()
      distract.setup()
      external.setup(distract.config)

      -- Test calling command builders
      external.spawn("cat", { x = 100, y = 200, flip_x = true })
      external.spawn("crab")
      external.spawn("sun")

      external.trigger_action("jump", 1)
      external.trigger_action("clip", "crab")
      external.trigger_action("eclipse", nil)

      external.despawn(1)
      external.get_status()
      external.send_event("typing", { speed = 5 })
      external.update_grid()
      external.clear()
    end)
  end)
end)

-- The overlay would reject a violating manifest on arrival, but the message
-- would come back through the IPC error path rather than as the clean refusal
-- the terminal backend gives. One manifest, one message, either backend.
describe("distract.external capability gating", function()
  local external = require("distract.external")

  it("refuses a manifest that breaks its own capabilities before sending it", function()
    external.setup({
      cell_width = 10,
      cell_height = 20,
      assets = {
        impossible = {
          name = "impossible",
          initial_state = "orbit",
          locomotion = "grounded",
          states = {
            orbit = {
              animation = { frames = { 0 }, fps = 1.0, loop_anim = true },
              physics = { path_type = "orbital" },
            },
          },
        },
      },
    })

    local sent = nil
    local orig_send, orig_running = external.send_command, external.is_running
    local orig_notify = external.is_running and vim.notify
    local errors = {}
    external.is_running = function()
      return true
    end
    external.send_command = function(cmd)
      sent = cmd
      return true
    end
    vim.notify = function(message, level)
      if level and level >= vim.log.levels.ERROR then
        errors[#errors + 1] = message
      end
    end

    external.spawn("impossible")

    vim.notify = orig_notify
    external.send_command, external.is_running = orig_send, orig_running

    assert.is_nil(sent, "a grounded orbit must not reach the overlay at all")
    assert(#errors > 0, "the refusal must be reported, not silent")
    assert(
      errors[1]:find("orbit"),
      "the message must name the offending state, got: " .. tostring(errors[1])
    )
  end)
end)

describe("distract.external spawn coordinates", function()
  local external = require("distract.external")

  it("converts spawn coordinates from terminal cells to overlay pixels", function()
    external.setup({ cell_width = 10, cell_height = 20 })

    local sent = nil
    local orig_send = external.send_command
    local orig_running = external.is_running
    external.is_running = function()
      return true
    end
    external.send_command = function(cmd)
      sent = cmd
      return true
    end

    external.spawn("cat", { x = 40, y = 12 })

    external.send_command = orig_send
    external.is_running = orig_running

    assert.is_not_nil(sent, "spawn should have sent a command")
    assert.are_equal(400, sent.x, "column 40 on a 10px cell is pixel 400, not pixel 40")
    assert.are_equal(240, sent.y, "line 12 on a 20px cell is pixel 240")
  end)

  it("leaves the engine to choose a position when none is given", function()
    external.setup({ cell_width = 10, cell_height = 20 })

    local sent = nil
    local orig_send = external.send_command
    local orig_running = external.is_running
    external.is_running = function()
      return true
    end
    external.send_command = function(cmd)
      sent = cmd
      return true
    end

    external.spawn("cat", {})

    external.send_command = orig_send
    external.is_running = orig_running

    assert.is_nil(sent.x)
    assert.is_nil(sent.y)
  end)
end)

-- The overlay is told where the floor is rather than working it out: only the
-- editor can see `cmdheight`, the statusline and where the buffer text ends.
describe("distract.external placement", function()
  local external = require("distract.external")
  local position = require("distract.position")

  local function configured(position_config)
    external.setup({
      cell_width = 10,
      cell_height = 20,
      position = position_config,
      assets = {},
    })
  end

  it("resolves the auto anchor against what the entity can physically do", function()
    configured(nil)
    local grounded = { initial_state = "walk", locomotion = "grounded", states = { walk = {} } }
    local floating = {
      initial_state = "drift",
      locomotion = "omnidirectional",
      states = { drift = {} },
    }

    assert.are_equal(position.BOTTOM, external.resolve_placement(grounded, {}).anchor)
    assert.are_equal(position.FREE, external.resolve_placement(floating, {}).anchor)
  end)

  it("sends no anchor when the spawn already says where to go", function()
    configured(nil)
    local placement = external.resolve_placement(nil, { x = 5, y = 6 })
    assert.is_nil(placement.anchor, "an explicit position leaves nothing to anchor")
    assert.are_equal(5, placement.x)
    assert.are_equal(6, placement.y)
  end)

  it("unpacks a table anchor into coordinates the engine understands", function()
    configured({ anchor = { x = 3, y = 4, z = 2 } })
    local placement = external.resolve_placement(nil, {})
    assert.is_nil(placement.anchor)
    assert.are_equal(3, placement.x)
    assert.are_equal(4, placement.y)
    assert.are_equal(2, placement.z)
  end)

  it("computes the parallax the overlay can actually honour", function()
    configured({ parallax = { per_unit = 0.2, min = 0.4, max = 1.6 } })
    assert.are_equal(1.4, external.resolve_placement(nil, { z = 2 }).parallax)
    assert.are_equal(1.0, external.resolve_placement(nil, {}).parallax)
  end)

  it("remembers the floor it was given and does not resend an unchanged one", function()
    configured(nil)
    -- Whatever a previous spawn measured, from a known starting point.
    external.set_ground_row(nil)

    local sent = 0
    local original = external.send_command
    external.send_command = function()
      sent = sent + 1
      return true
    end

    external.set_ground_row(24)
    external.set_ground_row(24)
    external.send_command = original

    assert.are_equal(24, external.get_ground_row())
    assert.are_equal(1, sent, "a floor that has not moved is not worth a message")
  end)
end)
