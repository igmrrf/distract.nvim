require("tests.test_harness")

--- The 3D render mode in the terminal.
---
--- The mesh itself is pinned to the overlay by `tests/voxel_parity_spec.lua`; this
--- covers the settings block and the software rasteriser that turns a mesh back
--- into the sprite canvas the half-block and kitty renderers draw.

local raster3d = require("distract.raster3d")
local render = require("distract.render")
local sprites = require("distract.terminal_sprites")
local voxel = require("distract.voxel")

local CAT = "cat"

local function silhouette(matrix)
  local rows = {}
  for row = 1, #matrix do
    local cells = {}
    for col = 1, #matrix[row] do
      cells[col] = matrix[row][col] and "#" or "."
    end
    rows[row] = table.concat(cells)
  end
  return table.concat(rows, "\n")
end

local function opaque_count(matrix)
  local count = 0
  for _, row in ipairs(matrix) do
    for _, pixel in ipairs(row) do
      if pixel then
        count = count + 1
      end
    end
  end
  return count
end

local function brightest(matrix)
  local best = -1
  for _, row in ipairs(matrix) do
    for _, pixel in ipairs(row) do
      if pixel then
        best = math.max(best, pixel[1] + pixel[2] + pixel[3])
      end
    end
  end
  return best
end

local function with_mode(config)
  sprites.configure_render(render.settings(config))
  sprites.bind_manifest(CAT, require("distract.manifests.cat"))
end

describe("render settings", function()
  after_each(function()
    sprites.configure_render(render.DEFAULTS)
  end)

  it("defaults to the flat mode every asset has always been drawn in", function()
    local settings = render.settings(nil)
    assert.are_equal(render.FLAT, settings.mode)
    assert.is_false(render.is_voxel(settings, nil))
  end)

  it("refuses an unknown mode rather than quietly drawing the default", function()
    assert.is_false(pcall(render.settings, { mode = "3D " }))
    assert.is_false(pcall(render.settings, { mode = "voxels" }))
  end)

  it("clamps every numeric field into its documented range", function()
    local settings = render.settings({
      mode = "3d",
      fov_y_degrees = 5000,
      depth_per_unit = 40,
      voxel_max_width = 100000,
      voxel_depth = 0,
      light = { ambient = 12 },
    })
    assert.are_equal(render.MAX_FOV_Y_DEGREES, settings.fov_y_degrees)
    assert.are_equal(render.MAX_DEPTH_PER_UNIT, settings.depth_per_unit)
    assert.are_equal(render.MAX_VOXEL_MAX_WIDTH, settings.voxel_max_width)
    assert.are_equal(1, settings.voxel_depth, "a zero-depth slab has no geometry")
    assert.are_equal(1, settings.light.ambient)
  end)

  it("refuses a non-numeric field instead of coercing it", function()
    assert.is_false(pcall(render.settings, { fov_y_degrees = "wide" }))
    assert.is_false(pcall(render.settings, { light = { direction = { 1, 2 } } }))
  end)

  it("normalises the light direction, and falls back when there is none", function()
    local straight_down = render.light_direction({ light = { direction = { 0, 8, 0 } } })
    assert.are.same({ 0, 1, 0 }, straight_down)
    assert.are.same({ 0, 1, 0 }, render.light_direction({ light = { direction = { 0, 0, 0 } } }))
  end)

  it("lets a manifest pin itself to a mode the configuration disagrees with", function()
    local voxel_session = render.settings({ mode = "3d" })
    assert.is_true(render.is_voxel(voxel_session, { name = "pet" }))
    assert.is_false(render.is_voxel(voxel_session, { name = "bubble", render = "2d" }))

    local flat_session = render.settings({ mode = "2d" })
    assert.is_true(render.is_voxel(flat_session, { name = "pet", render = "3d" }))
  end)

  it("refuses a manifest that declares a mode that does not exist", function()
    local settings = render.settings(nil)
    assert.is_false(pcall(render.mode_for, settings, { name = "pet", render = "isometric" }))
  end)
end)

