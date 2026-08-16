use crate::asset::AssetManager;
use crate::ipc::EntitySummary;
use crate::manifest;
use crate::manifest::{AssetManifest, PhysicsConfig, WrapMode};
use crate::spawn::{Anchor, EntitySeed, SpawnOptions};

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

/// A cubic Bezier evaluated at `t`, in sprite pixels relative to the anchor.
fn cubic_bezier(points: &[[f32; 2]], t: f32) -> (f32, f32) {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (
        a * points[0][0] + b * points[1][0] + c * points[2][0] + d * points[3][0],
        a * points[0][1] + b * points[1][1] + c * points[2][1] + d * points[3][1],
    )
}

/// Applies a path primitive's positional override in place.
///
/// The phase advances at a base rate and per-axis frequency multiplies *inside*
/// the trigonometric term. Folding frequency into the advance instead would
/// double-apply it on `lissajous`, where the two axes must run at different
/// rates against one shared phase. With `freq` defaulting to 1 and the
/// `path_frequency -> freq_y` alias, `sine` evaluates exactly what it always
/// did.
fn apply_path(
    entity: &mut Entity,
    path_type: &str,
    phys: &PhysicsConfig,
    dt: f32,
    scale_x: f32,
    scale_y: f32,
) {
    // `linear` is pure velocity integration, which already happened.
    if path_type == "linear" {
        return;
    }

    let p = phys.resolved_path();
    entity.path_phase += dt * p.freq;
    let phase = entity.path_phase;

    match path_type {
        "sine" => {
            entity.y = entity.base_y + (p.freq_y * phase).sin() * p.amp_y * scale_y;
        }
        "orbital" => {
            entity.x = entity.base_x + (p.freq_x * phase).cos() * p.amp_x * scale_x;
            entity.y = entity.base_y + (p.freq_y * phase).sin() * p.amp_y * scale_y;
        }
        "lissajous" => {
            entity.x = entity.base_x + (p.freq_x * phase + p.phase_delta).sin() * p.amp_x * scale_x;
            entity.y = entity.base_y + (p.freq_y * phase).sin() * p.amp_y * scale_y;
        }
        "bezier" => {
            let Some(points) = phys.path_params.as_ref().and_then(|pp| pp.points.as_ref()) else {
                return;
            };
            if points.len() < 4 {
                return;
            }
            // Wrapped rather than clamped, so the curve loops instead of
            // running off its last control point and staying there.
            let (ox, oy) = cubic_bezier(points, phase.rem_euclid(1.0));
            entity.x = entity.base_x + ox * scale_x;
            entity.y = entity.base_y + oy * scale_y;
        }
        // An unrecognised path is velocity integration, same as `linear`.
        _ => {}
    }
}

/// Where an anchored spawn starts vertically, in overlay pixels.
///
/// `None` when the anchor asks for nothing in particular, or asks for a floor
/// Neovim has not measured yet, leaving the caller's own default to apply.
/// What one animation frame is shown for when nothing declares a rate.
const FALLBACK_FRAME_SECONDS: f32 = 0.1;

const MS_PER_SECOND: f32 = 1000.0;

/// How long the entity's current animation frame is shown for, in seconds.
///
/// A manifest `fps` wins. Imported art whose state declares none is timed by
/// the delays stored in the file, which is the only rate an animation authored
/// elsewhere carries; `lua/distract/engine.lua` applies the same precedence, so
/// a GIF asset runs at one speed on both backends.
fn frame_duration_seconds(
    anim: &manifest::AnimationConfig,
    frame_idx: usize,
    asset: &crate::asset::LoadedAsset,
) -> f32 {
    if anim.fps > 0.0 {
        return 1.0 / anim.fps;
    }

    let delay_ms = anim
        .frames
        .get(frame_idx)
        .and_then(|sheet_index| asset.frame_delays_ms.get(*sheet_index))
        .copied()
        .unwrap_or(0);

    if delay_ms > 0 {
        delay_ms as f32 / MS_PER_SECOND
    } else {
        FALLBACK_FRAME_SECONDS
    }
}

