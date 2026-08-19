# Voxel 3D rendering plan — 2026-08-19

Supersedes decision 1 of
[`2026-08-19-full-feature-completion.md`](2026-08-19-full-feature-completion.md),
which recorded "2D is the contract, 3D is not built". 3D is now in scope. The
reasoning that decision gave is still sound and this plan is built to satisfy it
rather than to ignore it: **no backend forks off the manifest contract, and no
asset has to be authored twice.**

## The idea in one paragraph

Every asset already resolves to RGBA frames. Extrude a frame's opaque pixels into
a slab of cubes and you have a real 3D model of that frame — the paper-craft look
of a voxel pet — with no new art, no mesh format and no importer change. 3D is
therefore a **rendering mode over the existing asset pipeline**, not a second
asset pipeline. The simulation, the manifests, the placement vocabulary, the
obstacles and the parity harnesses are all untouched: they describe where a thing
is, and this decides how it is drawn.

## Decisions taken, so nothing stalls on a question

1. **The voxel model is derived, never authored.** `voxel::build` turns one RGBA
   frame into a mesh. No OBJ, no glTF, no separate 3D asset per pet. A format
   loader would mean every existing and imported asset is 2D-only forever, which
   is the opposite of the goal.

2. **The z = 0 plane renders identically in both modes.** The perspective camera
   sits at `d = (viewport_h / 2) / tan(fov_y / 2)` looking down `-Z` at the
   viewport centre, so the plane an entity with `z = 0` lives on maps 1:1 to
   pixels. Turning 3D on must not move a pet that never asked for depth, and that
   is a testable property rather than a hope: `camera` has a unit test asserting
   the perspective and orthographic projections agree on that plane.

3. **`z` becomes real depth, and parallax stays derived.** Depth already exists in
   the manifest and the placement vocabulary. In 3D the projection performs the
   shrink that `parallax` fakes in 2D, so `parallax` continues to damp *motion*
   and stops multiplying *size*: two mechanisms scaling the same sprite would
   compound.

4. **Both engines render 3D, from the same mesh.** The overlay rasterises on the
   GPU. The in-terminal backends rasterise in Lua into the sprite canvas they
   already draw, z-buffered, and cache by `(asset, frame, yaw bucket)` — so
   steady-state cost is a table lookup, the same as a sprite frame today. A
   half-block backend that could not do 3D would fork the contract, which
   decision 1 of the superseded plan was right to refuse.

5. **The mesh is a parity artifact.** `engine/tests/voxel_parity.rs` writes
   goldens; `tests/voxel_parity_spec.lua` asserts Lua reproduces them. Same shape
   as the sprite and physics harnesses, same reason: two implementations of one
   contract diverge silently otherwise. The golden is the mesh, not the picture:
   comparing rasterised pixels would fold two different divergences into one
   number.

6. **The voxel grid is capped, and it is capped by resampling.** A 192×208 pet
   frame is 39,936 pixels; a naive extrusion of 74 such frames is millions of
   triangles. `VOXEL_MAX_WIDTH = 48` resamples nearest-neighbour before
   extruding, exactly as `TERMINAL_SPRITE_MAX_WIDTH = 32` already does for the
   half-block renderer. Interior faces are culled: a face is emitted only where
   its neighbour is transparent.

7. **Per-entity override, so hybrid comes free.** A manifest may declare
   `render = "2d"` or `"3d"`; the mode is the default for entities that declare
   nothing. The 3D pass and the 2D pass composite into the same scene texture, so
   a flat UI sprite over a voxel pet needs no new machinery.

8. **Lighting is geometric, and it does not replace the sprite shader.** One
   directional light plus ambient, per-face normals, colour from the source pixel.
   The analytical shading in `sprite_gen` is how a 2D sprite gets its form; in 3D
   the geometry does that job and a second baked-in gradient would fight it.

9. **Depth needs a depth buffer, so the 3D pass is its own pass.** A render
   pass's depth attachment applies to every pipeline in it, so the mesh pass
   (depth-tested, `Depth32Float`) and the sprite pass (no depth, painter's order
   by `z_index`) cannot be one pass. Mesh first, sprites over the top.

## Sequence

Each step lands green, with its own tests, and nothing later depends on a
half-finished earlier step.

| # | Step | Lands |
|---|---|---|
| 1 | `engine/src/camera.rs` — projection math, no wgpu types | unit tests, including the z = 0 agreement property |
| 2 | `engine/src/voxel.rs` — RGBA frame to mesh | unit tests on face culling, colour, bounds |
| 3 | `engine/src/meshbook.rs` — every frame of every asset in one buffer pair | unit tests mirroring `atlas.rs` |
| 4 | `lua/distract/camera.lua`, `lua/distract/voxel.lua` | mirrors of 1 and 2 |
| 5 | `engine/tests/voxel_parity.rs` + `tests/voxel_parity_spec.lua` | goldens in `tests/fixtures/voxels/` |
| 6 | `engine/src/shader3d.wgsl`, `engine/src/gpu3d.rs`, depth target in `gpu.rs` | headless GPU test rendering a real mesh |
| 7 | `lua/distract/raster3d.lua` + `terminal_sprites` wiring | specs, and a `tools/preview_sprite.lua --3d` view |
| 8 | `render` config block, `UpdateRender` IPC, `:Distract` verb | specs both sides, IPC contract test |
| 9 | `doc/distract.txt`, `docs/configuration.md`, README, CHANGELOG | documentation gate |

## What this must not break

- Every existing golden: sprite parity, physics parity, the 17 screenshots.
- The 2D path when `render.mode` is `"2d"`, which stays the default.
- `on_draw`, which reports cells: a voxel pet occupies the same cell footprint as
  its sprite, because the footprint is what the physics measures against.
