use crate::asset::AssetManager;
use crate::ipc::EntitySummary;
use crate::manifest::{AssetManifest, WrapMode};

/// Default terminal cell size in physical pixels.
///
/// There is no portable way to ask a terminal for its cell size, so this is a
/// documented starting point that Neovim overrides via `UpdateGrid` once it has
/// measured or been configured with the real value. See `:help distract-overlay`.
pub const DEFAULT_CELL_W: f32 = 10.0;
pub const DEFAULT_CELL_H: f32 = 20.0;

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
    pub base_y: f32,
    pub ground_y: f32,
    pub path_phase: f32,
    pub action_timer: Option<f32>,
    pub action_duration: Option<f32>,
    pub return_state: Option<String>,
    pub is_locked: bool,
    pub z_index: i32,
}

impl Entity {
    pub fn new(
        id: usize,
        asset_name: String,
        initial_state: String,
        x: f32,
        y: f32,
        flip_x: bool,
        z_index: i32,
    ) -> Self {
        let heading_x = if flip_x { -1.0 } else { 1.0 };
        Self {
            id,
            asset_name,
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            target_vx: 0.0,
            target_vy: 0.0,
            heading_x,
            flip_x,
            current_state: initial_state,
            state_time: 0.0,
            frame_idx: 0,
            frame_timer: 0.0,
            animation_finished: false,
            is_active: true,
            base_y: y,
            ground_y: y,
            path_phase: 0.0,
            action_timer: None,
            action_duration: None,
            return_state: None,
            is_locked: false,
            z_index,
        }
    }