describe("the software rasteriser", function()
  after_each(function()
    sprites.configure_render(render.DEFAULTS)
    sprites.bind_manifest(CAT, require("distract.manifests.cat"))
  end)

  it("keeps the sprite's own canvas, because that is the cell footprint", function()
    with_mode({ mode = "2d" })
    local flat = sprites.pixel_matrix(CAT, 1, false)
    with_mode({ mode = "3d" })
    local model = sprites.pixel_matrix(CAT, 1, false)

    assert.are_equal(#flat, #model, "row count")
    assert.are_equal(#flat[1], #model[1], "column count")
  end)

  it("covers exactly the sprite's pixels when the model faces the viewer", function()
    -- The strongest statement this design can make: with no yaw the projection
    -- of the slab's front face is the sprite itself, so switching a session to 3D
    -- cannot move or reshape a pet until something actually turns it.
    with_mode({ mode = "2d" })
    local flat = silhouette(sprites.pixel_matrix(CAT, 1, false))
    with_mode({ mode = "3d", yaw_degrees = 0 })
    local model = silhouette(sprites.pixel_matrix(CAT, 1, false))

    assert.are_equal(flat, model)
  end)

  it("is indistinguishable from a mirror when nothing is turned", function()
    -- Worth pinning rather than assuming: at a yaw of zero the pet faces the
    -- viewer and its mirror faces left, and the side faces project to no width at
    -- all, so the two really are the same picture. Any claim that a turn differs
    -- from a mirror has to be made at an angle.
    with_mode({ mode = "3d", yaw_degrees = 0 })
    local facing = sprites.pixel_matrix(CAT, 1, false)
    local flipped = sprites.pixel_matrix(CAT, 1, true)
    assert.are_equal(silhouette(sprites.mirror_matrix(facing)), silhouette(flipped))
  end)

  it("turns the model rather than mirroring it when the pet faces left", function()
    -- Facing is a yaw, not a mirror: mirroring would swap which side the light
    -- falls on, so a pet turning round would appear to move the sun.
    with_mode({ mode = "3d", yaw_degrees = 35 })
    local facing = sprites.pixel_matrix(CAT, 1, false)
    local flipped = sprites.pixel_matrix(CAT, 1, true)
    local mirrored = sprites.mirror_matrix(facing)

    local differing = 0
    for row = 1, #flipped do
      for col = 1, #flipped[row] do
        local turned, reflected = flipped[row][col], mirrored[row][col]
        local same = (not turned and not reflected)
          or (
            turned
            and reflected
            and turned[1] == reflected[1]
            and turned[2] == reflected[2]
            and turned[3] == reflected[3]
          )
        if not same then
          differing = differing + 1
        end
      end
    end
    assert.is_true(differing > 0, "a turned model is not a mirrored one")
  end)

  it("draws something at every yaw, and something different", function()
    with_mode({ mode = "3d", yaw_degrees = 0 })
    local face_on = sprites.pixel_matrix(CAT, 1, false)
    with_mode({ mode = "3d", yaw_degrees = 60 })
    local turned = sprites.pixel_matrix(CAT, 1, false)

    assert.is_true(opaque_count(turned) > 0, "a turned model still has pixels")
    assert.are_not_equal(silhouette(face_on), silhouette(turned))
  end)

  it("lights a model, and an ambient of one flattens the lighting away", function()
    with_mode({ mode = "3d", light = { ambient = 1.0 } })
    local flat_lit = brightest(sprites.pixel_matrix(CAT, 1, false))
    with_mode({ mode = "3d", light = { ambient = 0.0, direction = { 0, 0, -1 } } })
    local shadowed = brightest(sprites.pixel_matrix(CAT, 1, false))

    assert.is_true(flat_lit > shadowed, "the Lambertian term did nothing")
  end)

  it("reuses a rasterised frame rather than rebuilding it per draw", function()
    with_mode({ mode = "3d" })
    local first = raster3d.matrix(CAT, 1, false)
    local second = raster3d.matrix(CAT, 1, false)
    assert.is_true(first == second, "the cache returned a different table")

    raster3d.reset(CAT)
    assert.is_false(first == raster3d.matrix(CAT, 1, false), "reset must drop the frame")
  end)

  it("has nothing to draw for an entirely transparent frame", function()
    local blank = {}
    for row = 1, 4 do
      blank[row] = { false, false, false, false }
    end
    local mesh = voxel.build(blank, {})
    assert.are_equal(0, #mesh.indices)
    assert.are_equal(0, opaque_count(raster3d.rasterise(mesh, 0)))
  end)
end)

describe("mode selection in the terminal renderer", function()
  after_each(function()
    sprites.configure_render(render.DEFAULTS)
    sprites.bind_manifest("pinned_probe", nil)
    sprites.bind_manifest(CAT, require("distract.manifests.cat"))
  end)

  it("follows the configured mode for an asset that pins nothing", function()
    with_mode({ mode = "3d" })
    assert.is_true(sprites.is_voxel(CAT))
    with_mode({ mode = "2d" })
    assert.is_false(sprites.is_voxel(CAT))
  end)

  it("honours a manifest that pinned itself flat in a 3D session", function()
    sprites.configure_render(render.settings({ mode = "3d" }))
    sprites.bind_manifest("pinned_probe", { name = "pinned_probe", render = "2d" })
    assert.is_false(sprites.is_voxel("pinned_probe"))
    assert.is_true(sprites.is_voxel(CAT), "and leaves everything else alone")
  end)

  it("re-renders every cached frame when the mode changes", function()
    with_mode({ mode = "2d" })
    local flat_lines = sprites.get_rendered_frame(CAT, 1, false)
    with_mode({ mode = "3d", yaw_degrees = 60 })
    local model_lines = sprites.get_rendered_frame(CAT, 1, false)

    assert.are_not_equal(
      table.concat(flat_lines, "\n"),
      table.concat(model_lines, "\n"),
      "a stale cache would draw the flat sprite in a 3D session"
    )
  end)
end)
