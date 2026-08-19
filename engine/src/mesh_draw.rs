//! Which models get drawn where.
//!
//! The pure half of the mesh pass: entity state to draw list, with no wgpu type
//! in it, so the mapping is testable without a GPU. `gpu3d.rs` is the half that
//! talks to the device — the same split `gpu.rs` already has between
//! `build_instances` and `GpuRenderer`.

use bytemuck::{Pod, Zeroable};

use crate::camera::Camera;
use crate::ecs::World;
use crate::entity::Entity;
use crate::manifest::WrapMode;
use crate::meshbook::MeshBook;
use crate::render::{RenderMode, RenderSettings};
use crate::wrap;

/// One model, placed. 32 bytes per drawn entity.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct MeshInstance {
    /// Where the model's top centre goes, in pixels, and its yaw in radians.
    pub placement: [f32; 4],
    /// Pixels per voxel on each axis, and the model's opacity.
    pub scaling: [f32; 4],
}

/// Every entity showing one frame of one asset, and where that frame's geometry
/// is.
///
/// Grouped because a hundred pets of the same asset in the same animation frame
/// are one instanced draw, and because the alternative — a draw call per entity —
/// is what makes a mesh renderer fall over at the entity counts `tick_budget`
/// already measures.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshDraw {
    pub first_index: u32,
    pub index_count: u32,
    pub instances: Vec<MeshInstance>,
}

/// One frame's mesh work: what to draw, and which instances belong to each draw.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshFrame {
    pub draws: Vec<MeshDraw>,
    pub slices: Vec<std::ops::Range<u32>>,
}

impl MeshFrame {
    pub fn is_empty(&self) -> bool {
        self.draws.is_empty()
    }
}

/// Whether anything in this world would be drawn as a mesh.
///
/// A 2D session must not pay to extrude every frame of every asset, so the book
/// is only ever built when the configuration asks for meshes or an asset pins
/// itself to them.
pub fn world_needs_meshes(world: &World) -> bool {
    if world.render.mode.is_voxel() {
        return true;
    }
    world
        .asset_manager
        .iter()
        .any(|(_, asset)| asset.manifest.render.is_some_and(RenderMode::is_voxel))
}

/// Which entities the mesh pass draws.
///
/// An asset's manifest may pin itself to one mode; everything else follows the
/// configuration.
pub fn draws_in_voxel_mode(entity: &Entity, world: &World) -> bool {
    let declared = world
        .asset_manager
        .get(&entity.asset_name)
        .and_then(|asset| asset.manifest.render);
    declared.unwrap_or(world.render.mode).is_voxel()
}

/// Builds the per-frame mesh draw list from the world.
///
/// Free of any wgpu type, so the mapping from entity state to draw call is
/// testable without a GPU — the same split `build_instances` has in `gpu.rs`.
pub fn build_mesh_draws(world: &World, book: &MeshBook, camera: &Camera) -> Vec<MeshDraw> {
    let settings = &world.render;
    let bounds = world.bounds();
    let mut grouped: Vec<MeshDraw> = Vec::new();

    for entity in world.entities.iter().filter(|entity| entity.is_active) {
        if !draws_in_voxel_mode(entity, world) {
            continue;
        }
        let Some(asset) = world.asset_manager.get(&entity.asset_name) else {
            continue;
        };
        let Some(state_def) = asset.manifest.states.get(&entity.current_state) else {
            continue;
        };
        let animation = &state_def.animation;
        if animation.frames.is_empty() {
            continue;
        }

        let frame = animation.frames[entity.frame_idx % animation.frames.len()];
        let Some(range) = book.range(&entity.asset_name, frame) else {
            continue;
        };
        if range.index_count == 0 || range.extent[0] == 0 || range.extent[1] == 0 {
            continue;
        }

        // The footprint is the sprite's, unscaled by parallax: the projection
        // performs the depth shrink in this mode, and applying both would
        // compound two mechanisms for one cue.
        let footprint = [
            asset.frame_w as f32 * world.sprite_scale_x,
            asset.frame_h as f32 * world.sprite_scale_y,
        ];
        let voxel_px = [
            footprint[0] / range.extent[0] as f32,
            footprint[1] / range.extent[1] as f32,
        ];
        let scaling = [voxel_px[0], voxel_px[1], voxel_px[0], 1.0];
        let yaw = yaw_for(entity, settings);
        let depth_px = camera.depth_px(entity.z);

        let placements = if state_def.physics.wrap_mode == WrapMode::Wrap {
            wrap::offsets((entity.x, entity.y), (footprint[0], footprint[1]), bounds)
        } else {
            vec![wrap::Offset { dx: 0.0, dy: 0.0 }]
        };

        let group = group_for(&mut grouped, range.first_index, range.index_count);
        for placement in placements {
            group.instances.push(MeshInstance {
                placement: [
                    entity.x + placement.dx + footprint[0] * 0.5,
                    entity.y + placement.dy,
                    depth_px,
                    yaw,
                ],
                scaling,
            });
        }
    }

    grouped.retain(|draw| !draw.instances.is_empty());
    grouped
}

/// The turn a model is drawn at.
///
/// Facing is a yaw rather than a mirror in this mode: a mirrored model would
/// swap which side the light falls on, so a pet turning round would appear to
/// move the sun.
fn yaw_for(entity: &Entity, settings: &RenderSettings) -> f32 {
    let base = settings.yaw_degrees.to_radians();
    if entity.flip_x {
        std::f32::consts::PI - base
    } else {
        base
    }
}

fn group_for(grouped: &mut Vec<MeshDraw>, first_index: u32, index_count: u32) -> &mut MeshDraw {
    if let Some(position) = grouped
        .iter()
        .position(|draw| draw.first_index == first_index)
    {
        return &mut grouped[position];
    }
    grouped.push(MeshDraw {
        first_index,
        index_count,
        instances: Vec::new(),
    });
    grouped.last_mut().expect("just pushed")
}