    pub fn set_state(&mut self, new_state: String) {
        if self.current_state != new_state {
            self.current_state = new_state;
            self.state_time = 0.0;
            self.frame_idx = 0;
            self.frame_timer = 0.0;
            self.animation_finished = false;
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

/// What the editor was doing when an event was sent.
///
/// The cursor position is the most informative signal the editor has, and it
/// was previously accepted over IPC and discarded. Entities use it to orient
/// toward where the user is actually working.
#[derive(Debug, Clone, Copy, Default)]
pub struct EventContext {
    /// Cursor column, in terminal cells.
    pub cursor_col: Option<f32>,
    /// Cursor row, in terminal cells.
    pub cursor_row: Option<f32>,
}

impl EventContext {
    pub fn from_json(value: Option<&serde_json::Value>) -> Self {
        let Some(obj) = value.and_then(|v| v.as_object()) else {
            return Self::default();
        };
        let num = |k: &str| obj.get(k).and_then(|v| v.as_f64()).map(|v| v as f32);
        Self {
            cursor_col: num("cursor_col"),
            cursor_row: num("cursor_row"),
        }
    }
}

pub struct World {
    pub entities: Vec<Entity>,
    pub asset_manager: AssetManager,
    pub next_id: usize,
    pub viewport_w: f32,
    pub viewport_h: f32,
    pub cell_w: f32,
    pub cell_h: f32,
    /// Integer upscale applied to sprite art when drawn.
    ///
    /// Sprites are authored at terminal-cell resolution: one sprite pixel is
    /// one cell wide and half a cell tall. Scaling by `cell_w` therefore makes
    /// an overlay sprite the same apparent size as the same sprite drawn in the
    /// terminal.
    pub sprite_scale: u32,
    /// Where the user is working, in overlay pixels, if known.
    pub focus_x: Option<f32>,
    pub focus_y: Option<f32>,
    rng: Rng,
}

impl World {
    pub fn new(viewport_w: f32, viewport_h: f32) -> Self {
        Self {
            entities: Vec::new(),
            asset_manager: AssetManager::new(),
            next_id: 1,
            viewport_w,
            viewport_h,
            cell_w: DEFAULT_CELL_W,
            cell_h: DEFAULT_CELL_H,
            sprite_scale: DEFAULT_CELL_W as u32,
            focus_x: None,
            focus_y: None,
            // Fixed seed: reproducible across runs, which keeps the tests
            // deterministic while still desynchronising entities from
            // each other.
            rng: Rng::new(0x1234_5678),
        }
    }

    /// Applies a new terminal grid measurement.
    ///
    /// `cell_w`/`cell_h` are the terminal's cell size in physical pixels. The
    /// sprite scale follows the cell width so overlay art matches what the
    /// in-terminal backend would draw.
    pub fn set_grid(
        &mut self,
        cols: u32,
        rows: u32,
        cell_w: Option<f32>,
        cell_h: Option<f32>,
        max_w: f32,
        max_h: f32,
    ) {
        if let Some(cw) = cell_w.filter(|v| *v > 0.0) {
            self.cell_w = cw;
        }
        if let Some(ch) = cell_h.filter(|v| *v > 0.0) {
            self.cell_h = ch;
        }
        self.sprite_scale = (self.cell_w.round() as u32).clamp(1, 32);

        self.viewport_w = (cols as f32 * self.cell_w).max(100.0).min(max_w);
        self.viewport_h = (rows as f32 * self.cell_h).max(100.0).min(max_h);
    }

    pub fn spawn(
        &mut self,
        asset_name: &str,
        manifest_opt: Option<AssetManifest>,
        x_opt: Option<f32>,
        y_opt: Option<f32>,
        flip_x_opt: Option<bool>,
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
        let z_index = asset.manifest.z_index.unwrap_or(0);
        let id = self.next_id;
        self.next_id += 1;

        let spawn_x = x_opt.unwrap_or(self.viewport_w / 2.0);
        let spawn_y = y_opt.unwrap_or(self.viewport_h / 2.0);
        let flip_x = flip_x_opt.unwrap_or(false);

        let mut entity = Entity::new(
            id,
            asset_name.to_string(),
            initial_state.clone(),
            spawn_x,
            spawn_y,
            flip_x,
            z_index,
        );

        // Apply initial physics targets if defined
        if let Some(state_def) = asset.manifest.states.get(&initial_state) {
            entity.target_vx = state_def.physics.target_vx * entity.heading_x;
            entity.target_vy = state_def.physics.target_vy;
            entity.vx = entity.target_vx;
            entity.vy = entity.target_vy;
            entity.is_locked = state_def.is_locked;
            if let Some(gy) = state_def.physics.ground_y {
                entity.ground_y = gy;
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

    pub fn despawn(&mut self, id: usize) -> bool {
        let initial_len = self.entities.len();
        self.entities.retain(|e| e.id != id);
        self.entities.len() < initial_len
    }

    pub fn clear_all(&mut self) {
        self.entities.clear();
    }

    pub fn trigger_action(
        &mut self,
        id_opt: Option<usize>,
        asset_name_opt: Option<&str>,
        action_name: &str,
    ) -> Result<Vec<(usize, String, String)>, String> {
        let mut triggered = Vec::new();

        for entity in &mut self.entities {
            if let Some(target_id) = id_opt {
                if entity.id != target_id {
                    continue;
                }
            } else if let Some(target_asset) = asset_name_opt {
                if entity.asset_name != target_asset {
                    continue;
                }
            }

            if let Some(asset) = self.asset_manager.get(&entity.asset_name) {
                if let Some(action_def) = asset.manifest.custom_actions.get(action_name) {
                    let target_state = action_def.target_state.clone();
                    let duration_s = action_def.duration_ms.map(|ms| ms as f32 / 1000.0);
                    let return_state = action_def.return_state.clone();
                    let is_locked = action_def.is_locked.unwrap_or(true);

                    // Save takeoff position as ground elevation
                    entity.ground_y = entity.y;
                    entity.set_action(target_state.clone(), duration_s, return_state, is_locked);

                    // Apply jump impulse if configured
                    if let Some(state_def) = asset.manifest.states.get(&target_state) {
                        if let Some(impulse) = state_def.physics.jump_impulse_y {
                            entity.vy = impulse;
                        }
                    }

                    triggered.push((entity.id, entity.asset_name.clone(), target_state));
                }
            }
        }

        if triggered.is_empty() {
            Err(format!(
                "Action '{}' not found or matched no active entities",
                action_name
            ))
        } else {
            Ok(triggered)
        }
    }

    pub fn handle_editor_event(&mut self, event_name: &str, context: EventContext) {
        if let Some(col) = context.cursor_col {
            self.focus_x = Some(col * self.cell_w);
        }
        if let Some(row) = context.cursor_row {
            self.focus_y = Some(row * self.cell_h);
        }
        let focus_x = self.focus_x;

        for entity in &mut self.entities {
            if entity.is_locked {
                continue;
            }
            if let Some(asset) = self.asset_manager.get(&entity.asset_name) {
                if let Some(state_def) = asset.manifest.states.get(&entity.current_state) {
                    if let Some(next_state) = state_def.transitions.on_event.get(event_name) {
                        let changed = entity.current_state != *next_state;
                        entity.set_state(next_state.clone());

                        // Orient toward the cursor when picking up a new
                        // behaviour, so the entity looks like it noticed.
                        if changed {
                            if let Some(fx) = focus_x {
                                let moves = asset
                                    .manifest
                                    .states
                                    .get(next_state)
                                    .map(|s| s.physics.target_vx.abs() > 0.0)
                                    .unwrap_or(false);
                                if moves {
                                    entity.face_toward(fx);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Advances the world by `dt` seconds.
    ///
    /// Returns the ids of entities removed during this step. `WrapMode::Despawn`
    /// used to drop entities silently, so Neovim's idea of what was alive
    /// diverged from the engine's and `:DistractStatus` disagreed with reality.
    pub fn update(&mut self, dt: f32) -> Vec<usize> {
        let viewport_w = self.viewport_w;
        let viewport_h = self.viewport_h;
        let sprite_scale = self.sprite_scale;

        for entity in &mut self.entities {
            if !entity.is_active {
                continue;
            }

            entity.state_time += dt;

            // 1. Advance action timer and handle return state
            if let (Some(ref mut timer), Some(duration)) =
                (&mut entity.action_timer, entity.action_duration)
            {
                *timer += dt;
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

            let asset = match self.asset_manager.get(&entity.asset_name) {
                Some(a) => a,
                None => continue,
            };

            let frame_w = (asset.frame_w * sprite_scale) as f32;
            let frame_h = (asset.frame_h * sprite_scale) as f32;

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
                    let frame_duration = if anim.fps > 0.0 { 1.0 / anim.fps } else { 0.1 };
                    entity.frame_timer += dt;

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
                let step = dt * 60.0;
                let px = step * sprite_scale as f32;
                let phys = &state_def.physics;
                let speed_x = phys.target_vx.abs();
                entity.target_vx = speed_x * entity.heading_x;
                entity.target_vy = phys.target_vy;

                // Sync flip with heading direction
                entity.flip_x = entity.heading_x < 0.0;

                // Smooth exponential velocity lerping
                let lerp_factor = (1.0 - (-phys.friction * step).exp()).clamp(0.01, 1.0);
                entity.vx += (entity.target_vx - entity.vx) * lerp_factor;

                if phys.gravity > 0.0 {
                    entity.vy += phys.gravity * step;
                    entity.y += entity.vy * px;

                    // Ground collision clamping
                    if entity.y >= entity.ground_y {
                        entity.y = entity.ground_y;
                        entity.vy = 0.0;
                    }
                } else {
                    entity.vy += (entity.target_vy - entity.vy) * lerp_factor;
                    // Pathing calculations
                    if let Some(ref path_type) = phys.path_type {
                        if path_type == "sine" {
                            let amp = phys.path_amplitude.unwrap_or(4.0) * sprite_scale as f32;
                            let freq = phys.path_frequency.unwrap_or(2.0);
                            entity.path_phase += dt * freq;
                            entity.y = entity.base_y + entity.path_phase.sin() * amp;
                        } else {
                            entity.y += entity.vy * px;
                        }
                    } else {
                        entity.y += entity.vy * px;
                    }
                }

                entity.x += entity.vx * px;

                // 5. Boundary checking
                match phys.wrap_mode {
                    WrapMode::Wrap => {
                        // Gated on position and heading, not on instantaneous
                        // velocity: `vx` lerps toward its target, so a state
                        // whose target is zero decays it through zero and an
                        // entity that had already left the viewport would never
                        // wrap back — it just sat off-screen forever.
                        if entity.x > viewport_w {
                            entity.x = -frame_w;
                        } else if entity.x < -frame_w {
                            entity.x = viewport_w;
                        }
                        if entity.y > viewport_h {
                            entity.y = -frame_h;
                        } else if entity.y < -frame_h {
                            entity.y = viewport_h;
                        }
                    }
                    WrapMode::Bounce => {
                        if entity.x <= 0.0 {
                            entity.x = 0.0;
                            entity.heading_x = 1.0;
                            entity.vx = entity.vx.abs().max(0.5);
                            entity.flip_x = false;
                            if let Some(ref edge_state) = state_def.transitions.on_edge_left {
                                entity.set_state(edge_state.clone());
                            }
                        } else if entity.x + frame_w >= viewport_w {
                            entity.x = (viewport_w - frame_w).max(0.0);
                            entity.heading_x = -1.0;
                            entity.vx = -entity.vx.abs().max(0.5);
                            entity.flip_x = true;
                            if let Some(ref edge_state) = state_def.transitions.on_edge_right {
                                entity.set_state(edge_state.clone());
                            }
                        }

                        if entity.vy != 0.0 {
                            if entity.y <= 0.0 {
                                entity.y = 0.0;
                                entity.vy = entity.vy.abs();
                            } else if entity.y + frame_h >= viewport_h {
                                entity.y = (viewport_h - frame_h).max(0.0);
                                entity.vy = -entity.vy.abs();
                            }
                        }
                    }
                    WrapMode::Clamp => {
                        entity.x = entity.x.clamp(0.0, (viewport_w - frame_w).max(0.0));
                        entity.y = entity.y.clamp(0.0, (viewport_h - frame_h).max(0.0));
                    }
                    WrapMode::Despawn => {
                        if entity.x < -frame_w
                            || entity.x > viewport_w
                            || entity.y < -frame_h
                            || entity.y > viewport_h
                        {
                            entity.is_active = false;
                        }
                    }
                    WrapMode::None => {}
                }
            }
        }

        // Clean up inactive entities, reporting what went.
        let removed: Vec<usize> = self
            .entities
            .iter()
            .filter(|e| !e.is_active)
            .map(|e| e.id)
            .collect();
        if !removed.is_empty() {
            self.entities.retain(|e| e.is_active);
        }
        removed
    }

    pub fn get_summaries(&self) -> Vec<EntitySummary> {
        self.entities
            .iter()
            .map(|e| EntitySummary {
                id: e.id,
                asset_name: e.asset_name.clone(),
                state: e.current_state.clone(),
                x: e.x,
                y: e.y,
                vx: e.vx,
                vy: e.vy,
            })
            .collect()
    }

    /// Whether anything in the world can still change without further input.
    ///
    /// Used to skip redrawing entirely when nothing is moving or animating: an
    /// overlay of sleeping cats should cost approximately nothing.
    pub fn is_quiescent(&self) -> bool {
        self.entities.iter().all(|e| {
            if !e.is_active {
                return false;
            }
            if e.action_timer.is_some() {
                return false;
            }
            if e.vx.abs() > 0.001 || e.vy.abs() > 0.001 {
                return false;
            }
            let Some(asset) = self.asset_manager.get(&e.asset_name) else {
                return true;
            };
            let Some(state) = asset.manifest.states.get(&e.current_state) else {
                return true;
            };
            // A multi-frame animation, a pending timeout or a path all keep
            // producing new pictures with no further input.
            if state.animation.frames.len() > 1 {
                return false;
            }
            if state.transitions.timeout_ms.is_some() {
                return false;
            }
            if state.physics.path_type.is_some() {
                return false;
            }
            true
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(event: &str) -> EventContext {
        let _ = event;
        EventContext::default()
    }

    #[test]
    fn test_entity_creation_and_state_change() {
        let mut ent = Entity::new(
            1,
            "cat".to_string(),
            "idle".to_string(),
            10.0,
            20.0,
            false,
            0,
        );
        assert_eq!(ent.id, 1);
        assert_eq!(ent.current_state, "idle");
        assert_eq!(ent.state_time, 0.0);

        ent.state_time = 5.0;
        ent.frame_idx = 2;
        ent.set_state("jump".to_string());
        assert_eq!(ent.current_state, "jump");
        assert_eq!(ent.state_time, 0.0);
        assert_eq!(ent.frame_idx, 0);
    }

    #[test]
    fn test_world_spawn_and_despawn() {
        let mut world = World::new(800.0, 600.0);
        let id1 = world
            .spawn("cat", None, Some(10.0), Some(20.0), None)
            .unwrap();
        let id2 = world
            .spawn("crab", None, Some(50.0), Some(60.0), None)
            .unwrap();
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(world.entities.len(), 2);

        assert!(world.despawn(id1));
        assert_eq!(world.entities.len(), 1);
        assert!(!world.despawn(999));
    }

    #[test]
    fn test_world_clear_all() {
        let mut world = World::new(800.0, 600.0);
        world.spawn("cat", None, None, None, None).unwrap();
        world.spawn("crab", None, None, None, None).unwrap();
        world.spawn("sun", None, None, None, None).unwrap();
        assert_eq!(world.entities.len(), 3);

        world.clear_all();
        assert_eq!(world.entities.len(), 0);
    }

    #[test]
    fn test_editor_event_transitions() {
        let mut world = World::new(800.0, 600.0);
        world.spawn("cat", None, None, None, None).unwrap();
        world.spawn("crab", None, None, None, None).unwrap();

        world.handle_editor_event("typing", plain("typing"));
        assert_eq!(world.entities[0].current_state, "walk_fast");
        assert_eq!(world.entities[1].current_state, "walk_fast");

        world.handle_editor_event("scrolling", plain("scrolling"));
        assert_eq!(world.entities[0].current_state, "yawn");
        assert_eq!(world.entities[1].current_state, "clip_claws");
    }

    #[test]
    fn test_timeout_transition() {
        let mut world = World::new(800.0, 600.0);
        world.spawn("cat", None, None, None, None).unwrap();
        assert_eq!(world.entities[0].current_state, "idle");

        world.update(7.0);
        assert_eq!(world.entities[0].current_state, "sleep");
    }

    #[test]
    fn test_bounce_wrap_mode() {
        let mut world = World::new(200.0, 200.0);
        world.sprite_scale = 1;
        let id = world
            .spawn("crab", None, Some(190.0), Some(50.0), Some(false))
            .unwrap();
        world.trigger_action(Some(id), None, "walk").unwrap();
        assert_eq!(world.entities[0].current_state, "walk");

        for _ in 0..10 {
            world.update(0.1);
        }

        assert!(world.entities[0].flip_x);
    }

    #[test]
    fn test_action_dispatch_errors() {
        let mut world = World::new(800.0, 600.0);
        world.spawn("cat", None, None, None, None).unwrap();

        assert!(world
            .trigger_action(None, Some("cat"), "nonexistent_action")
            .is_err());
        assert!(world.trigger_action(Some(999), None, "jump").is_err());
    }

    #[test]
    fn test_get_summaries() {
        let mut world = World::new(800.0, 600.0);
        world
            .spawn("cat", None, Some(123.0), Some(456.0), None)
            .unwrap();
        let summaries = world.get_summaries();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].asset_name, "cat");
        assert_eq!(summaries[0].x, 123.0);
        assert_eq!(summaries[0].y, 456.0);
    }

    #[test]
    fn test_gravity_jump_and_ground_collision() {
        let mut world = World::new(800.0, 600.0);
        let id = world
            .spawn("cat", None, Some(100.0), Some(200.0), None)
            .unwrap();

        world.trigger_action(Some(id), None, "jump").unwrap();
        assert_eq!(world.entities[0].current_state, "jump");
        assert!(world.entities[0].vy < 0.0);
        assert_eq!(world.entities[0].ground_y, 200.0);

        for _ in 0..50 {
            world.update(0.05);
        }

        assert_eq!(world.entities[0].y, 200.0);
        assert_eq!(world.entities[0].vy, 0.0);
    }

    #[test]
    fn test_sine_pathing_phase() {
        let mut world = World::new(800.0, 600.0);
        world
            .spawn("sun", None, Some(100.0), Some(200.0), None)
            .unwrap();
        assert_eq!(world.entities[0].current_state, "shining");

        let before = world.entities[0].path_phase;
        world.update(0.5);
        assert!(world.entities[0].path_phase > before);
    }

    #[test]
    fn test_multi_entity_action_dispatch() {
        let mut world = World::new(800.0, 600.0);
        world
            .spawn("cat", None, Some(50.0), Some(200.0), None)
            .unwrap();
        world
            .spawn("cat", None, Some(150.0), Some(200.0), None)
            .unwrap();
        world
            .spawn("crab", None, Some(300.0), Some(200.0), None)
            .unwrap();

        let triggered = world.trigger_action(None, Some("cat"), "jump").unwrap();
        assert_eq!(triggered.len(), 2);
        assert_eq!(world.entities[0].current_state, "jump");
        assert_eq!(world.entities[1].current_state, "jump");
        assert_eq!(world.entities[2].current_state, "idle");
    }

    // ---- regressions from the review ----

    #[test]
    fn update_reports_entities_it_despawns() {
        let mut world = World::new(200.0, 200.0);
        world.sprite_scale = 1;
        let mut manifest = AssetManifest::default_cat();
        manifest.name = "runner".to_string();
        if let Some(state) = manifest.states.get_mut("idle") {
            state.physics.wrap_mode = WrapMode::Despawn;
            state.physics.target_vx = 40.0;
            state.transitions.timeout_ms = None;
            state.transitions.on_timeout = None;
        }
        let id = world
            .spawn("runner", Some(manifest), Some(190.0), Some(50.0), None)
            .unwrap();

        let mut reported = Vec::new();
        for _ in 0..40 {
            reported.extend(world.update(0.1));
        }

        assert_eq!(reported, vec![id], "despawn must be reported to Neovim");
        assert!(world.entities.is_empty());
    }

    #[test]
    fn wrap_recovers_an_entity_whose_velocity_decayed_off_screen() {
        // The old gate was `vx > 0 && x > viewport_w`. Park an entity past the
        // right edge with no velocity: it must still come back.
        let mut world = World::new(200.0, 200.0);
        world.sprite_scale = 1;
        world
            .spawn("cat", None, Some(10.0), Some(10.0), None)
            .unwrap();
        world.entities[0].current_state = "walk".to_string();
        world.entities[0].x = 500.0;
        world.entities[0].vx = 0.0;
        world.entities[0].target_vx = 0.0;
        world.entities[0].heading_x = 0.0;

        world.update(0.016);
        assert!(
            world.entities[0].x < 200.0,
            "entity stayed off-screen at x={}",
            world.entities[0].x
        );
    }

    #[test]
    fn spawn_desynchronises_identical_entities() {
        let mut world = World::new(800.0, 600.0);
        for _ in 0..8 {
            world
                .spawn("cat", None, Some(10.0), Some(10.0), None)
                .unwrap();
        }
        let phases: std::collections::HashSet<u32> = world
            .entities
            .iter()
            .map(|e| (e.path_phase * 1000.0) as u32)
            .collect();
        assert!(phases.len() > 1, "all entities share one path phase");

        let timers: std::collections::HashSet<u32> = world
            .entities
            .iter()
            .map(|e| (e.frame_timer * 100_000.0) as u32)
            .collect();
        assert!(timers.len() > 1, "all entities share one frame timer");
    }

    #[test]
    fn entities_turn_toward_the_cursor_when_they_react() {
        let mut world = World::new(800.0, 600.0);
        world.cell_w = 10.0;
        world
            .spawn("cat", None, Some(400.0), Some(300.0), Some(false))
            .unwrap();

        // Cursor far to the left: a cat that starts walking should face it.
        world.handle_editor_event(
            "moving",
            EventContext {
                cursor_col: Some(2.0),
                cursor_row: Some(1.0),
            },
        );
        assert_eq!(world.entities[0].current_state, "walk");
        assert_eq!(world.entities[0].heading_x, -1.0);
        assert!(world.entities[0].flip_x);
    }

    #[test]
    fn a_still_state_with_no_animation_is_quiescent() {
        let mut world = World::new(800.0, 600.0);
        assert!(world.is_quiescent(), "an empty world has nothing to draw");

        let mut manifest = AssetManifest::default_cat();
        manifest.name = "statue".to_string();
        manifest.initial_state = "idle".to_string();
        if let Some(state) = manifest.states.get_mut("idle") {
            state.animation.frames = vec![0];
            state.physics.target_vx = 0.0;
            state.physics.path_type = None;
            state.transitions.timeout_ms = None;
        }
        world
            .spawn("statue", Some(manifest), Some(10.0), Some(10.0), None)
            .unwrap();
        world.entities[0].vx = 0.0;
        world.entities[0].vy = 0.0;
        assert!(world.is_quiescent());
    }

    #[test]
    fn an_animating_entity_is_not_quiescent() {
        let mut world = World::new(800.0, 600.0);
        world
            .spawn("cat", None, Some(10.0), Some(10.0), None)
            .unwrap();
        assert!(!world.is_quiescent());
    }

    #[test]
    fn spawn_surfaces_a_broken_manifest_instead_of_degrading() {
        let mut world = World::new(800.0, 600.0);
        let mut manifest = AssetManifest::default_cat();
        manifest.name = "broken".to_string();
        manifest.asset_type = "sprite".to_string();
        manifest.spritesheet.path = Some("/nowhere/at/all.png".to_string());

        let err = world
            .spawn("broken", Some(manifest), None, None, None)
            .unwrap_err();
        assert!(err.contains("not found"), "unexpected message: {}", err);
    }

    #[test]
    fn sprite_scale_follows_the_measured_cell_width() {
        let mut world = World::new(1920.0, 1080.0);
        world.set_grid(80, 24, Some(16.0), Some(36.0), 1920.0, 1080.0);
        assert_eq!(world.cell_w, 16.0);
        assert_eq!(world.cell_h, 36.0);
        assert_eq!(world.sprite_scale, 16);
        assert_eq!(world.viewport_w, 1280.0);
        assert_eq!(world.viewport_h, 864.0);
    }

    #[test]
    fn set_grid_ignores_nonsense_cell_sizes() {
        let mut world = World::new(1920.0, 1080.0);
        world.set_grid(80, 24, Some(0.0), Some(-4.0), 1920.0, 1080.0);
        assert_eq!(world.cell_w, DEFAULT_CELL_W);
        assert_eq!(world.cell_h, DEFAULT_CELL_H);
    }

    #[test]
    fn rng_desynchronises_adjacent_seeds() {
        let a = Rng::new(1).next_u64();
        let b = Rng::new(2).next_u64();
        assert_ne!(a, b);
    }
}
