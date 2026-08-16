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
