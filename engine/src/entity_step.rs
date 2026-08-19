//! One entity's frame: timers, animation, physics, then boundaries.
//!
//! The Rust counterpart to `lua/distract/entity_step.lua`, and split from
//! `ecs.rs` for the same reason that one was split from `engine.lua`:
//! `World::update` was three hundred lines of per-entity work wrapped in thirty
//! lines of coordination, and the two read as one only because they shared a
//! scope.
//!
//! The five numbered steps run in order and each reads what the previous one
//! wrote, which is why this is one function rather than five. The physics-parity
//! fixtures pin that order across both engines.

use crate::asset::AssetManager;
use crate::bounds::Bounds;
use crate::entity::Entity;
use crate::frame_timing::frame_duration_seconds;
use crate::journal::{self, WorldEvent};
use crate::manifest;
use crate::manifest::WrapMode;
use crate::obstacles::{self, Footprint, Obstacle, PushDirection};
use crate::path::apply_path;

/// Everything one entity's step reads that is not the entity.
///
/// Assembled once per frame by `World::update`: the entity list is borrowed
/// mutably for the whole loop, so nothing here can be reached through `self`.
pub struct StepContext<'a> {
    pub dt: f32,
    pub bounds: Bounds,
    /// Terminal cell size, which is what converts a manifest's sprite pixels
    /// into the pixels the boundary modes measure against.
    pub scale_x: f32,
    pub scale_y: f32,
    pub assets: &'a AssetManager,
    pub obstacles: &'a [Obstacle],
    /// Whether the journal is recording, so collision events are only built when
    /// something will read them.
    pub is_recording: bool,
}

