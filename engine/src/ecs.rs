use crate::asset::AssetManager;
use crate::bounds::Bounds;
use crate::entity::{Entity, Rng};
use crate::entity_step::{self, StepContext};
use crate::ipc::EntitySummary;
use crate::journal::{Journal, WorldEvent};
use crate::manifest::AssetManifest;
use crate::obstacles::{self, Obstacle};
use crate::render::RenderSettings;
use crate::spawn::{Anchor, EntitySeed, SpawnOptions};

/// Default terminal cell size in physical pixels.
///
/// There is no portable way to ask a terminal for its cell size, so this is a
/// documented starting point that Neovim overrides via `UpdateGrid` once it has
/// measured or been configured with the real value. See `:help distract-overlay`.
pub const DEFAULT_CELL_W: f32 = 10.0;
pub const DEFAULT_CELL_H: f32 = 20.0;

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
    /// What the world did, for the Lua plugin pipeline. Idle until subscribed.
    pub journal: Journal,
    /// The rectangle entities may move in, when Neovim scoped it to less than
    /// the whole window. `None` is the whole window.
    pub scope: Option<Bounds>,
    /// Solid ground and hazards a Neovim plugin registered, in overlay pixels.
    /// Empty for every session with no obstacle provider, which is the default.
    pub obstacles: Vec<Obstacle>,
    pub asset_manager: AssetManager,
    pub next_id: usize,
    pub viewport_w: f32,
    pub viewport_h: f32,
    pub cell_w: f32,
    pub cell_h: f32,
    /// Upscale applied to sprite art when drawn, per axis.
    ///
    /// Sprites are authored at terminal-cell resolution: one sprite pixel is
    /// one cell wide and *half* a cell tall. The two axes therefore scale by
    /// different amounts — `cell_w` and `cell_h / 2` — and a single uniform
    /// factor is only correct on a cell that happens to be exactly 2:1. On a
    /// HiDPI 16x36 cell a uniform scale drew sprites 7.1 cells tall where the
    /// terminal backend drew 8.
    pub sprite_scale_x: f32,
    pub sprite_scale_y: f32,
    /// Where the user is working, in overlay pixels, if known.
    pub focus_x: Option<f32>,
    pub focus_y: Option<f32>,
    /// The floor, in overlay pixels: the surface an entity's feet rest on.
    ///
    /// Measured by Neovim, which is the only side that knows about `cmdheight`,
    /// the statusline and where the buffer text ends, and pushed over
    /// `UpdateGrid` when it moves. `None` until one arrives, which is the
    /// behaviour every entity had before floors existed: it stands where it
    /// spawned.
    pub ground_y: Option<f32>,
    /// How the renderer draws, pushed from Neovim. Held here rather than in the
    /// renderer because the terminal backends read the same block and because a
    /// world snapshot has to be able to say what mode it was drawn in.
    pub render: RenderSettings,
    rng: Rng,
}

/// How close two floors must be to count as the same one, in overlay pixels.
///
/// Sub-pixel: a floor that moved by less than this did not move.
const FLOOR_MATCH_EPSILON_PX: f32 = 0.001;

