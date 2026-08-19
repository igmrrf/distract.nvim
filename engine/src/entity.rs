//! One simulated entity, and the noise that keeps two of them apart.
//!
//! Split from `ecs.rs`, which owned the entity, the world, the step and the state
//! machine at once. What an entity *is* -- its fields, its state and action
//! transitions, which way it faces -- is a smaller and far more stable question
//! than what the world does with it every frame.

use crate::spawn::EntitySeed;

/// Small deterministic PRNG, used only to desynchronise entities from each
/// other. Two entities of the same type spawned together are otherwise
/// perfectly in step forever, which reads as a chorus line rather than as two
/// animals.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // splitmix64 finalisation, so even adjacent seeds diverge immediately.
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform float in 0..1.
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub id: usize,
    pub asset_name: String,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub target_vx: f32,
    pub target_vy: f32,
    pub heading_x: f32,
    pub flip_x: bool,
    pub current_state: String,
    pub state_time: f32,
    pub frame_idx: usize,
    pub frame_timer: f32,
    pub animation_finished: bool,
    pub is_active: bool,
    /// Where a path primitive anchors its x axis: the spawn point, re-taken on
    /// every state change. `base_y` has always existed for `sine`; the paths
    /// that write x need the other half of the same idea.
    pub base_x: f32,
    pub base_y: f32,
    pub ground_y: f32,
    pub path_phase: f32,
    pub action_timer: Option<f32>,
    pub action_duration: Option<f32>,
    pub return_state: Option<String>,
    pub is_locked: bool,
    pub z_index: i32,
    /// Depth, dimensionless. Draw order comes from `z_index`, which a spawned
    /// `z` overrides; this is what parallax is computed from.
    pub z: f32,
    /// How far depth damps this entity's motion and shrinks its art. Exactly 1
    /// unless a configuration asked for parallax.
    pub parallax: f32,
}

impl Entity {
    pub fn new(id: usize, asset_name: String, seed: EntitySeed) -> Self {
        let heading_x = if seed.flip_x { -1.0 } else { 1.0 };
        Self {
            id,
            asset_name,
            x: seed.x,
            y: seed.y,
            vx: 0.0,
            vy: 0.0,
            target_vx: 0.0,
            target_vy: 0.0,
            heading_x,
            flip_x: seed.flip_x,
            current_state: seed.initial_state,
            state_time: 0.0,
            frame_idx: 0,
            frame_timer: 0.0,
            animation_finished: false,
            is_active: true,
            base_x: seed.x,
            base_y: seed.y,
            ground_y: seed.y,
            path_phase: 0.0,
            action_timer: None,
            action_duration: None,
            return_state: None,
            is_locked: false,
            z_index: seed.z_index,
            z: seed.z,
            parallax: seed.parallax,
        }
    }

    pub fn set_state(&mut self, new_state: String) {
        if self.current_state != new_state {
            self.current_state = new_state;
            self.state_time = 0.0;
            self.frame_idx = 0;
            self.frame_timer = 0.0;
            self.animation_finished = false;
            self.base_x = self.x;
            self.base_y = self.y;
            self.path_phase = 0.0;
        }
    }

    pub fn set_action(
        &mut self,
        target_state: String,
        duration_opt: Option<f32>,
        return_opt: Option<String>,
        is_locked: bool,
    ) {
        self.set_state(target_state);
        self.action_timer = Some(0.0);
        self.action_duration = duration_opt;
        self.return_state = return_opt;
        self.is_locked = is_locked;
    }

    /// Turns the entity to face a point, if it is not already facing it.
    ///
    /// Called on state changes rather than every tick, so bounce and edge
    /// handling still own the heading once the entity is moving.
    pub fn face_toward(&mut self, target_x: f32) {
        let dx = target_x - self.x;
        if dx.abs() < 1.0 {
            return;
        }
        self.heading_x = if dx > 0.0 { 1.0 } else { -1.0 };
        self.flip_x = self.heading_x < 0.0;
    }
}
