//! The per-frame sprite draw list: which quads the overlay draws, and where.
//!
//! The flat counterpart to `mesh_draw.rs`, and split from `gpu.rs` for the same
//! reason that one was split from `gpu3d.rs`: deciding what to draw is pure
//! logic over the world, and holding it next to the pass recording made the
//! renderer a file nobody could read the top of. Nothing here touches a wgpu
//! type, so every mapping from entity state to draw call is tested without a
//! GPU.

use crate::atlas::Atlas;
use crate::ecs::World;
use crate::gpu_setup::SpriteInstance;
use crate::manifest::WrapMode;
use crate::wrap;

/// Builds the per-frame instance list from the world, z-sorted.
///
/// Kept free of any wgpu type so the mapping from entity state to draw call can
/// be tested without a GPU.
pub fn build_instances(world: &World, atlas: &Atlas) -> Vec<SpriteInstance> {
    let mut sorted: Vec<&crate::entity::Entity> =
        world.entities.iter().filter(|e| e.is_active).collect();
    sorted.sort_by_key(|e| e.z_index);

    let (scale_x, scale_y) = (world.sprite_scale_x, world.sprite_scale_y);
    let bounds = world.bounds();
    let mut out = Vec::with_capacity(sorted.len());

    for entity in sorted {
        let Some(asset) = world.asset_manager.get(&entity.asset_name) else {
            continue;
        };
        let Some(state_def) = asset.manifest.states.get(&entity.current_state) else {
            continue;
        };
        let anim = &state_def.animation;
        if anim.frames.is_empty() {
            continue;
        }

        let frame = anim.frames[entity.frame_idx % anim.frames.len()];
        let flip = entity.flip_x ^ anim.flip_x;
        let Some(uv) = atlas.uv(&entity.asset_name, frame, flip) else {
            continue;
        };

        // Depth is drawn as well as simulated: a distant sprite is smaller by
        // the same factor that damps its motion, which is the whole reason the
        // overlay can express parallax and the half-block renderer cannot.
        let size = [
            asset.frame_w as f32 * scale_x * entity.parallax,
            asset.frame_h as f32 * scale_y * entity.parallax,
        ];

        // A wrapping sprite is drawn again at each complementary position, so the
        // half that has left one edge arrives at the other in the same frame
        // rather than popping across a tick later. The pass is scissored to the
        // bounds, so the part of each extra quad that falls outside is clipped.
        let placements = if state_def.physics.wrap_mode == WrapMode::Wrap {
            wrap::offsets((entity.x, entity.y), (size[0], size[1]), bounds)
        } else {
            vec![wrap::Offset { dx: 0.0, dy: 0.0 }]
        };

        for placement in placements {
            out.push(SpriteInstance {
                pos: [entity.x + placement.dx, entity.y + placement.dy],
                size,
                uv_min: [uv[0], uv[1]],
                uv_max: [uv[2], uv[3]],
            });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::Atlas;
    use crate::ecs::World;
    use crate::spawn::SpawnOptions;

    fn world_with(entities: &[(&str, f32, f32)]) -> World {
        let mut world = World::new(800.0, 600.0);
        world.sprite_scale_x = 1.0;
        world.sprite_scale_y = 1.0;
        for (name, x, y) in entities {
            world.spawn(name, None, SpawnOptions::at(*x, *y)).unwrap();
        }
        world
    }

    #[test]
    fn one_instance_per_visible_entity() {
        let world = world_with(&[("cat", 10.0, 20.0), ("crab", 30.0, 40.0)]);
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();
        let instances = build_instances(&world, &atlas);
        assert_eq!(instances.len(), 2);
    }

    #[test]
    fn instances_carry_pixel_position_and_scaled_size() {
        let mut world = world_with(&[("cat", 10.0, 20.0)]);
        world.sprite_scale_x = 4.0;
        world.sprite_scale_y = 4.0;
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();
        let instances = build_instances(&world, &atlas);

        let cat = world.asset_manager.get("cat").unwrap();
        assert_eq!(instances[0].pos, [10.0, 20.0]);
        assert_eq!(
            instances[0].size,
            [cat.frame_w as f32 * 4.0, cat.frame_h as f32 * 4.0]
        );
    }

    #[test]
    fn a_wrapping_sprite_at_the_edge_is_drawn_at_both_edges() {
        let mut world = world_with(&[("cat", 10.0, 20.0)]);
        let cat_width = world.asset_manager.get("cat").unwrap().frame_w as f32;
        // The cat's `idle` clamps and its `walk` wraps, so the state has to be
        // the wrapping one for there to be a departing half at all.
        world.entities[0].current_state = "walk".to_string();
        // Straddling the right edge: `wrap` only teleports once it is entirely
        // past, so this is a position the simulation really produces.
        world.entities[0].x = 800.0 - cat_width / 2.0;

        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();
        let instances = build_instances(&world, &atlas);

        assert_eq!(
            instances.len(),
            2,
            "the departing half has to arrive at the other edge in the same frame"
        );
        assert_eq!(instances[0].pos[0], 800.0 - cat_width / 2.0);
        assert_eq!(instances[1].pos[0], -cat_width / 2.0);
        assert_eq!(instances[0].size, instances[1].size);
        assert_eq!(instances[0].uv_min, instances[1].uv_min);
    }

    #[test]
    fn a_clamped_sprite_is_never_drawn_twice() {
        let mut manifest = crate::manifest::AssetManifest::default_cat();
        manifest.name = "clamped".to_string();
        for state in manifest.states.values_mut() {
            state.physics.wrap_mode = WrapMode::Clamp;
        }

        let mut world = World::new(800.0, 600.0);
        world.sprite_scale_x = 1.0;
        world.sprite_scale_y = 1.0;
        world
            .spawn("clamped", Some(manifest), SpawnOptions::at(790.0, 20.0))
            .unwrap();

        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();
        assert_eq!(
            build_instances(&world, &atlas).len(),
            1,
            "only a wrapping sprite has a departing half to draw"
        );
    }

    #[test]
    fn parallax_draws_a_distant_sprite_smaller() {
        let mut world = world_with(&[("cat", 10.0, 20.0)]);
        world.entities[0].parallax = 0.5;
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();
        let instances = build_instances(&world, &atlas);

        let cat = world.asset_manager.get("cat").unwrap();
        assert_eq!(
            instances[0].size,
            [cat.frame_w as f32 * 0.5, cat.frame_h as f32 * 0.5],
            "depth has to be visible, not only felt in the physics"
        );
    }

    #[test]
    fn instances_are_sorted_back_to_front_by_z_index() {
        // The sun is z -10, the cat z 10, so the sun must be drawn first.
        let world = world_with(&[("cat", 10.0, 10.0), ("sun", 20.0, 20.0)]);
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();
        let instances = build_instances(&world, &atlas);
        assert_eq!(instances[0].pos, [20.0, 20.0], "sun should draw first");
        assert_eq!(instances[1].pos, [10.0, 10.0]);
    }

    #[test]
    fn a_flipped_entity_gets_mirrored_uvs_not_a_second_frame() {
        let mut world = world_with(&[("cat", 10.0, 10.0)]);
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();

        let facing = build_instances(&world, &atlas);
        world.entities[0].flip_x = true;
        let mirrored = build_instances(&world, &atlas);

        assert_eq!(mirrored[0].uv_min[0], facing[0].uv_max[0]);
        assert_eq!(mirrored[0].uv_max[0], facing[0].uv_min[0]);
        assert_eq!(mirrored[0].uv_min[1], facing[0].uv_min[1]);
    }

    #[test]
    fn an_entity_in_an_unknown_state_is_skipped_rather_than_drawn_wrong() {
        let mut world = world_with(&[("cat", 10.0, 10.0)]);
        world.entities[0].current_state = "no_such_state".to_string();
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();
        assert!(build_instances(&world, &atlas).is_empty());
    }

    #[test]
    fn inactive_entities_produce_no_instances() {
        let mut world = world_with(&[("cat", 10.0, 10.0)]);
        world.entities[0].is_active = false;
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();
        assert!(build_instances(&world, &atlas).is_empty());
    }

    #[test]
    fn per_frame_upload_is_bytes_not_megabytes() {
        // The point of the rewrite: a full-screen framebuffer at 4K is 33 MB a
        // frame. Three sprites should cost under a hundred bytes.
        let world = world_with(&[("cat", 1.0, 1.0), ("crab", 2.0, 2.0), ("sun", 3.0, 3.0)]);
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();
        let instances = build_instances(&world, &atlas);
        let bytes = std::mem::size_of_val(&instances[..]);
        assert_eq!(bytes, 3 * 32);
        assert!(bytes < 128);
    }

    #[test]
    fn animation_position_maps_through_the_manifest_frame_list() {
        let mut world = world_with(&[("cat", 0.0, 0.0)]);
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();

        let cat = world.asset_manager.get("cat").unwrap();
        let idle = cat.manifest.states["idle"].animation.frames.clone();
        assert!(idle.len() > 1, "idle should be animated");

        world.entities[0].frame_idx = 0;
        let first = build_instances(&world, &atlas)[0];
        world.entities[0].frame_idx = 1;
        let second = build_instances(&world, &atlas)[0];

        assert_ne!(
            first.uv_min, second.uv_min,
            "advancing the animation must select a different atlas rect"
        );
        assert_eq!(first.uv_min[0], atlas.uv("cat", idle[0], false).unwrap()[0]);
        assert_eq!(
            second.uv_min[0],
            atlas.uv("cat", idle[1], false).unwrap()[0]
        );
    }
}
