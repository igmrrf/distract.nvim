//! Bringing one entity into the world.
//!
//! The Rust counterpart to `lua/distract/entity_spawn.lua`, and split from
//! `ecs.rs` for the same reason: a spawn resolves a manifest, validates its
//! capabilities, decides where the entity stands, seeds the initial state's
//! physics and desynchronises it from everything already alive. None of that is
//! what the world does per frame.
//!
//! `engine/tests/ecs_placement.rs` and `tests/spawn_characterisation_spec.lua`
//! are what make this safe to move: the physics-parity fixtures set an explicit
//! position and then zero the randomised fields, so they barely touch the spawn.

use crate::bounds::Bounds;
use crate::ecs::World;
use crate::entity::Entity;
use crate::manifest::AssetManifest;
use crate::spawn::{Anchor, EntitySeed, SpawnOptions};

/// Where an anchored spawn starts vertically, in overlay pixels.
///
/// `None` when the anchor asks for nothing in particular, or asks for a floor
/// Neovim has not measured yet, leaving the caller's own default to apply.
fn anchored_y(anchor: Option<Anchor>, floor_y: Option<f32>, bounds: Bounds) -> Option<f32> {
    match anchor {
        Some(Anchor::Bottom) => floor_y,
        Some(Anchor::Top) => Some(bounds.top),
        Some(Anchor::Free) | None => None,
    }
}

impl World {
    pub fn spawn(
        &mut self,
        asset_name: &str,
        manifest_opt: Option<AssetManifest>,
        options: SpawnOptions,
    ) -> Result<usize, String> {
        if let Some(manifest) = manifest_opt {
            // Surface the error rather than silently degrading to procedural
            // art: a mistyped spritesheet path used to look like a working
            // spawn with the wrong pictures.
            self.asset_manager.register_manifest(manifest)?;
        }

        let asset = self
            .asset_manager
            .get(asset_name)
            .ok_or_else(|| format!("Unknown asset '{}'", asset_name))?;

        let initial_state = asset.manifest.initial_state.clone();
        let bounds = match self.scope {
            Some(scope) => scope,
            None => Bounds::window(self.viewport_w, self.viewport_h),
        };
        let id = self.next_id;
        self.next_id += 1;

        let parallax = options.parallax.unwrap_or(1.0);
        // Parallax shrinks the art, so it shrinks the footprint the floor and
        // the boundary modes measure against too.
        let frame_h = asset.frame_h as f32 * self.sprite_scale_y * parallax;
        let floor_y = self.ground_y.map(|surface| surface - frame_h);

        let seed = EntitySeed {
            initial_state: initial_state.clone(),
            x: options.x.unwrap_or(bounds.left + bounds.width / 2.0),
            y: options
                .y
                .or(anchored_y(options.anchor, floor_y, bounds))
                .unwrap_or(bounds.top + bounds.height / 2.0),
            flip_x: options.flip_x.unwrap_or(false),
            // A spawned `z` is the draw order as well as the depth, so it wins
            // over whatever the manifest declared.
            z_index: options
                .z
                .map(|z| z.round() as i32)
                .or(asset.manifest.z_index)
                .unwrap_or(0),
            z: options.z.unwrap_or(0.0),
            parallax,
        };

        let mut entity = Entity::new(id, asset_name.to_string(), seed);
        if let Some(floor_y) = floor_y {
            entity.ground_y = floor_y;
        }

        // Apply initial physics targets if defined
        if let Some(state_def) = asset.manifest.states.get(&initial_state) {
            entity.target_vx = state_def.physics.target_vx * entity.heading_x;
            entity.target_vy = state_def.physics.target_vy;
            entity.vx = entity.target_vx;
            entity.vy = entity.target_vy;
            entity.is_locked = state_def.is_locked;
            if let Some(gy) = state_def.physics.ground_y {
                // A manifest floor is a position, and manifest positions are in
                // terminal cells -- `spawn` is handed cells its caller already
                // converted. Copying the raw number in put the same manifest's
                // floor `cell_h` times further down here than in the terminal.
                entity.ground_y = gy * self.cell_h;
            }
        }

        // Desynchronise from anything already alive. Without this, two cats
        // spawned together share a frame index, a frame timer and a path phase
        // for the rest of their lives.
        let frame_count = asset
            .manifest
            .states
            .get(&initial_state)
            .map(|s| s.animation.frames.len())
            .unwrap_or(1)
            .max(1);
        entity.frame_idx = (self.rng.next_u64() as usize) % frame_count;
        entity.frame_timer = self.rng.next_f32() * 0.1;
        entity.path_phase = self.rng.next_f32() * std::f32::consts::TAU;

        self.entities.push(entity);
        Ok(id)
    }
}