/// Advances one entity by `context.dt`, appending any collision it reports.
pub fn advance(entity: &mut Entity, context: &StepContext, collisions: &mut Vec<WorldEvent>) {
    if !entity.is_active {
        return;
    }

    entity.state_time += context.dt;

    // 1. Advance action timer and handle return state
    if let (Some(ref mut timer), Some(duration)) =
        (&mut entity.action_timer, entity.action_duration)
    {
        *timer += context.dt;
        if *timer >= duration {
            entity.action_timer = None;
            entity.action_duration = None;
            entity.is_locked = false;
            let next_state = entity
                .return_state
                .take()
                .unwrap_or_else(|| "idle".to_string());
            entity.set_state(next_state);
        }
    }

    let asset = match context.assets.get(&entity.asset_name) {
        Some(a) => a,
        None => return,
    };

    // Parallax shrinks the drawn art, so the footprint the boundary
    // modes measure against shrinks with it.
    let frame_w = asset.frame_w as f32 * context.scale_x * entity.parallax;
    let frame_h = asset.frame_h as f32 * context.scale_y * entity.parallax;

    let state_def = asset.manifest.states.get(&entity.current_state);

    if let Some(state_def) = state_def {
        // 2. Check timeout transitions
        if let (Some(timeout_ms), Some(ref next_state)) = (
            state_def.transitions.timeout_ms,
            &state_def.transitions.on_timeout,
        ) {
            if entity.state_time * 1000.0 >= timeout_ms as f32 {
                entity.set_state(next_state.clone());
            }
        }

        // 3. Advance animation frames
        let anim = &state_def.animation;
        let frame_count = anim.frames.len();
        if frame_count > 0 {
            let frame_duration = frame_duration_seconds(anim, entity.frame_idx, asset);
            entity.frame_timer += context.dt;

            if entity.frame_timer >= frame_duration {
                entity.frame_timer -= frame_duration;
                if entity.frame_idx + 1 < frame_count {
                    entity.frame_idx += 1;
                } else if anim.loop_anim {
                    entity.frame_idx = 0;
                } else {
                    entity.animation_finished = true;
                    if let Some(ref next_state) = state_def.transitions.on_finish {
                        entity.set_state(next_state.clone());
                    }
                }
            }
        }

        // 4. Physics, in the shared manifest unit.
        //
        // Manifest positions and velocities are in *sprite pixels* per
        // frame at 60 FPS, and one sprite pixel is one terminal cell
        // wide. Converting on integration is what makes one manifest
        // describe one behaviour on both backends; the two used to
        // apply unrelated ad-hoc factors and moved at different speeds.
        //
        // Parallax damps the displacement rather than the stored
        // velocity: damping the velocity every frame would decay it to
        // zero instead of moving a distant thing slower at a steady
        // speed.
        let step = context.dt * 60.0;
        let px = step * context.scale_x * entity.parallax;
        let py = step * context.scale_y * entity.parallax;
        let phys = &state_def.physics;
        let speed_x = phys.target_vx.abs();
        entity.target_vx = speed_x * entity.heading_x;
        entity.target_vy = phys.target_vy;

        // Sync flip with heading direction
        entity.flip_x = entity.heading_x < 0.0;

        // Smooth exponential velocity lerping
        let lerp_factor = (1.0 - (-phys.friction * step).exp()).clamp(0.01, 1.0);
        entity.vx += (entity.target_vx - entity.vx) * lerp_factor;
        // Constant acceleration, on top of the pull toward `target_vx`.
        // These were declared in the manifest schema and read by
        // nothing, so a manifest could set them and watch them do
        // nothing. `gravity` is `accel_y` under a name that also brings
        // a floor with it.
        entity.vx += phys.accel_x * step;

        if phys.gravity > 0.0 {
            // Read before the integration: an entity already resting on
            // the floor is re-accelerated by gravity and caught by the
            // clamp on every single tick, so "the clamp ran" is not a
            // landing. Crossing the floor from above is.
            let feet_before = entity.y + frame_h;
            entity.vy += phys.gravity * step;
            entity.y += entity.vy * py;

            // A registered platform is a floor the entity reaches
            // earlier, so the surface for this frame is whichever is
            // higher. With no context.obstacles this is the floor exactly, and
            // the arithmetic is the ground clamp it replaces.
            let floor_feet = entity.ground_y + frame_h;
            let surface = obstacles::crossed_platform(
                context.obstacles,
                Footprint {
                    left: entity.x,
                    top: entity.y,
                    width: frame_w,
                    height: frame_h,
                },
                feet_before,
            )
            .map_or(floor_feet, |platform_top| platform_top.min(floor_feet));
            let was_airborne = feet_before < surface;

            if entity.y + frame_h >= surface {
                entity.y = surface - frame_h;
                let landed = was_airborne && entity.vy > 0.0;
                entity.vy = 0.0;
                if landed && context.is_recording {
                    collisions.push(WorldEvent::collision(entity.id, journal::EDGE_BOTTOM));
                }
                if landed && phys.effective_locomotion() == manifest::BALLISTIC {
                    if let Some(ref land_state) = state_def.transitions.on_land {
                        // Landing ends the action that launched the
                        // entity. Leaving its timer running would drag
                        // the entity out of the state it just reached
                        // as soon as the clock caught up, so a jump
                        // that lands early would still be locked until
                        // its declared duration.
                        entity.action_timer = None;
                        entity.action_duration = None;
                        entity.return_state = None;
                        entity.is_locked = false;
                        entity.set_state(land_state.clone());
                    }
                }
            }
        } else {
            entity.vy += (entity.target_vy - entity.vy) * lerp_factor;
            entity.vy += phys.accel_y * step;
            entity.y += entity.vy * py;
        }

        entity.x += entity.vx * px;

        // A path is a positional *override*, applied after integration
        // so it replaces the velocity result on the axes it owns and
        // leaves the others alone. Gravity is excluded: a path that
        // writes y fights the floor, which is what the locomotion
        // classes exist to keep apart.
        if phys.gravity <= 0.0 {
            if let Some(ref path_type) = phys.path_type {
                apply_path(
                    entity,
                    path_type,
                    phys,
                    context.dt,
                    context.scale_x,
                    context.scale_y,
                );
            }
        }

        // A grounded state has no gravity to fall under, so which
        // surface it stands on is resolved rather than integrated. Only
        // reached while context.obstacles exist: without them the answer is the
        // floor the entity was already seated on.
        if !context.obstacles.is_empty()
            && phys.gravity <= 0.0
            && asset.manifest.locomotion_for(state_def) == manifest::GROUNDED
        {
            entity.y = obstacles::standing_surface(
                context.obstacles,
                Footprint {
                    left: entity.x,
                    top: entity.y,
                    width: frame_w,
                    height: frame_h,
                },
                entity.ground_y + frame_h,
            ) - frame_h;
        }

        if let Some(deflection) = obstacles::deflection(
            context.obstacles,
            Footprint {
                left: entity.x,
                top: entity.y,
                width: frame_w,
                height: frame_h,
            },
            entity.heading_x,
        ) {
            entity.x = deflection.x;
            entity.heading_x = match deflection.direction {
                PushDirection::Left => -1.0,
                PushDirection::Right => 1.0,
            };
            entity.vx = entity.vx.abs() * entity.heading_x;
            entity.flip_x = entity.heading_x < 0.0;
            if context.is_recording {
                collisions.push(WorldEvent::collision(entity.id, journal::EDGE_OBSTACLE));
            }
        }

        // 5. Boundary checking
        match phys.wrap_mode {
            WrapMode::Wrap => {
                // Gated on position and heading, not on instantaneous
                // velocity: `vx` lerps toward its target, so a state
                // whose target is zero decays it through zero and an
                // entity that had already left the viewport would never
                // wrap back — it just sat off-screen forever.
                if entity.x > context.bounds.right() {
                    entity.x = context.bounds.left - frame_w;
                } else if entity.x < context.bounds.left - frame_w {
                    entity.x = context.bounds.right();
                }
                if entity.y > context.bounds.bottom() {
                    entity.y = context.bounds.top - frame_h;
                } else if entity.y < context.bounds.top - frame_h {
                    entity.y = context.bounds.bottom();
                }
            }
            WrapMode::Bounce => {
                if entity.x <= context.bounds.left {
                    entity.x = context.bounds.left;
                    entity.heading_x = 1.0;
                    entity.vx = entity.vx.abs().max(0.5);
                    entity.flip_x = false;
                    if context.is_recording {
                        collisions.push(WorldEvent::collision(entity.id, journal::EDGE_LEFT));
                    }
                    if let Some(ref edge_state) = state_def.transitions.on_edge_left {
                        entity.set_state(edge_state.clone());
                    }
                } else if entity.x + frame_w >= context.bounds.right() {
                    entity.x = (context.bounds.right() - frame_w).max(context.bounds.left);
                    entity.heading_x = -1.0;
                    entity.vx = -entity.vx.abs().max(0.5);
                    entity.flip_x = true;
                    if context.is_recording {
                        collisions.push(WorldEvent::collision(entity.id, journal::EDGE_RIGHT));
                    }
                    if let Some(ref edge_state) = state_def.transitions.on_edge_right {
                        entity.set_state(edge_state.clone());
                    }
                }

                if entity.vy != 0.0 {
                    if entity.y <= context.bounds.top {
                        entity.y = context.bounds.top;
                        entity.vy = entity.vy.abs();
                        if context.is_recording {
                            collisions.push(WorldEvent::collision(entity.id, journal::EDGE_TOP));
                        }
                    } else if entity.y + frame_h >= context.bounds.bottom() {
                        entity.y = (context.bounds.bottom() - frame_h).max(context.bounds.top);
                        entity.vy = -entity.vy.abs();
                        if context.is_recording {
                            collisions.push(WorldEvent::collision(entity.id, journal::EDGE_BOTTOM));
                        }
                    }
                }
            }
            WrapMode::Clamp => {
                entity.x = entity.x.clamp(
                    context.bounds.left,
                    (context.bounds.right() - frame_w).max(context.bounds.left),
                );
                entity.y = entity.y.clamp(
                    context.bounds.top,
                    (context.bounds.bottom() - frame_h).max(context.bounds.top),
                );
            }
            WrapMode::Despawn => {
                if entity.x < context.bounds.left - frame_w
                    || entity.x > context.bounds.right()
                    || entity.y < context.bounds.top - frame_h
                    || entity.y > context.bounds.bottom()
                {
                    entity.is_active = false;
                }
            }
            WrapMode::None => {}
        }
    }
}
