use crate::asset::AssetManager;
use crate::bounds::Bounds;
use crate::entity::{Entity, Rng};
use crate::entity_step::{self, StepContext};
use crate::ipc::EntitySummary;
use crate::journal::{Journal, WorldEvent};
use crate::obstacles::{self, Obstacle};
use crate::render::RenderSettings;

/// Default terminal cell size in physical pixels.
///
/// There is no portable way to ask a terminal for its cell size, so this is a
/// documented starting point that Neovim overrides via `UpdateGrid` once it has
/// measured or been configured with the real value. See `:help distract-overlay`.
pub const DEFAULT_CELL_W: f32 = 10.0;
pub const DEFAULT_CELL_H: f32 = 20.0;

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
    /// Crate-visible so `world_spawn` can desynchronise a new entity; not part
    /// of the public surface.
    pub(crate) rng: Rng,
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
