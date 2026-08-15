use crate::asset::AssetManager;
use crate::ipc::EntitySummary;
use crate::manifest::{AssetManifest, WrapMode};

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
    pub fn new(id: usize, asset_name: String, initial_state: String, x: f32, y: f32, flip_x: bool, z_index: i32) -> Self {
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
}

pub struct World {
    pub entities: Vec<Entity>,
    pub asset_manager: AssetManager,
    pub next_id: usize,
    pub viewport_w: f32,
    pub viewport_h: f32,
    pub cell_w: f32,
    pub cell_h: f32,
}

impl World {
    pub fn new(viewport_w: f32, viewport_h: f32) -> Self {
        Self {
            entities: Vec::new(),
            asset_manager: AssetManager::new(),
            next_id: 1,

            viewport_w,
            viewport_h,
            cell_w: 10.0,
            cell_h: 20.0,
        }
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
            let _ = self.asset_manager.register_manifest(manifest);
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

        let mut entity = Entity::new(id, asset_name.to_string(), initial_state.clone(), spawn_x, spawn_y, flip_x, z_index);

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
            Err(format!("Action '{}' not found or matched no active entities", action_name))
        } else {
            Ok(triggered)
        }
    }

    pub fn handle_editor_event(&mut self, event_name: &str) {
        for entity in &mut self.entities {
            if entity.is_locked {
                continue;
            }
            if let Some(asset) = self.asset_manager.get(&entity.asset_name) {
                if let Some(state_def) = asset.manifest.states.get(&entity.current_state) {
                    if let Some(next_state) = state_def.transitions.on_event.get(event_name) {
                        entity.set_state(next_state.clone());
                    }
                }
            }
        }
    }

    pub fn update(&mut self, dt: f32) {
        let viewport_w = self.viewport_w;
        let viewport_h = self.viewport_h;

        for entity in &mut self.entities {
            if !entity.is_active {
                continue;
            }

            entity.state_time += dt;

            // 1. Advance action timer and handle return state
            if let (Some(ref mut timer), Some(duration)) = (&mut entity.action_timer, entity.action_duration) {
                *timer += dt;
                if *timer >= duration {
                    entity.action_timer = None;
                    entity.action_duration = None;
                    entity.is_locked = false;
                    let next_state = entity.return_state.take().unwrap_or_else(|| "idle".to_string());
                    entity.set_state(next_state);
                }
            }

            let asset = match self.asset_manager.get(&entity.asset_name) {
                Some(a) => a,
                None => continue,
            };

            let frame_w = asset.frame_w as f32;
            let frame_h = asset.frame_h as f32;

            let state_def = asset.manifest.states.get(&entity.current_state);

            if let Some(state_def) = state_def {
                // 2. Check timeout transitions
                if let (Some(timeout_ms), Some(ref next_state)) = (state_def.transitions.timeout_ms, &state_def.transitions.on_timeout) {
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

                // 4. Physics update with frame-rate independent delta time (dt)
                let phys = &state_def.physics;
                let speed_x = phys.target_vx.abs();
                entity.target_vx = speed_x * entity.heading_x;
                entity.target_vy = phys.target_vy;

                // Sync flip with heading direction
                entity.flip_x = entity.heading_x < 0.0;

                // Smooth exponential velocity lerping
                let lerp_factor = (1.0 - (-phys.friction * dt * 60.0).exp()).clamp(0.01, 1.0);
                entity.vx += (entity.target_vx - entity.vx) * lerp_factor;

                if phys.gravity > 0.0 {
                    entity.vy += phys.gravity * dt * 60.0;
                    entity.y += entity.vy * dt * 60.0;

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
                            let amp = phys.path_amplitude.unwrap_or(15.0);
                            let freq = phys.path_frequency.unwrap_or(2.0);
                            entity.path_phase += dt * freq;
                            entity.y = entity.base_y + entity.path_phase.sin() * amp;
                        } else {
                            entity.y += entity.vy * dt * 60.0;
                        }
                    } else {
                        entity.y += entity.vy * dt * 60.0;
                    }
                }

                entity.x += entity.vx * dt * 60.0;

                // 5. Boundary checking
                match phys.wrap_mode {
                    WrapMode::Wrap => {
                        if entity.vx > 0.0 && entity.x > viewport_w {
                            entity.x = -frame_w;
                        } else if entity.vx < 0.0 && entity.x < -frame_w {
                            entity.x = viewport_w;
                        }
                        if entity.vy > 0.0 && entity.y > viewport_h {
                            entity.y = -frame_h;
                        } else if entity.vy < 0.0 && entity.y < -frame_h {
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
                        if entity.x < -frame_w || entity.x > viewport_w || entity.y < -frame_h || entity.y > viewport_h {
                            entity.is_active = false;
                        }
                    }
                    WrapMode::None => {}
                }
            }
        }

        // Clean up inactive entities
        self.entities.retain(|e| e.is_active);
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
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_creation_and_state_change() {
        let mut ent = Entity::new(1, "cat".to_string(), "idle".to_string(), 10.0, 20.0, false, 0);
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
        let id1 = world.spawn("cat", None, Some(10.0), Some(20.0), None).unwrap();
        let id2 = world.spawn("crab", None, Some(50.0), Some(60.0), None).unwrap();
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(world.entities.len(), 2);

        assert!(world.despawn(id1));
        assert_eq!(world.entities.len(), 1);
        assert!(!world.despawn(999)); // non-existent
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

        // Editor typing event
        world.handle_editor_event("typing");
        assert_eq!(world.entities[0].current_state, "walk_fast"); // Cat
        assert_eq!(world.entities[1].current_state, "walk_fast"); // Crab

        // Editor scrolling event
        world.handle_editor_event("scrolling");
        assert_eq!(world.entities[0].current_state, "yawn"); // Cat yawn on scroll
        assert_eq!(world.entities[1].current_state, "clip_claws"); // Crab clip claws on scroll
    }

    #[test]
    fn test_timeout_transition() {
        let mut world = World::new(800.0, 600.0);
        world.spawn("cat", None, None, None, None).unwrap();
        assert_eq!(world.entities[0].current_state, "idle");

        // Cat has timeout_ms: 6000 -> "sleep"
        // Advance time by 7.0 seconds
        world.update(7.0);
        assert_eq!(world.entities[0].current_state, "sleep");
    }

    #[test]
    fn test_bounce_wrap_mode() {
        let mut world = World::new(200.0, 200.0);
        // Crab has wrap_mode: Bounce
        let id = world.spawn("crab", None, Some(190.0), Some(50.0), Some(false)).unwrap();
        world.trigger_action(Some(id), None, "walk").unwrap();
        assert_eq!(world.entities[0].current_state, "walk");

        // Move right into wall
        for _ in 0..10 {
            world.update(0.1);
        }

        // Entity should have bounced off right edge, reversed velocity and flipped
        assert!(world.entities[0].flip_x);
    }

    #[test]
    fn test_action_dispatch_errors() {
        let mut world = World::new(800.0, 600.0);
        world.spawn("cat", None, None, None, None).unwrap();

        // Invalid action name
        let err1 = world.trigger_action(None, Some("cat"), "nonexistent_action");
        assert!(err1.is_err());

        // Target mismatch
        let err2 = world.trigger_action(Some(999), None, "jump");
        assert!(err2.is_err());
    }

    #[test]
    fn test_get_summaries() {
        let mut world = World::new(800.0, 600.0);
        world.spawn("cat", None, Some(123.0), Some(456.0), None).unwrap();
        let summaries = world.get_summaries();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].asset_name, "cat");
        assert_eq!(summaries[0].x, 123.0);
        assert_eq!(summaries[0].y, 456.0);
    }

    #[test]
    fn test_gravity_jump_and_ground_collision() {
        let mut world = World::new(800.0, 600.0);
        let id = world.spawn("cat", None, Some(100.0), Some(200.0), None).unwrap();
        
        // Trigger jump action with impulse -4.0 and gravity 0.15
        world.trigger_action(Some(id), None, "jump").unwrap();
        assert_eq!(world.entities[0].current_state, "jump");
        assert!(world.entities[0].vy < 0.0); // moving up initially
        assert_eq!(world.entities[0].ground_y, 200.0);

        // Update several frames: entity should rise, reach apex, fall down, and clamp to ground_y
        for _ in 0..50 {
            world.update(0.05);
        }

        // Entity must not fall below ground level (200.0)
        assert_eq!(world.entities[0].y, 200.0);
        assert_eq!(world.entities[0].vy, 0.0);
    }

    #[test]
    fn test_sine_pathing_phase() {
        let mut world = World::new(800.0, 600.0);
        let _id = world.spawn("sun", None, Some(100.0), Some(200.0), None).unwrap();
        assert_eq!(world.entities[0].current_state, "shining");

        // Advance time and check smooth oscillation around base_y (200.0)
        world.update(0.5);
        let y1 = world.entities[0].y;
        assert!(world.entities[0].path_phase > 0.0);
        assert!(y1 != 200.0); // Oscillating
    }

    #[test]
    fn test_multi_entity_action_dispatch() {
        let mut world = World::new(800.0, 600.0);
        let _id1 = world.spawn("cat", None, Some(50.0), Some(200.0), None).unwrap();
        let _id2 = world.spawn("cat", None, Some(150.0), Some(200.0), None).unwrap();
        let _id3 = world.spawn("crab", None, Some(300.0), Some(200.0), None).unwrap();

        // Trigger jump for all cats
        let triggered = world.trigger_action(None, Some("cat"), "jump").unwrap();
        assert_eq!(triggered.len(), 2);
        assert_eq!(world.entities[0].current_state, "jump");
        assert_eq!(world.entities[1].current_state, "jump");
        assert_eq!(world.entities[2].current_state, "idle"); // Crab unchanged
    }
}