fn anchored_y(anchor: Option<Anchor>, floor_y: Option<f32>) -> Option<f32> {
    match anchor {
        Some(Anchor::Bottom) => floor_y,
        Some(Anchor::Top) => Some(0.0),
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
            asset_manager: AssetManager::new(),
            next_id: 1,
            viewport_w,
            viewport_h,
            cell_w: DEFAULT_CELL_W,
            cell_h: DEFAULT_CELL_H,
            sprite_scale_x: DEFAULT_CELL_W,
            sprite_scale_y: DEFAULT_CELL_H / 2.0,
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
        let id = self.next_id;
        self.next_id += 1;

        let parallax = options.parallax.unwrap_or(1.0);
        // Parallax shrinks the art, so it shrinks the footprint the floor and
        // the boundary modes measure against too.
        let frame_h = asset.frame_h as f32 * self.sprite_scale_y * parallax;
        let floor_y = self.ground_y.map(|surface| surface - frame_h);

        let seed = EntitySeed {
            initial_state: initial_state.clone(),
            x: options.x.unwrap_or(self.viewport_w / 2.0),
            y: options
                .y
                .or(anchored_y(options.anchor, floor_y))
                .unwrap_or(self.viewport_h / 2.0),
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
        let scale_x = self.sprite_scale_x;
        let scale_y = self.sprite_scale_y;

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

            // Parallax shrinks the drawn art, so the footprint the boundary
            // modes measure against shrinks with it.
            let frame_w = asset.frame_w as f32 * scale_x * entity.parallax;
            let frame_h = asset.frame_h as f32 * scale_y * entity.parallax;

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
                //
                // Parallax damps the displacement rather than the stored
                // velocity: damping the velocity every frame would decay it to
                // zero instead of moving a distant thing slower at a steady
                // speed.
                let step = dt * 60.0;
                let px = step * scale_x * entity.parallax;
                let py = step * scale_y * entity.parallax;
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
                    let was_airborne = entity.y < entity.ground_y;
                    entity.vy += phys.gravity * step;
                    entity.y += entity.vy * py;

                    // Ground collision clamping
                    if entity.y >= entity.ground_y {
                        entity.y = entity.ground_y;
                        let landed = was_airborne && entity.vy > 0.0;
                        entity.vy = 0.0;
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
                        apply_path(entity, path_type, phys, dt, scale_x, scale_y);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{PathParams, PhysicsConfig, StateDefinition, TransitionConfig};

    fn plain(event: &str) -> EventContext {
        let _ = event;
        EventContext::default()
    }

    fn timed_asset(delays_ms: Vec<u32>) -> crate::asset::LoadedAsset {
        let manifest = AssetManifest::default_cat();
        let mut asset = crate::asset::AssetManager::load_asset(manifest, 0)
            .expect("the built-in cat must load for the timing tests");
        asset.frame_delays_ms = delays_ms;
        asset
    }

    fn animation(fps: f32, frames: Vec<usize>) -> manifest::AnimationConfig {
        manifest::AnimationConfig {
            frames,
            fps,
            loop_anim: true,
            flip_x: false,
        }
    }

    #[test]
    fn a_declared_fps_outranks_the_files_own_timing() {
        let asset = timed_asset(vec![500, 500]);
        let anim = animation(20.0, vec![0, 1]);

        assert_eq!(frame_duration_seconds(&anim, 0, &asset), 0.05);
    }

    #[test]
    fn imported_art_without_an_fps_runs_at_the_files_delay() {
        let asset = timed_asset(vec![200, 80]);
        let anim = animation(0.0, vec![0, 1]);

        assert_eq!(frame_duration_seconds(&anim, 0, &asset), 0.2);
        assert_eq!(frame_duration_seconds(&anim, 1, &asset), 0.08);
    }

    #[test]
    fn art_with_neither_an_fps_nor_a_delay_falls_back() {
        let asset = timed_asset(Vec::new());
        let anim = animation(0.0, vec![0]);

        assert_eq!(
            frame_duration_seconds(&anim, 0, &asset),
            FALLBACK_FRAME_SECONDS
        );
    }

    #[test]
    fn test_entity_creation_and_state_change() {
        let mut ent = Entity::new(
            1,
            "cat".to_string(),
            EntitySeed {
                initial_state: "idle".to_string(),
                x: 10.0,
                y: 20.0,
                flip_x: false,
                z_index: 0,
                z: 0.0,
                parallax: 1.0,
            },
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
            .spawn("cat", None, SpawnOptions::at(10.0, 20.0))
            .unwrap();
        let id2 = world
            .spawn("crab", None, SpawnOptions::at(50.0, 60.0))
            .unwrap();
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(world.entities.len(), 2);

        assert!(world.despawn(id1));
        assert_eq!(world.entities.len(), 1);
        assert!(!world.despawn(999));
    }

    /// The floor an entity of this asset would stand on, in overlay pixels.
    fn resting_y(world: &World, asset_name: &str, ground_y: f32) -> f32 {
        let asset = world
            .asset_manager
            .get(asset_name)
            .expect("built-in asset is registered");
        ground_y - asset.frame_h as f32 * world.sprite_scale_y
    }

    #[test]
    fn bottom_anchored_spawn_stands_on_the_pushed_floor() {
        let mut world = World::new(800.0, 600.0);
        world.set_ground_y(400.0);
        world
            .spawn(
                "cat",
                None,
                SpawnOptions {
                    anchor: Some(Anchor::Bottom),
                    ..SpawnOptions::default()
                },
            )
            .unwrap();

        let expected = resting_y(&world, "cat", 400.0);
        assert_eq!(world.entities[0].y, expected);
        assert_eq!(world.entities[0].ground_y, expected);
    }

    #[test]
    fn top_anchored_spawn_starts_at_the_viewport_top() {
        let mut world = World::new(800.0, 600.0);
        world.set_ground_y(400.0);
        world
            .spawn(
                "cat",
                None,
                SpawnOptions {
                    anchor: Some(Anchor::Top),
                    ..SpawnOptions::default()
                },
            )
            .unwrap();

        assert_eq!(world.entities[0].y, 0.0);
        // The anchor says where it starts, not what it falls to.
        assert_eq!(
            world.entities[0].ground_y,
            resting_y(&world, "cat", 400.0),
            "a top-anchored entity still owns the floor it will land on"
        );
    }

    #[test]
    fn an_explicit_position_wins_over_the_anchor() {
        let mut world = World::new(800.0, 600.0);
        world.set_ground_y(400.0);
        world
            .spawn(
                "cat",
                None,
                SpawnOptions {
                    y: Some(42.0),
                    anchor: Some(Anchor::Bottom),
                    ..SpawnOptions::default()
                },
            )
            .unwrap();

        assert_eq!(world.entities[0].y, 42.0);
    }

    #[test]
    fn spawning_without_a_floor_leaves_the_entity_standing_where_it_spawned() {
        let mut world = World::new(800.0, 600.0);
        world
            .spawn("cat", None, SpawnOptions::at(10.0, 20.0))
            .unwrap();

        assert_eq!(
            world.entities[0].ground_y, 20.0,
            "with no floor measured, an entity stands where it was put"
        );
    }

    #[test]
    fn moving_the_floor_carries_a_resting_entity_with_it() {
        let mut world = World::new(800.0, 600.0);
        world.set_ground_y(400.0);
        world
            .spawn(
                "cat",
                None,
                SpawnOptions {
                    anchor: Some(Anchor::Bottom),
                    ..SpawnOptions::default()
                },
            )
            .unwrap();

        world.set_ground_y(300.0);

        let expected = resting_y(&world, "cat", 300.0);
        assert_eq!(world.entities[0].ground_y, expected);
        assert_eq!(
            world.entities[0].y, expected,
            "an entity already on the floor moves with it rather than hanging"
        );
    }

    #[test]
    fn moving_the_floor_leaves_a_manifest_floor_alone() {
        let mut manifest = AssetManifest::default_cat();
        manifest.name = "floored".to_string();
        manifest
            .states
            .get_mut("idle")
            .expect("cat has idle")
            .physics
            .ground_y = Some(5.0);

        let mut world = World::new(800.0, 600.0);
        world.set_ground_y(400.0);
        world
            .spawn("floored", Some(manifest), SpawnOptions::at(10.0, 20.0))
            .unwrap();
        let declared = world.entities[0].ground_y;

        world.set_ground_y(300.0);

        assert_eq!(
            world.entities[0].ground_y, declared,
            "a manifest declares its own floor; the screen has nothing to say about it"
        );
    }

    #[test]
    fn a_spawned_z_overrides_the_manifests_draw_order() {
        let mut world = World::new(800.0, 600.0);
        world
            .spawn(
                "sun",
                None,
                SpawnOptions {
                    z: Some(3.0),
                    ..SpawnOptions::default()
                },
            )
            .unwrap();

        assert_eq!(world.entities[0].z_index, 3);
        assert_eq!(world.entities[0].z, 3.0);
    }

    #[test]
    fn parallax_damps_how_far_an_entity_travels() {
        let mut near = World::new(800.0, 600.0);
        near.spawn("cat", None, SpawnOptions::at(0.0, 0.0)).unwrap();
        near.entities[0].vx = 2.0;

        let mut far = World::new(800.0, 600.0);
        far.spawn("cat", None, SpawnOptions::at(0.0, 0.0)).unwrap();
        far.entities[0].vx = 2.0;
        far.entities[0].parallax = 0.5;

        near.update(1.0 / 60.0);
        far.update(1.0 / 60.0);

        assert!(
            (far.entities[0].x - near.entities[0].x / 2.0).abs() < 1e-4,
            "half the parallax should cover half the ground: near {}, far {}",
            near.entities[0].x,
            far.entities[0].x
        );
    }

    /// Landing has to end the whole action, not only the state.
    ///
    /// A golden trajectory cannot reach this: the parity fixtures describe
    /// physics, and nothing in them triggers an action. The Lua engine carries
    /// the same assertion by hand, which is the mitigation the harness's own
    /// blind spot note asks for.
    #[test]
    fn landing_cancels_the_action_that_launched_the_jump() {
        let mut world = World::new(800.0, 600.0);
        world
            .spawn("cat", None, SpawnOptions::at(100.0, 200.0))
            .unwrap();
        world
            .trigger_action(Some(1), None, "jump")
            .expect("the cat declares a jump");

        assert!(
            world.entities[0].action_timer.is_some(),
            "the jump is pending"
        );

        for _ in 0..240 {
            world.update(1.0 / 60.0);
            if world.entities[0].current_state != "jump" {
                break;
            }
        }

        assert_eq!(
            world.entities[0].current_state, "idle",
            "the cat lands in idle"
        );
        assert!(
            world.entities[0].action_timer.is_none(),
            "a landing that leaves the timer running drags the cat back later"
        );
        assert!(
            !world.entities[0].is_locked,
            "a landed cat responds to the editor again"
        );
    }

    #[test]
    fn test_world_clear_all() {
        let mut world = World::new(800.0, 600.0);
        world.spawn("cat", None, SpawnOptions::default()).unwrap();
        world.spawn("crab", None, SpawnOptions::default()).unwrap();
        world.spawn("sun", None, SpawnOptions::default()).unwrap();
        assert_eq!(world.entities.len(), 3);

        world.clear_all();
        assert_eq!(world.entities.len(), 0);
    }

    #[test]
    fn test_editor_event_transitions() {
        let mut world = World::new(800.0, 600.0);
        world.spawn("cat", None, SpawnOptions::default()).unwrap();
        world.spawn("crab", None, SpawnOptions::default()).unwrap();

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
        world.spawn("cat", None, SpawnOptions::default()).unwrap();
        assert_eq!(world.entities[0].current_state, "idle");

        world.update(7.0);
        assert_eq!(world.entities[0].current_state, "sleep");
    }

    #[test]
    fn test_bounce_wrap_mode() {
        let mut world = World::new(200.0, 200.0);
        world.sprite_scale_x = 1.0;
        world.sprite_scale_y = 1.0;
        let id = world
            .spawn("crab", None, SpawnOptions::at(190.0, 50.0))
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
        world.spawn("cat", None, SpawnOptions::default()).unwrap();

        assert!(
            world
                .trigger_action(None, Some("cat"), "nonexistent_action")
                .is_err()
        );
        assert!(world.trigger_action(Some(999), None, "jump").is_err());
    }

    #[test]
    fn test_get_summaries() {
        let mut world = World::new(800.0, 600.0);
        world
            .spawn("cat", None, SpawnOptions::at(123.0, 456.0))
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
            .spawn("cat", None, SpawnOptions::at(100.0, 200.0))
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
            .spawn("sun", None, SpawnOptions::at(100.0, 200.0))
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
            .spawn("cat", None, SpawnOptions::at(50.0, 200.0))
            .unwrap();
        world
            .spawn("cat", None, SpawnOptions::at(150.0, 200.0))
            .unwrap();
        world
            .spawn("crab", None, SpawnOptions::at(300.0, 200.0))
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
        world.sprite_scale_x = 1.0;
        world.sprite_scale_y = 1.0;
        let mut manifest = AssetManifest::default_cat();
        manifest.name = "runner".to_string();
        if let Some(state) = manifest.states.get_mut("idle") {
            state.physics.wrap_mode = WrapMode::Despawn;
            state.physics.target_vx = 40.0;
            state.transitions.timeout_ms = None;
            state.transitions.on_timeout = None;
        }
        let id = world
            .spawn("runner", Some(manifest), SpawnOptions::at(190.0, 50.0))
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
        world.sprite_scale_x = 1.0;
        world.sprite_scale_y = 1.0;
        world
            .spawn("cat", None, SpawnOptions::at(10.0, 10.0))
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
                .spawn("cat", None, SpawnOptions::at(10.0, 10.0))
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
            .spawn("cat", None, SpawnOptions::at(400.0, 300.0))
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
            .spawn("statue", Some(manifest), SpawnOptions::at(10.0, 10.0))
            .unwrap();
        world.entities[0].vx = 0.0;
        world.entities[0].vy = 0.0;
        assert!(world.is_quiescent());
    }

    #[test]
    fn an_animating_entity_is_not_quiescent() {
        let mut world = World::new(800.0, 600.0);
        world
            .spawn("cat", None, SpawnOptions::at(10.0, 10.0))
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
            .spawn("broken", Some(manifest), SpawnOptions::default())
            .unwrap_err();
        assert!(err.contains("not found"), "unexpected message: {}", err);
    }

    #[test]
    fn sprite_scale_follows_the_measured_cell_width() {
        let mut world = World::new(1920.0, 1080.0);
        world.set_grid(80, 24, Some(16.0), Some(36.0), 1920.0, 1080.0);
        assert_eq!(world.cell_w, 16.0);
        assert_eq!(world.cell_h, 36.0);
        assert_eq!(world.sprite_scale_x, 16.0);
        assert_eq!(world.sprite_scale_y, 18.0);
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
    fn sprite_scale_uses_a_separate_factor_per_axis() {
        // A sprite pixel is one cell wide and half a cell tall. On a 16x36
        // HiDPI cell a uniform scale drew a 16px-tall sprite 7.1 cells tall
        // where the terminal backend drew 8.
        let mut world = World::new(1920.0, 1080.0);
        world.set_grid(80, 24, Some(16.0), Some(36.0), 1920.0, 1080.0);

        let cat = world.asset_manager.get("cat").unwrap();
        let drawn_h = cat.frame_h as f32 * world.sprite_scale_y;
        assert_eq!(
            drawn_h / world.cell_h,
            cat.frame_h as f32 / 2.0,
            "an overlay sprite must occupy the same number of cells as the terminal one"
        );
    }

    #[test]
    fn accel_x_is_integrated_rather_than_ignored() {
        // `accel_x`/`accel_y` were in the manifest schema and read by nothing.
        let mut world = World::new(2000.0, 2000.0);
        world.sprite_scale_x = 1.0;
        world.sprite_scale_y = 1.0;

        let mut manifest = AssetManifest::default_cat();
        manifest.name = "thruster".to_string();
        if let Some(state) = manifest.states.get_mut("idle") {
            state.physics.target_vx = 0.0;
            state.physics.accel_x = 0.5;
            state.physics.wrap_mode = WrapMode::None;
            state.transitions.timeout_ms = None;
            state.transitions.on_timeout = None;
        }
        world
            .spawn("thruster", Some(manifest), SpawnOptions::at(0.0, 0.0))
            .unwrap();

        for _ in 0..30 {
            world.update(1.0 / 60.0);
        }
        assert!(
            world.entities[0].vx > 0.1,
            "constant acceleration must build velocity, got vx={}",
            world.entities[0].vx
        );
        assert!(world.entities[0].x > 0.0, "and must move the entity");
    }

    #[test]
    fn accel_y_moves_an_entity_with_no_gravity_and_no_floor() {
        let mut world = World::new(2000.0, 2000.0);
        world.sprite_scale_x = 1.0;
        world.sprite_scale_y = 1.0;

        let mut manifest = AssetManifest::default_cat();
        manifest.name = "drifter".to_string();
        if let Some(state) = manifest.states.get_mut("idle") {
            state.physics.gravity = 0.0;
            state.physics.accel_y = -0.4;
            state.physics.path_type = None;
            state.physics.wrap_mode = WrapMode::None;
            state.transitions.timeout_ms = None;
            state.transitions.on_timeout = None;
        }
        world
            .spawn("drifter", Some(manifest), SpawnOptions::at(50.0, 500.0))
            .unwrap();

        for _ in 0..30 {
            world.update(1.0 / 60.0);
        }
        assert!(
            world.entities[0].y < 500.0,
            "accel_y must lift an entity that has no gravity, got y={}",
            world.entities[0].y
        );
    }

    #[test]
    fn rng_desynchronises_adjacent_seeds() {
        let a = Rng::new(1).next_u64();
        let b = Rng::new(2).next_u64();
        assert_ne!(a, b);
    }

    /// One entity whose only state runs `physics`, at one pixel per cell.
    ///
    /// The scale is 1:1 so the assertions below can be written in manifest
    /// units and read as the arithmetic they are.
    fn path_world(physics: PhysicsConfig) -> World {
        let mut world = World::new(800.0, 600.0);
        world.sprite_scale_x = 1.0;
        world.sprite_scale_y = 1.0;

        let mut manifest = AssetManifest::default_cat();
        manifest.name = "pathprobe".to_string();
        manifest.initial_state = "idle".to_string();
        // Inherited from the cat, which walks. A probe that orbits does not, and
        // the capability gate is right to say so.
        manifest.locomotion = Some(manifest::OMNIDIRECTIONAL.to_string());
        manifest.capabilities = Default::default();
        if let Some(state) = manifest.states.get_mut("idle") {
            state.animation.frames = vec![0];
            state.physics = physics;
            state.transitions = Default::default();
        }

        world
            .spawn("pathprobe", Some(manifest), SpawnOptions::at(100.0, 200.0))
            .expect("path probe spawns");
        // Spawn desynchronises entities with a random phase, which is right for
        // two suns on screen and fatal for an analytic assertion.
        world.entities[0].path_phase = 0.0;
        world
    }

    // `freq = 0` pins the phase where the test put it, so each assertion below
    // is the path equation evaluated by hand rather than wherever the
    // integrator happened to arrive.

    #[test]
    fn an_orbital_path_drives_the_x_axis_too() {
        let mut world = path_world(PhysicsConfig {
            path_type: Some("orbital".to_string()),
            path_params: Some(PathParams {
                freq: Some(0.0),
                amp_x: Some(12.0),
                amp_y: Some(5.0),
                ..Default::default()
            }),
            ..Default::default()
        });
        world.update(0.1);

        let e = &world.entities[0];
        assert!(
            (e.x - 112.0).abs() < 1e-4,
            "orbital must move x: cos(0) * 12 from base_x 100, got {}",
            e.x
        );
        assert!(
            (e.y - 200.0).abs() < 1e-4,
            "orbital y at phase 0 sits on base_y, got {}",
            e.y
        );
    }

    #[test]
    fn a_lissajous_path_offsets_x_by_its_phase_delta() {
        let mut world = path_world(PhysicsConfig {
            path_type: Some("lissajous".to_string()),
            path_params: Some(PathParams {
                freq: Some(0.0),
                amp_x: Some(10.0),
                amp_y: Some(4.0),
                phase_delta: Some(std::f32::consts::FRAC_PI_2),
                ..Default::default()
            }),
            ..Default::default()
        });
        world.update(0.1);

        let e = &world.entities[0];
        assert!(
            (e.x - 110.0).abs() < 1e-4,
            "sin(0 + pi/2) * 10 from base_x 100, got {}",
            e.x
        );
        assert!(
            (e.y - 200.0).abs() < 1e-4,
            "phase_delta is an x-axis offset only, got y {}",
            e.y
        );
    }

    #[test]
    fn a_bezier_path_starts_on_its_first_control_point() {
        let mut world = path_world(PhysicsConfig {
            path_type: Some("bezier".to_string()),
            path_params: Some(PathParams {
                freq: Some(0.0),
                points: Some(vec![[10.0, 20.0], [30.0, 0.0], [50.0, 40.0], [70.0, 5.0]]),
                ..Default::default()
            }),
            ..Default::default()
        });
        world.update(0.1);

        let e = &world.entities[0];
        assert!(
            (e.x - 110.0).abs() < 1e-4,
            "a cubic at t=0 is its first control point, got x {}",
            e.x
        );
        assert!(
            (e.y - 220.0).abs() < 1e-4,
            "control points are relative to the spawn position, got y {}",
            e.y
        );
    }

    #[test]
    fn the_legacy_sine_fields_describe_the_same_curve_as_path_params() {
        let mut legacy = path_world(PhysicsConfig {
            path_type: Some("sine".to_string()),
            path_amplitude: Some(15.0),
            path_frequency: Some(2.0),
            ..Default::default()
        });
        let mut modern = path_world(PhysicsConfig {
            path_type: Some("sine".to_string()),
            path_params: Some(PathParams {
                amp_y: Some(15.0),
                freq_y: Some(2.0),
                ..Default::default()
            }),
            ..Default::default()
        });

        for _ in 0..20 {
            legacy.update(0.05);
            modern.update(0.05);
        }

        assert!(
            (legacy.entities[0].y - 200.0).abs() > 1e-3,
            "the fixture has to actually be moving for this to mean anything"
        );
        assert!(
            (legacy.entities[0].y - modern.entities[0].y).abs() < 1e-5,
            "path_amplitude/path_frequency must alias amp_y/freq_y exactly, \
             got {} vs {}",
            legacy.entities[0].y,
            modern.entities[0].y
        );
    }

    /// One entity under `physics` and `transitions`, at one pixel per cell.
    fn locomotion_world(physics: PhysicsConfig, transitions: TransitionConfig) -> World {
        let mut world = World::new(800.0, 600.0);
        world.sprite_scale_x = 1.0;
        world.sprite_scale_y = 1.0;
        // One pixel per cell, so the manifest's floor and the entity's position
        // are the same number and the assertions stay readable.
        world.cell_w = 1.0;
        world.cell_h = 1.0;

        let mut manifest = AssetManifest::default_cat();
        manifest.name = "jumper".to_string();
        manifest.initial_state = "flying".to_string();
        manifest.states.clear();
        manifest.states.insert(
            "flying".to_string(),
            StateDefinition {
                physics,
                transitions,
                ..Default::default()
            },
        );
        manifest
            .states
            .insert("landed".to_string(), StateDefinition::default());

        world
            .spawn("jumper", Some(manifest), SpawnOptions::at(100.0, 200.0))
            .expect("locomotion probe spawns");
        world
    }

    /// Physics that falls onto a floor 20 cells below the spawn point.
    fn falling(locomotion: Option<&str>) -> PhysicsConfig {
        PhysicsConfig {
            gravity: 0.6,
            ground_y: Some(220.0),
            wrap_mode: WrapMode::None,
            locomotion: locomotion.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn spawning_a_manifest_that_breaks_its_own_capabilities_is_refused() {
        // Checked where the manifest arrives, not per frame: a manifest that
        // cannot work is worth one message when it lands, not thirty a second.
        let mut world = World::new(800.0, 600.0);
        let mut manifest = AssetManifest::default_cat();
        manifest.name = "impossible".to_string();
        manifest.initial_state = "orbit".to_string();
        manifest.locomotion = Some(manifest::GROUNDED.to_string());
        manifest.states.clear();
        manifest.states.insert(
            "orbit".to_string(),
            StateDefinition {
                physics: PhysicsConfig {
                    path_type: Some("orbital".to_string()),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        let error = world
            .spawn("impossible", Some(manifest), SpawnOptions::default())
            .expect_err("a grounded orbit cannot be drawn, so it must not spawn");
        assert!(
            error.contains("orbit"),
            "the refusal must name the offending state, got: {error}"
        );
        assert!(
            world.entities.is_empty(),
            "a refused spawn must leave no entity behind"
        );
    }

    #[test]
    fn a_manifest_floor_is_read_in_cells_like_every_other_position() {
        // `physics.ground_y` is a position, and manifest positions are in
        // terminal cells -- `spawn` is handed cells converted by its caller.
        // Copying the raw number into `Entity::ground_y`, which is in pixels,
        // put the same manifest's floor `cell_h` times further down on the
        // overlay than in the terminal. No built-in sets the field, so nothing
        // had exercised it.
        let mut world = World::new(800.0, 600.0);
        world.cell_w = 10.0;
        world.cell_h = 20.0;

        let mut manifest = AssetManifest::default_cat();
        manifest.name = "floored".to_string();
        manifest.initial_state = "idle".to_string();
        manifest.states.clear();
        manifest.states.insert(
            "idle".to_string(),
            StateDefinition {
                physics: PhysicsConfig {
                    ground_y: Some(15.0),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        world
            .spawn("floored", Some(manifest), SpawnOptions::at(0.0, 0.0))
            .expect("floored probe spawns");

        assert_eq!(
            world.entities[0].ground_y, 300.0,
            "a floor 15 cells down is 300 pixels down at a 20-pixel cell"
        );
    }

    #[test]
    fn a_ballistic_entity_changes_state_when_it_touches_down() {
        // The cat's jump returns through the animation's `on_finish`, so today
        // it lands when the art happens to run out rather than when it reaches
        // the ground.
        let mut world = locomotion_world(
            falling(Some("ballistic")),
            TransitionConfig {
                on_land: Some("landed".to_string()),
                ..Default::default()
            },
        );

        for _ in 0..120 {
            world.update(1.0 / 60.0);
        }

        assert_eq!(
            world.entities[0].current_state, "landed",
            "a ballistic entity that reached its floor must fire on_land"
        );
    }

    #[test]
    fn on_land_does_not_fire_again_while_the_entity_rests_on_the_floor() {
        // Gravity re-accelerates a resting entity every tick and the clamp
        // catches it again, so a landing test written against the clamp alone
        // fires forever.
        let mut world = locomotion_world(
            falling(Some("ballistic")),
            TransitionConfig {
                on_land: Some("landed".to_string()),
                ..Default::default()
            },
        );
        // Already at rest on the floor: nothing has just landed.
        world.entities[0].y = 220.0;
        world.entities[0].vy = 0.0;
        world.entities[0].current_state = "flying".to_string();

        for _ in 0..30 {
            world.update(1.0 / 60.0);
        }

        assert_eq!(
            world.entities[0].current_state, "flying",
            "sitting on the ground is not a landing"
        );
    }

    #[test]
    fn a_grounded_entity_ignores_on_land() {
        let mut world = locomotion_world(
            falling(Some("grounded")),
            TransitionConfig {
                on_land: Some("landed".to_string()),
                ..Default::default()
            },
        );

        for _ in 0..120 {
            world.update(1.0 / 60.0);
        }

        assert_eq!(
            world.entities[0].current_state, "flying",
            "on_land belongs to ballistic locomotion, not to every floor"
        );
    }

    #[test]
    fn an_omitted_locomotion_is_derived_from_gravity() {
        // No manifest in the wild sets `locomotion`, so the derived value is
        // what every existing asset actually runs under.
        assert_eq!(falling(None).effective_locomotion(), "grounded");
        assert_eq!(
            PhysicsConfig::default().effective_locomotion(),
            "omnidirectional"
        );
    }

    #[test]
    fn a_linear_path_does_not_by_itself_keep_the_world_awake() {
        let world = path_world(PhysicsConfig {
            path_type: Some("linear".to_string()),
            ..Default::default()
        });
        assert!(
            world.is_quiescent(),
            "`linear` overrides no position, so it produces no new pictures"
        );
    }
}