impl World {
    pub fn new(viewport_w: f32, viewport_h: f32) -> Self {
        Self {
            entities: Vec::new(),
            journal: Journal::default(),
            scope: None,
            obstacles: Vec::new(),
            asset_manager: AssetManager::new(),
            next_id: 1,
            viewport_w,
            viewport_h,
            cell_w: DEFAULT_CELL_W,
            cell_h: DEFAULT_CELL_H,
            sprite_scale_x: DEFAULT_CELL_W,
            sprite_scale_y: DEFAULT_CELL_H / 2.0,
            render: RenderSettings::default(),
            focus_x: None,
            focus_y: None,
            ground_y: None,
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
        self.sprite_scale_x = self.cell_w.clamp(1.0, 64.0);
        self.sprite_scale_y = (self.cell_h / 2.0).clamp(1.0, 64.0);

        self.viewport_w = (cols as f32 * self.cell_w).max(100.0).min(max_w);
        self.viewport_h = (rows as f32 * self.cell_h).max(100.0).min(max_h);
    }

    /// Moves the floor, re-seating whatever was standing on the old one.
    ///
    /// Mirrors `engine.set_ground_row`. Only entities whose floor *is* the
    /// previous world floor move: a manifest floor and the anchor a jump takes
    /// are their own, and a screen that changed shape has nothing to say about
    /// either. An entity already resting is carried down with the floor rather
    /// than left hanging until gravity notices.
    /// Where entities may be, in overlay pixels.
    pub fn bounds(&self) -> Bounds {
        match self.scope {
            Some(scope) => scope,
            None => Bounds::window(self.viewport_w, self.viewport_h),
        }
    }

    /// Replaces the registered obstacles.
    ///
    /// # Errors
    /// When more than `obstacles::MAX_OBSTACLES` arrive: the physics pass is per
    /// entity per obstacle per frame, so an unbounded list from a Tree-sitter
    /// query over a large file is a frame-budget hazard, not a feature.
    pub fn set_obstacles(&mut self, obstacles: Vec<Obstacle>) -> Result<(), String> {
        if obstacles.len() > obstacles::MAX_OBSTACLES {
            return Err(format!(
                "at most {} obstacles may be registered, got {}",
                obstacles::MAX_OBSTACLES,
                obstacles.len()
            ));
        }
        self.obstacles = obstacles;
        Ok(())
    }

    /// Restricts entities to a rectangle Neovim measured, or clears it.
    ///
    /// # Errors
    /// When the rectangle has no positive size, or does not intersect the
    /// window: either would leave every entity permanently out of bounds.
    pub fn set_scope(&mut self, scope: Option<Bounds>) -> Result<(), String> {
        match scope {
            None => {
                self.scope = None;
                Ok(())
            }
            Some(request) => {
                let resolved = Bounds::scoped(request, self.viewport_w, self.viewport_h)?;
                self.scope = Some(resolved);
                Ok(())
            }
        }
    }

    pub fn set_ground_y(&mut self, ground_y: f32) {
        let previous = self.ground_y.replace(ground_y);
        let Some(previous) = previous else {
            return;
        };
        if (previous - ground_y).abs() < FLOOR_MATCH_EPSILON_PX {
            return;
        }

        let scale_y = self.sprite_scale_y;
        let frame_heights: Vec<Option<f32>> = self
            .entities
            .iter()
            .map(|entity| {
                self.asset_manager
                    .get(&entity.asset_name)
                    .map(|asset| asset.frame_h as f32 * scale_y * entity.parallax)
            })
            .collect();

        for (entity, frame_h) in self.entities.iter_mut().zip(frame_heights) {
            let Some(frame_h) = frame_h else {
                continue;
            };
            let was = previous - frame_h;
            if (entity.ground_y - was).abs() > FLOOR_MATCH_EPSILON_PX {
                continue;
            }
            let is_resting = entity.y >= was - FLOOR_MATCH_EPSILON_PX;
            entity.ground_y = ground_y - frame_h;
            if is_resting {
                entity.y = entity.ground_y;
            }
        }
    }

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

    /// Puts an entity into a state a plugin asked for.
    ///
    /// The mirror of `engine.lua`'s `set_entity_state` reached through a world
    /// command, so a hook that requests a state gets the same result on the
    /// overlay as it does in the terminal. A state the manifest does not
    /// declare is refused rather than left to animate nothing.
    pub fn set_entity_state(&mut self, id: usize, state: &str) -> Result<(), String> {
        let asset_name = match self.entities.iter().find(|e| e.id == id) {
            Some(entity) => entity.asset_name.clone(),
            None => return Err(format!("Entity #{} not found", id)),
        };
        let declares_state = self
            .asset_manager
            .get(&asset_name)
            .map(|asset| asset.manifest.states.contains_key(state))
            .unwrap_or(false);
        if !declares_state {
            return Err(format!("'{}' declares no state '{}'", asset_name, state));
        }
        if let Some(entity) = self.entities.iter_mut().find(|e| e.id == id) {
            entity.set_state(state.to_string());
        }
        Ok(())
    }

    /// Adds to an entity's velocity, in sprite pixels per frame at 60 FPS.
    pub fn apply_impulse(&mut self, id: usize, vx: f32, vy: f32) -> Result<(), String> {
        match self.entities.iter_mut().find(|e| e.id == id) {
            Some(entity) => {
                entity.vx += vx;
                entity.vy += vy;
                Ok(())
            }
            None => Err(format!("Entity #{} not found", id)),
        }
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
        let bounds = self.bounds();
        let scale_x = self.sprite_scale_x;
        let scale_y = self.sprite_scale_y;
        let is_recording = self.journal.is_enabled();
        let mut collisions: Vec<WorldEvent> = Vec::new();
        // Taken out of `self` for the duration of the loop, which holds
        // `entities` mutably. Empty in every session without a provider, so the
        // move costs nothing there.
        let obstacles = std::mem::take(&mut self.obstacles);

        for entity in &mut self.entities {
            entity_step::advance(
                entity,
                &StepContext {
                    dt,
                    bounds,
                    scale_x,
                    scale_y,
                    assets: &self.asset_manager,
                    obstacles: &obstacles,
                    is_recording,
                },
                &mut collisions,
            );
        }

        self.obstacles = obstacles;
        self.journal.record_all(collisions);
        if is_recording {
            let states: Vec<(usize, String)> = self
                .entities
                .iter()
                .filter(|e| e.is_active)
                .map(|e| (e.id, e.current_state.clone()))
                .collect();
            self.journal
                .sync_states(states.iter().map(|(id, state)| (*id, state.as_str())));
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
            // `linear` is the exception: it overrides no position, so it
            // produces no picture that velocity alone would not.
            if state
                .physics
                .path_type
                .as_deref()
                .is_some_and(|p| p != "linear")
            {
                return false;
            }
            true
        })
    }
}
