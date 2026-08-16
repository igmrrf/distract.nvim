use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::sprites;

/// Bounding wrap mode when an entity hits the edge of the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WrapMode {
    /// Entity wraps around to the opposite side of the screen.
    #[default]
    Wrap,
    /// Entity bounces off the screen edge (inverting velocity and toggling flip_x).
    Bounce,
    /// Entity is clamped within screen boundaries.
    Clamp,
    /// Entity is despawned when leaving the screen.
    Despawn,
    /// No boundary enforcement.
    None,
}

/// Spritesheet layout definition.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SpritesheetConfig {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub frame_width: Option<u32>,
    #[serde(default)]
    pub frame_height: Option<u32>,
    #[serde(default)]
    pub columns: Option<u32>,
    #[serde(default)]
    pub rows: Option<u32>,
    #[serde(default)]
    pub margin_x: Option<u32>,
    #[serde(default)]
    pub margin_y: Option<u32>,
    #[serde(default)]
    pub spacing_x: Option<u32>,
    #[serde(default)]
    pub spacing_y: Option<u32>,
}

impl<'de> Deserialize<'de> for SpritesheetConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SpritesheetVisitor;

        impl<'de> serde::de::Visitor<'de> for SpritesheetVisitor {
            type Value = SpritesheetConfig;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a spritesheet map, empty array, or null")
            }

            fn visit_seq<A>(self, _seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                Ok(SpritesheetConfig::default())
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                let mut cfg = SpritesheetConfig::default();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "path" => cfg.path = map.next_value()?,
                        "frame_width" => cfg.frame_width = map.next_value()?,
                        "frame_height" => cfg.frame_height = map.next_value()?,
                        "columns" => cfg.columns = map.next_value()?,
                        "rows" => cfg.rows = map.next_value()?,
                        "margin_x" => cfg.margin_x = map.next_value()?,
                        "margin_y" => cfg.margin_y = map.next_value()?,
                        "spacing_x" => cfg.spacing_x = map.next_value()?,
                        "spacing_y" => cfg.spacing_y = map.next_value()?,
                        _ => {
                            let _ = map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(cfg)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(SpritesheetConfig::default())
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(SpritesheetConfig::default())
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserializer.deserialize_any(self)
            }
        }

        deserializer.deserialize_any(SpritesheetVisitor)
    }
}

/// Animation configuration for a specific state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationConfig {
    /// Frame indices within the spritesheet atlas.
    #[serde(default)]
    pub frames: Vec<usize>,
    /// Animation playback speed in frames per second.
    #[serde(default = "default_fps")]
    pub fps: f32,
    /// Whether the animation loops continuously.
    #[serde(default = "default_true")]
    pub loop_anim: bool,
    /// Whether to flip frames horizontally.
    #[serde(default)]
    pub flip_x: bool,
}

fn default_fps() -> f32 {
    8.0
}

fn default_true() -> bool {
    true
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            frames: vec![0],
            fps: 8.0,
            loop_anim: true,
            flip_x: false,
        }
    }
}

/// Physics configuration for a specific state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicsConfig {
    #[serde(default)]
    pub target_vx: f32,
    #[serde(default)]
    pub target_vy: f32,
    #[serde(default)]
    pub accel_x: f32,
    #[serde(default)]
    pub accel_y: f32,
    #[serde(default)]
    pub gravity: f32,
    #[serde(default)]
    pub jump_impulse_y: Option<f32>,
    #[serde(default)]
    pub ground_y: Option<f32>,
    #[serde(default = "default_friction")]
    pub friction: f32,
    #[serde(default)]
    pub wrap_mode: WrapMode,
    /// How this state moves: `grounded`, `ballistic` or `omnidirectional`.
    ///
    /// Derived from `gravity` when omitted, which is what every manifest
    /// written before this field existed runs under.
    pub locomotion: Option<String>,
    /// Positional override applied on top of velocity integration.
    ///
    /// One of `linear`, `sine`, `orbital`, `lissajous`, `bezier`. Anything else
    /// is treated as `linear`.
    pub path_type: Option<String>,
    /// Legacy alias for `path_params.amp_y`.
    pub path_amplitude: Option<f32>,
    /// Legacy alias for `path_params.freq_y`.
    pub path_frequency: Option<f32>,
    #[serde(default)]
    pub path_params: Option<PathParams>,
}

/// Parameters for the path primitives selected by `path_type`.
///
/// Amplitudes are in sprite pixels, like every other distance in a manifest;
/// frequencies are dimensionless multipliers on one shared phase.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathParams {
    /// Rate the shared phase advances at, in radians per second.
    pub freq: Option<f32>,
    pub freq_x: Option<f32>,
    pub freq_y: Option<f32>,
    pub amp_x: Option<f32>,
    pub amp_y: Option<f32>,
    /// Phase offset applied to the x axis of a `lissajous` path.
    pub phase_delta: Option<f32>,
    /// Four cubic control points, in sprite pixels relative to the spawn point.
    #[serde(default, deserialize_with = "points_allowing_empty_table")]
    pub points: Option<Vec<[f32; 2]>>,
}

/// Reads a control-point list, accepting `{}` as well as `[]`.
///
/// `vim.json.encode` writes an empty Lua table as `{}`, and `points` is the
/// first array-valued field a manifest can carry. The engines both ignore a
/// list too short to draw with, so rejecting the encoding outright would make
/// one manifest parse in the terminal and fail on the overlay. A *keyed* table
/// is still an error: that is a mistake, not an empty path.
fn points_allowing_empty_table<'de, D>(d: D) -> Result<Option<Vec<[f32; 2]>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Points {
        List(Vec<[f32; 2]>),
        Table(HashMap<String, serde::de::IgnoredAny>),
    }

    match Option::<Points>::deserialize(d)? {
        None => Ok(None),
        Some(Points::List(points)) => Ok(Some(points)),
        Some(Points::Table(table)) if table.is_empty() => Ok(Some(Vec::new())),
        Some(Points::Table(_)) => Err(serde::de::Error::custom(
            "path_params.points must be a list of [x, y] pairs",
        )),
    }
}

/// Path parameters with the legacy aliases and the defaults filled in.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedPath {
    pub freq: f32,
    pub freq_x: f32,
    pub freq_y: f32,
    pub amp_x: f32,
    pub amp_y: f32,
    pub phase_delta: f32,
}

/// Locomotion class names, in one place so both engines and the validator
/// agree on the spelling.
pub const GROUNDED: &str = "grounded";
pub const BALLISTIC: &str = "ballistic";
pub const OMNIDIRECTIONAL: &str = "omnidirectional";
pub const LOCOMOTION_CLASSES: [&str; 3] = [GROUNDED, BALLISTIC, OMNIDIRECTIONAL];

/// Paths that move y at most, and so do not need a floor-free state.
const FLOOR_SAFE_PATHS: [&str; 2] = ["linear", "sine"];

impl AssetManifest {
    /// The locomotion class a state runs under.
    ///
    /// The state's own value wins, then the asset-level default, then the
    /// derivation from gravity that keeps pre-`locomotion` manifests working.
    pub fn locomotion_for<'a>(&'a self, state: &'a StateDefinition) -> &'a str {
        state
            .physics
            .locomotion
            .as_deref()
            .or(self.locomotion.as_deref())
            .unwrap_or_else(|| state.physics.effective_locomotion())
    }

    /// Checks every state against this asset's declared capabilities.
    ///
    /// Run once at load rather than per frame: a manifest that cannot work is
    /// worth one message when it arrives, not thirty a second forever.
    ///
    /// # Errors
    ///
    /// Returns the first violation found, naming the state responsible.
    pub fn validate_capabilities(&self) -> Result<(), String> {
        let mut states: Vec<&String> = self.states.keys().collect();
        // HashMap order is arbitrary, and an error that names a different state
        // on every run is an error nobody can reproduce.
        states.sort();

        for name in states {
            let Some(state) = self.states.get(name) else {
                continue;
            };
            let locomotion = self.locomotion_for(state);

            if !LOCOMOTION_CLASSES.contains(&locomotion) {
                return Err(format!(
                    "state '{name}' declares an unknown locomotion '{locomotion}'; \
                     expected one of {}",
                    LOCOMOTION_CLASSES.join(", ")
                ));
            }

            if locomotion == OMNIDIRECTIONAL && state.physics.gravity > 0.0 {
                return Err(format!(
                    "state '{name}' declares '{OMNIDIRECTIONAL}' locomotion but sets \
                     gravity {}; gravity brings a floor with it, so the state would \
                     clamp to a floor it claims not to have",
                    state.physics.gravity
                ));
            }

            if let Some(path) = state.physics.path_type.as_deref() {
                if !FLOOR_SAFE_PATHS.contains(&path) && locomotion != OMNIDIRECTIONAL {
                    return Err(format!(
                        "state '{name}' uses the '{path}' path, which writes x directly \
                         and needs '{OMNIDIRECTIONAL}' locomotion, but the state is \
                         '{locomotion}'"
                    ));
                }
            }

            if let Some(ref allowed) = self.capabilities.locomotion {
                if !allowed.iter().any(|class| class == locomotion) {
                    return Err(format!(
                        "state '{name}' uses '{locomotion}' locomotion, which '{}' does \
                         not declare; capabilities.locomotion allows {}",
                        self.name,
                        allowed.join(", ")
                    ));
                }
            }
        }

        Ok(())
    }
}

impl PhysicsConfig {
    /// This state's locomotion class, derived when the manifest omits it.
    ///
    /// No manifest written before the field existed sets it, so the derived
    /// value has to be the behaviour those manifests already had: a floor when
    /// there is gravity to fall under, free movement otherwise.
    pub fn effective_locomotion(&self) -> &str {
        match self.locomotion.as_deref() {
            Some(explicit) => explicit,
            None if self.gravity > 0.0 => GROUNDED,
            None => OMNIDIRECTIONAL,
        }
    }

    /// Resolves this state's path parameters.
    ///
    /// `path_amplitude` and `path_frequency` predate `path_params` and are
    /// exactly `amp_y` and `freq_y` under older names -- the sun's manifest
    /// still uses them, and so may anyone else's.
    pub fn resolved_path(&self) -> ResolvedPath {
        let p = self.path_params.as_ref();
        let amp_y = p
            .and_then(|p| p.amp_y)
            .or(self.path_amplitude)
            .unwrap_or(4.0);
        let freq_y = p
            .and_then(|p| p.freq_y)
            .or(self.path_frequency)
            .unwrap_or(2.0);
        ResolvedPath {
            freq: p.and_then(|p| p.freq).unwrap_or(1.0),
            // Defaulting the x axis to the y axis makes an `orbital` path with
            // no parameters a circle rather than a flat line.
            freq_x: p.and_then(|p| p.freq_x).unwrap_or(freq_y),
            freq_y,
            amp_x: p.and_then(|p| p.amp_x).unwrap_or(amp_y),
            amp_y,
            phase_delta: p.and_then(|p| p.phase_delta).unwrap_or(0.0),
        }
    }
}

fn default_friction() -> f32 {
    0.05
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            target_vx: 0.0,
            target_vy: 0.0,
            accel_x: 0.0,
            accel_y: 0.0,
            gravity: 0.0,
            jump_impulse_y: None,
            ground_y: None,
            friction: 0.05,
            wrap_mode: WrapMode::Wrap,
            locomotion: None,
            path_type: None,
            path_amplitude: None,
            path_frequency: None,
            path_params: None,
        }
    }
}

/// State transition rules.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransitionConfig {
    /// Mapping of editor event name (e.g. "typing", "scrolling", "idle") to target state.
    #[serde(default)]
    pub on_event: HashMap<String, String>,
    /// State to switch to when a non-looping animation completes.
    pub on_finish: Option<String>,
    /// Duration in milliseconds after which to transition to `on_timeout_state`.
    pub timeout_ms: Option<u64>,
    /// Target state when `timeout_ms` expires.
    pub on_timeout: Option<String>,
    /// State to switch to when hitting the left edge.
    pub on_edge_left: Option<String>,
    /// State to switch to when hitting the right edge.
    pub on_edge_right: Option<String>,
    /// State to switch to on the tick a `ballistic` entity reaches its floor.
    pub on_land: Option<String>,
}

/// Definition of a single entity state (e.g. "walk", "jump", "sleep", "eclipse").
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateDefinition {
    #[serde(default)]
    pub animation: AnimationConfig,
    #[serde(default)]
    pub physics: PhysicsConfig,
    #[serde(default)]
    pub transitions: TransitionConfig,
    /// If true, background editor events (typing/moving) will not interrupt this state.
    #[serde(default)]
    pub is_locked: bool,
}

/// Definition of a custom action callable from user commands or IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomActionConfig {
    pub target_state: String,
    pub duration_ms: Option<u64>,
    pub return_state: Option<String>,
    #[serde(default)]
    pub is_locked: Option<bool>,
}

/// Complete asset manifest representing an entity type (e.g. Cat, Crab, Sun).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetManifest {
    pub name: String,
    #[serde(default = "default_asset_type")]
    pub asset_type: String, // "sprite", "gif", "procedural", "shader"
    #[serde(default)]
    pub spritesheet: SpritesheetConfig,
    #[serde(default = "default_initial_state")]
    pub initial_state: String,
    #[serde(default)]
    pub states: HashMap<String, StateDefinition>,
    #[serde(default)]
    pub custom_actions: HashMap<String, CustomActionConfig>,
    #[serde(default)]
    pub z_index: Option<i32>,
    /// Locomotion classes this asset is allowed to use, checked at load.
    #[serde(default)]
    pub capabilities: Capabilities,
    /// Locomotion for every state that does not name its own.
    ///
    /// Only the cat's jump has gravity, so without this an asset would have to
    /// repeat `locomotion` in each state's physics to say the one thing that is
    /// true of all of them.
    #[serde(default)]
    pub locomotion: Option<String>,
}

/// What an asset declares it is allowed to do.
///
/// Permissive when omitted, so no manifest written before this existed can
/// newly fail to load. Only an asset that declares capabilities can violate
/// them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    pub locomotion: Option<Vec<String>>,
}

fn default_asset_type() -> String {
    "sprite".to_string()
}

fn default_initial_state() -> String {
    "idle".to_string()
}

impl AssetManifest {
    /// Built-in procedural Cat manifest with walk, jump, yawn, sleep, sit behaviors.
    pub fn default_cat() -> Self {
        let mut states = HashMap::new();

        // Idle state: transitions to walk on typing/moving, sleep on idle timeout
        let mut idle_transitions = HashMap::new();
        idle_transitions.insert("typing".to_string(), "walk_fast".to_string());
        idle_transitions.insert("moving".to_string(), "walk".to_string());
        idle_transitions.insert("scrolling".to_string(), "yawn".to_string());
        idle_transitions.insert("idle".to_string(), "sleep".to_string());

        states.insert(
            "idle".to_string(),
            StateDefinition {
                animation: AnimationConfig {
                    frames: sprites::cat_set().frames_for("idle"),
                    fps: 2.0,
                    loop_anim: true,
                    flip_x: false,
                },
                physics: PhysicsConfig {
                    target_vx: 0.0,
                    target_vy: 0.0,
                    friction: 0.1,
                    wrap_mode: WrapMode::Clamp,
                    ..Default::default()
                },
                transitions: TransitionConfig {
                    on_event: idle_transitions,
                    timeout_ms: Some(6000),
                    on_timeout: Some("sleep".to_string()),
                    ..Default::default()
                },
                is_locked: false,
            },
        );

        // Walk right
        let mut walk_transitions = HashMap::new();
        walk_transitions.insert("typing".to_string(), "walk_fast".to_string());
        walk_transitions.insert("idle".to_string(), "idle".to_string());
        walk_transitions.insert("scrolling".to_string(), "yawn".to_string());

        states.insert(
            "walk".to_string(),
            StateDefinition {
                animation: AnimationConfig {
                    frames: sprites::cat_set().frames_for("walk"),
                    fps: 6.0,
                    loop_anim: true,
                    flip_x: false,
                },
                physics: PhysicsConfig {
                    target_vx: 1.5,
                    target_vy: 0.0,
                    wrap_mode: WrapMode::Wrap,
                    ..Default::default()
                },
                transitions: TransitionConfig {
                    on_event: walk_transitions,
                    ..Default::default()
                },
                is_locked: false,
            },
        );

        // Walk fast (sprinting when typing)
        let mut walk_fast_transitions = HashMap::new();
        walk_fast_transitions.insert("idle".to_string(), "idle".to_string());
        walk_fast_transitions.insert("moving".to_string(), "walk".to_string());
        walk_fast_transitions.insert("scrolling".to_string(), "yawn".to_string());

        states.insert(
            "walk_fast".to_string(),
            StateDefinition {
                animation: AnimationConfig {
                    frames: sprites::cat_set().frames_for("walk_fast"),
                    fps: 12.0,
                    loop_anim: true,
                    flip_x: false,
                },
                physics: PhysicsConfig {
                    target_vx: 3.5,
                    target_vy: 0.0,
                    wrap_mode: WrapMode::Wrap,
                    ..Default::default()
                },
                transitions: TransitionConfig {
                    on_event: walk_fast_transitions,
                    timeout_ms: Some(1500),
                    on_timeout: Some("walk".to_string()),
                    ..Default::default()
                },
                is_locked: false,
            },
        );

        // Jump: launches vertically with gravity, returns to idle on landing
        states.insert(
            "jump".to_string(),
            StateDefinition {
                animation: AnimationConfig {
                    frames: sprites::cat_set().frames_for("jump"),
                    fps: 10.0,
                    loop_anim: false,
                    flip_x: false,
                },
                physics: PhysicsConfig {
                    target_vx: 2.0,
                    target_vy: 0.0,
                    jump_impulse_y: Some(-2.2),
                    gravity: 0.32,
                    wrap_mode: WrapMode::Bounce,
                    // The one state that leaves the ground, and the reason the
                    // cat declares `ballistic` at all.
                    locomotion: Some(BALLISTIC.to_string()),
                    ..Default::default()
                },
                transitions: TransitionConfig {
                    // The jump ends when the cat lands, not when a clock says
                    // it should have. `timeout_ms` stays as the
                    // floor-never-reached fallback: a duration tuned against
                    // `gravity` and `jump_impulse_y` is a number that has to be
                    // re-tuned by hand every time either of them moves.
                    on_land: Some("idle".to_string()),
                    timeout_ms: Some(1200),
                    on_timeout: Some("idle".to_string()),
                    ..Default::default()
                },
                is_locked: true,
            },
        );

        // Yawn
        states.insert(
            "yawn".to_string(),
            StateDefinition {
                animation: AnimationConfig {
                    frames: sprites::cat_set().frames_for("yawn"),
                    fps: 3.0,
                    loop_anim: false,
                    flip_x: false,
                },
                physics: PhysicsConfig {
                    target_vx: 0.0,
                    target_vy: 0.0,
                    ..Default::default()
                },
                transitions: TransitionConfig {
                    on_finish: Some("sleep".to_string()),
                    timeout_ms: Some(2000),
                    on_timeout: Some("sleep".to_string()),
                    ..Default::default()
                },
                is_locked: true,
            },
        );

        // Sleep
        let mut sleep_transitions = HashMap::new();
        sleep_transitions.insert("typing".to_string(), "yawn".to_string());
        sleep_transitions.insert("moving".to_string(), "idle".to_string());

        states.insert(
            "sleep".to_string(),
            StateDefinition {
                animation: AnimationConfig {
                    frames: sprites::cat_set().frames_for("sleep"),
                    fps: 1.0,
                    loop_anim: true,
                    flip_x: false,
                },
                physics: PhysicsConfig {
                    target_vx: 0.0,
                    target_vy: 0.0,
                    friction: 0.2,
                    ..Default::default()
                },
                transitions: TransitionConfig {
                    on_event: sleep_transitions,
                    ..Default::default()
                },
                is_locked: false,
            },
        );

        // Custom actions
        let mut custom_actions = HashMap::new();
        custom_actions.insert(
            "jump".to_string(),
            CustomActionConfig {
                target_state: "jump".to_string(),
                duration_ms: Some(1200),
                return_state: Some("idle".to_string()),
                is_locked: Some(true),
            },
        );
        custom_actions.insert(
            "yawn".to_string(),
            CustomActionConfig {
                target_state: "yawn".to_string(),
                duration_ms: Some(2000),
                return_state: Some("sleep".to_string()),
                is_locked: Some(true),
            },
        );
        custom_actions.insert(
            "sleep".to_string(),
            CustomActionConfig {
                target_state: "sleep".to_string(),
                duration_ms: None,
                return_state: None,
                is_locked: Some(false),
            },
        );
        custom_actions.insert(
            "wake".to_string(),
            CustomActionConfig {
                target_state: "idle".to_string(),
                duration_ms: None,
                return_state: None,
                is_locked: Some(false),
            },
        );

        Self {
            name: "cat".to_string(),
            asset_type: "sprite".to_string(),
            spritesheet: SpritesheetConfig::default(),
            initial_state: "idle".to_string(),
            states,
            custom_actions,
            z_index: Some(10),
            capabilities: Capabilities {
                locomotion: Some(vec![GROUNDED.to_string(), BALLISTIC.to_string()]),
            },
            locomotion: Some(GROUNDED.to_string()),
        }
    }

    /// Built-in Crab manifest with sideways walk, claw clipping, and burrowing.
    pub fn default_crab() -> Self {
        let mut states = HashMap::new();

        // Idle / standing
        let mut idle_trans = HashMap::new();
        idle_trans.insert("typing".to_string(), "walk_fast".to_string());
        idle_trans.insert("moving".to_string(), "walk".to_string());
        idle_trans.insert("scrolling".to_string(), "clip_claws".to_string());
        idle_trans.insert("idle".to_string(), "sleep".to_string());

        states.insert(
            "idle".to_string(),
            StateDefinition {
                animation: AnimationConfig {
                    frames: sprites::crab_set().frames_for("idle"),
                    fps: 2.0,
                    loop_anim: true,
                    flip_x: false,
                },
                physics: PhysicsConfig {
                    target_vx: 0.0,
                    target_vy: 0.0,
                    wrap_mode: WrapMode::Clamp,
                    ..Default::default()
                },
                transitions: TransitionConfig {
                    on_event: idle_trans,
                    timeout_ms: Some(8000),
                    on_timeout: Some("clip_claws".to_string()),
                    ..Default::default()
                },
                is_locked: false,
            },
        );

        // Walk sideways
        let mut walk_trans = HashMap::new();
        walk_trans.insert("typing".to_string(), "walk_fast".to_string());
        walk_trans.insert("idle".to_string(), "idle".to_string());

        states.insert(
            "walk".to_string(),
            StateDefinition {
                animation: AnimationConfig {
                    frames: sprites::crab_set().frames_for("walk"),
                    fps: 5.0,
                    loop_anim: true,
                    flip_x: false,
                },
                physics: PhysicsConfig {
                    target_vx: 1.2,
                    target_vy: 0.0,
                    wrap_mode: WrapMode::Bounce,
                    ..Default::default()
                },
                transitions: TransitionConfig {
                    on_event: walk_trans,
                    ..Default::default()
                },
                is_locked: false,
            },
        );

        // Walk fast
        let mut crab_walk_fast_trans = HashMap::new();
        crab_walk_fast_trans.insert("scrolling".to_string(), "clip_claws".to_string());
        crab_walk_fast_trans.insert("idle".to_string(), "idle".to_string());

        states.insert(
            "walk_fast".to_string(),
            StateDefinition {
                animation: AnimationConfig {
                    frames: sprites::crab_set().frames_for("walk_fast"),
                    fps: 10.0,
                    loop_anim: true,
                    flip_x: false,
                },
                physics: PhysicsConfig {
                    target_vx: 2.8,
                    target_vy: 0.0,
                    wrap_mode: WrapMode::Bounce,
                    ..Default::default()
                },
                transitions: TransitionConfig {
                    on_event: crab_walk_fast_trans,
                    timeout_ms: Some(2000),
                    on_timeout: Some("walk".to_string()),
                    ..Default::default()
                },
                is_locked: false,
            },
        );

        // Clip claws
        states.insert(
            "clip_claws".to_string(),
            StateDefinition {
                animation: AnimationConfig {
                    frames: sprites::crab_set().frames_for("clip_claws"),
                    fps: 6.0,
                    loop_anim: false,
                    flip_x: false,
                },
                physics: PhysicsConfig {
                    target_vx: 0.0,
                    target_vy: 0.0,
                    ..Default::default()
                },
                transitions: TransitionConfig {
                    on_finish: Some("idle".to_string()),
                    timeout_ms: Some(1500),
                    on_timeout: Some("idle".to_string()),
                    ..Default::default()
                },
                is_locked: true,
            },
        );

        // Burrow
        states.insert(
            "burrow".to_string(),
            StateDefinition {
                animation: AnimationConfig {
                    frames: sprites::crab_set().frames_for("burrow"),
                    fps: 4.0,
                    loop_anim: false,
                    flip_x: false,
                },
                physics: PhysicsConfig {
                    target_vx: 0.0,
                    target_vy: 0.5,
                    ..Default::default()
                },
                transitions: TransitionConfig {
                    timeout_ms: Some(3000),
                    on_timeout: Some("sleep".to_string()),
                    ..Default::default()
                },
                is_locked: true,
            },
        );

        // Sleep
        let mut sleep_trans = HashMap::new();
        sleep_trans.insert("typing".to_string(), "clip_claws".to_string());
        sleep_trans.insert("moving".to_string(), "idle".to_string());

        states.insert(
            "sleep".to_string(),
            StateDefinition {
                animation: AnimationConfig {
                    frames: sprites::crab_set().frames_for("sleep"),
                    fps: 1.0,
                    loop_anim: true,
                    flip_x: false,
                },
                physics: PhysicsConfig {
                    target_vx: 0.0,
                    target_vy: 0.0,
                    ..Default::default()
                },
                transitions: TransitionConfig {
                    on_event: sleep_trans,
                    ..Default::default()
                },
                is_locked: false,
            },
        );

        let mut custom_actions = HashMap::new();
        custom_actions.insert(
            "clip".to_string(),
            CustomActionConfig {
                target_state: "clip_claws".to_string(),
                duration_ms: Some(1500),
                return_state: Some("idle".to_string()),
                is_locked: Some(true),
            },
        );
        custom_actions.insert(
            "burrow".to_string(),
            CustomActionConfig {
                target_state: "burrow".to_string(),
                duration_ms: Some(3000),
                return_state: Some("sleep".to_string()),
                is_locked: Some(true),
            },
        );
        custom_actions.insert(
            "sleep".to_string(),
            CustomActionConfig {
                target_state: "sleep".to_string(),
                duration_ms: None,
                return_state: None,
                is_locked: Some(false),
            },
        );
        custom_actions.insert(
            "wake".to_string(),
            CustomActionConfig {
                target_state: "idle".to_string(),
                duration_ms: None,
                return_state: None,
                is_locked: Some(false),
            },
        );
        custom_actions.insert(
            "walk".to_string(),
            CustomActionConfig {
                target_state: "walk".to_string(),
                duration_ms: None,
                return_state: None,
                is_locked: Some(false),
            },
        );

        Self {
            name: "crab".to_string(),
            asset_type: "procedural".to_string(),
            spritesheet: SpritesheetConfig::default(),
            initial_state: "idle".to_string(),
            states,
            custom_actions,
            z_index: Some(10),
            capabilities: Capabilities {
                locomotion: Some(vec![GROUNDED.to_string()]),
            },
            locomotion: Some(GROUNDED.to_string()),
        }
    }

    /// Built-in Sun manifest with shining, rise, set, eclipse, and solar flare.
    pub fn default_sun() -> Self {
        let mut states = HashMap::new();

        // Shining (default state)
        let mut shine_trans = HashMap::new();
        shine_trans.insert("scrolling".to_string(), "flare".to_string());

        states.insert(
            "shining".to_string(),
            StateDefinition {
                animation: AnimationConfig {
                    frames: sprites::sun_set().frames_for("shining"),
                    fps: 2.0,
                    loop_anim: true,
                    flip_x: false,
                },
                physics: PhysicsConfig {
                    target_vx: 0.2,
                    target_vy: 0.0,
                    wrap_mode: WrapMode::Wrap,
                    path_type: Some("sine".to_string()),
                    path_amplitude: Some(15.0),
                    path_frequency: Some(2.0),
                    ..Default::default()
                },
                transitions: TransitionConfig {
                    on_event: shine_trans,
                    ..Default::default()
                },
                is_locked: false,
            },
        );

        // Rising (moves upwards)
        states.insert(
            "rising".to_string(),
            StateDefinition {
                animation: AnimationConfig {
                    frames: sprites::sun_set().frames_for("rising"),
                    fps: 2.0,
                    loop_anim: true,
                    flip_x: false,
                },
                physics: PhysicsConfig {
                    target_vx: 0.5,
                    target_vy: -1.0,
                    wrap_mode: WrapMode::Clamp,
                    ..Default::default()
                },
                transitions: TransitionConfig {
                    timeout_ms: Some(4000),
                    on_timeout: Some("shining".to_string()),
                    ..Default::default()
                },
                is_locked: true,
            },
        );

        // Setting (moves downwards)
        states.insert(
            "setting".to_string(),
            StateDefinition {
                animation: AnimationConfig {
                    frames: sprites::sun_set().frames_for("setting"),
                    fps: 2.0,
                    loop_anim: true,
                    flip_x: false,
                },
                physics: PhysicsConfig {
                    target_vx: 0.5,
                    target_vy: 1.0,
                    wrap_mode: WrapMode::Clamp,
                    ..Default::default()
                },
                transitions: TransitionConfig {
                    timeout_ms: Some(4000),
                    on_timeout: Some("shining".to_string()),
                    ..Default::default()
                },
                is_locked: true,
            },
        );

        // Eclipse (moon blocks sun corona)
        states.insert(
            "eclipse".to_string(),
            StateDefinition {
                animation: AnimationConfig {
                    frames: sprites::sun_set().frames_for("eclipse"),
                    fps: 1.0,
                    loop_anim: true,
                    flip_x: false,
                },
                physics: PhysicsConfig {
                    target_vx: 0.0,
                    target_vy: 0.0,
                    ..Default::default()
                },
                transitions: TransitionConfig {
                    timeout_ms: Some(8000),
                    on_timeout: Some("shining".to_string()),
                    ..Default::default()
                },
                is_locked: true,
            },
        );

        // Solar flare
        states.insert(
            "flare".to_string(),
            StateDefinition {
                animation: AnimationConfig {
                    frames: sprites::sun_set().frames_for("flare"),
                    fps: 6.0,
                    loop_anim: false,
                    flip_x: false,
                },
                physics: PhysicsConfig {
                    target_vx: 0.4,
                    target_vy: 0.0,
                    ..Default::default()
                },
                transitions: TransitionConfig {
                    on_finish: Some("shining".to_string()),
                    timeout_ms: Some(2000),
                    on_timeout: Some("shining".to_string()),
                    ..Default::default()
                },
                is_locked: true,
            },
        );

        let mut custom_actions = HashMap::new();
        custom_actions.insert(
            "eclipse".to_string(),
            CustomActionConfig {
                target_state: "eclipse".to_string(),
                duration_ms: Some(8000),
                return_state: Some("shining".to_string()),
                is_locked: Some(true),
            },
        );
        custom_actions.insert(
            "rise".to_string(),
            CustomActionConfig {
                target_state: "rising".to_string(),
                duration_ms: Some(4000),
                return_state: Some("shining".to_string()),
                is_locked: Some(true),
            },
        );
        custom_actions.insert(
            "set".to_string(),
            CustomActionConfig {
                target_state: "setting".to_string(),
                duration_ms: Some(4000),
                return_state: Some("shining".to_string()),
                is_locked: Some(true),
            },
        );
        custom_actions.insert(
            "flare".to_string(),
            CustomActionConfig {
                target_state: "flare".to_string(),
                duration_ms: Some(2000),
                return_state: Some("shining".to_string()),
                is_locked: Some(true),
            },
        );

        Self {
            name: "sun".to_string(),
            asset_type: "procedural".to_string(),
            spritesheet: SpritesheetConfig::default(),
            initial_state: "shining".to_string(),
            states,
            custom_actions,
            z_index: Some(-10),
            capabilities: Capabilities {
                locomotion: Some(vec![OMNIDIRECTIONAL.to_string()]),
            },
            locomotion: Some(OMNIDIRECTIONAL.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-state manifest declaring `capabilities` and running `physics`.
    fn declared(allowed: Option<&[&str]>, physics: PhysicsConfig) -> AssetManifest {
        let mut manifest = AssetManifest::default_cat();
        manifest.name = "declared".to_string();
        manifest.initial_state = "only".to_string();
        manifest.locomotion = None;
        manifest.capabilities = Capabilities {
            locomotion: allowed.map(|list| list.iter().map(|s| s.to_string()).collect()),
        };
        manifest.states.clear();
        manifest.states.insert(
            "only".to_string(),
            StateDefinition {
                physics,
                ..Default::default()
            },
        );
        manifest
    }

    #[test]
    fn a_manifest_without_capabilities_accepts_any_locomotion() {
        let manifest = declared(
            None,
            PhysicsConfig {
                locomotion: Some(OMNIDIRECTIONAL.to_string()),
                ..Default::default()
            },
        );
        assert!(manifest.validate_capabilities().is_ok());
    }

    #[test]
    fn a_state_outside_the_declared_locomotion_is_rejected() {
        let manifest = declared(
            Some(&[GROUNDED]),
            PhysicsConfig {
                locomotion: Some(BALLISTIC.to_string()),
                gravity: 0.3,
                ..Default::default()
            },
        );
        let message = manifest
            .validate_capabilities()
            .expect_err("a ballistic state under a grounded-only asset must not load");
        assert!(
            message.contains("only") && message.contains(BALLISTIC),
            "the message must name the offending state and class, got: {message}"
        );
    }

    #[test]
    fn an_exotic_path_requires_omnidirectional_locomotion() {
        // Anything past `linear` and `sine` writes x directly, which fights a
        // floor. The engines skip paths entirely under gravity, so a grounded
        // orbit would silently do nothing at all.
        let manifest = declared(
            None,
            PhysicsConfig {
                locomotion: Some(GROUNDED.to_string()),
                path_type: Some("orbital".to_string()),
                ..Default::default()
            },
        );
        assert!(manifest.validate_capabilities().is_err());
    }

    #[test]
    fn sine_and_linear_paths_are_allowed_on_the_ground() {
        for path in ["linear", "sine"] {
            let manifest = declared(
                None,
                PhysicsConfig {
                    locomotion: Some(GROUNDED.to_string()),
                    path_type: Some(path.to_string()),
                    ..Default::default()
                },
            );
            assert!(
                manifest.validate_capabilities().is_ok(),
                "{path} moves y at most, so it does not need omnidirectional"
            );
        }
    }

    #[test]
    fn declaring_omnidirectional_while_gravity_pulls_is_a_contradiction() {
        // The gravity branch wins at runtime, so the state would clamp to a
        // floor while claiming to have none.
        let manifest = declared(
            None,
            PhysicsConfig {
                locomotion: Some(OMNIDIRECTIONAL.to_string()),
                gravity: 0.4,
                ..Default::default()
            },
        );
        assert!(manifest.validate_capabilities().is_err());
    }

    #[test]
    fn an_unknown_locomotion_name_is_rejected() {
        let manifest = declared(
            None,
            PhysicsConfig {
                locomotion: Some("hovering".to_string()),
                ..Default::default()
            },
        );
        assert!(manifest.validate_capabilities().is_err());
    }

    #[test]
    fn a_state_inherits_the_manifest_locomotion_when_it_names_none() {
        let mut manifest = declared(Some(&[GROUNDED]), PhysicsConfig::default());
        manifest.locomotion = Some(GROUNDED.to_string());
        assert_eq!(
            manifest.locomotion_for(&manifest.states["only"]),
            GROUNDED,
            "a walking state has no gravity, so without the asset-level default \
             it would derive omnidirectional and violate its own declaration"
        );
        assert!(manifest.validate_capabilities().is_ok());
    }

    #[test]
    fn every_builtin_satisfies_the_capabilities_it_declares() {
        for manifest in [
            AssetManifest::default_cat(),
            AssetManifest::default_crab(),
            AssetManifest::default_sun(),
        ] {
            let name = manifest.name.clone();
            assert!(
                manifest.capabilities.locomotion.is_some(),
                "{name} should declare what it can do, or the gate proves nothing"
            );
            manifest
                .validate_capabilities()
                .unwrap_or_else(|error| panic!("{name} violates its own declaration: {error}"));
        }
    }

    #[test]
    fn an_empty_points_table_survives_the_lua_json_encoding() {
        // `vim.json.encode` writes an empty Lua table as `{}`, not `[]`, and
        // `points` is the first array-valued field a manifest can carry. The
        // terminal backend merely ignores a points list too short to draw with,
        // so without this the same manifest would fail to parse on the overlay
        // and describe two behaviours.
        let phys: PhysicsConfig =
            serde_json::from_str(r#"{"path_type":"bezier","path_params":{"points":{}}}"#)
                .expect("an empty points table must parse");
        assert_eq!(phys.path_params.and_then(|p| p.points), Some(Vec::new()));
    }

    #[test]
    fn a_points_table_that_is_not_a_list_is_still_an_error() {
        let err = serde_json::from_str::<PhysicsConfig>(
            r#"{"path_params":{"points":{"first":[1.0,2.0]}}}"#,
        );
        assert!(
            err.is_err(),
            "a keyed table is a mistake worth reporting, not an empty path"
        );
    }

    #[test]
    fn test_default_manifests() {
        let cat = AssetManifest::default_cat();
        assert_eq!(cat.name, "cat");
        assert_eq!(cat.initial_state, "idle");
        assert!(cat.states.contains_key("idle"));
        assert!(cat.states.contains_key("walk"));
        assert!(cat.states.contains_key("walk_fast"));
        assert!(cat.states.contains_key("jump"));
        assert!(cat.states.contains_key("yawn"));
        assert!(cat.states.contains_key("sleep"));
        assert!(cat.custom_actions.contains_key("jump"));
        assert!(cat.custom_actions.contains_key("yawn"));

        let crab = AssetManifest::default_crab();
        assert_eq!(crab.name, "crab");
        assert_eq!(crab.initial_state, "idle");
        assert!(crab.states.contains_key("clip_claws"));
        assert!(crab.states.contains_key("burrow"));
        assert!(crab.custom_actions.contains_key("clip"));
        assert!(crab.custom_actions.contains_key("burrow"));

        let sun = AssetManifest::default_sun();
        assert_eq!(sun.name, "sun");
        assert_eq!(sun.initial_state, "shining");
        assert!(sun.states.contains_key("eclipse"));
        assert!(sun.states.contains_key("rising"));
        assert!(sun.states.contains_key("setting"));
        assert!(sun.custom_actions.contains_key("eclipse"));
        assert!(sun.custom_actions.contains_key("rise"));
    }

    #[test]
    fn test_custom_json_deserialization() {
        let json_data = r#"{
            "name": "custom_dragon",
            "asset_type": "sprite",
            "initial_state": "fly",
            "spritesheet": {
                "path": "assets/dragon.png",
                "frame_width": 64,
                "frame_height": 64,
                "columns": 8,
                "rows": 2
            },
            "states": {
                "fly": {
                    "animation": { "frames": [0, 1, 2, 3], "fps": 10.0, "loop_anim": true, "flip_x": false },
                    "physics": { "target_vx": 4.0, "target_vy": -1.0, "gravity": 0.0, "friction": 0.05, "wrap_mode": "bounce" },
                    "transitions": {
                        "on_event": { "typing": "breathe_fire" },
                        "on_edge_left": "turn_right",
                        "timeout_ms": 5000,
                        "on_timeout": "glide"
                    }
                }
            },
            "custom_actions": {
                "fire": {
                    "target_state": "breathe_fire",
                    "duration_ms": 3000,
                    "return_state": "fly"
                }
            }
        }"#;

        let manifest: AssetManifest =
            serde_json::from_str(json_data).expect("Should deserialize valid manifest");
        assert_eq!(manifest.name, "custom_dragon");
        assert_eq!(manifest.spritesheet.frame_width, Some(64));
        assert_eq!(manifest.spritesheet.columns, Some(8));
        assert_eq!(manifest.initial_state, "fly");

        let fly_state = manifest.states.get("fly").unwrap();
        assert_eq!(fly_state.animation.frames, vec![0, 1, 2, 3]);
        assert_eq!(fly_state.animation.fps, 10.0);
        assert_eq!(fly_state.physics.wrap_mode, WrapMode::Bounce);
        assert_eq!(fly_state.transitions.timeout_ms, Some(5000));
        assert_eq!(fly_state.transitions.on_timeout, Some("glide".to_string()));

        let fire_action = manifest.custom_actions.get("fire").unwrap();
        assert_eq!(fire_action.target_state, "breathe_fire");
        assert_eq!(fire_action.duration_ms, Some(3000));
    }

    #[test]
    fn test_wrap_mode_variants() {
        let modes = vec![
            (r#""wrap""#, WrapMode::Wrap),
            (r#""bounce""#, WrapMode::Bounce),
            (r#""clamp""#, WrapMode::Clamp),
            (r#""despawn""#, WrapMode::Despawn),
            (r#""none""#, WrapMode::None),
        ];

        for (json_str, expected) in modes {
            let parsed: WrapMode = serde_json::from_str(json_str).unwrap();
            assert_eq!(parsed, expected);
        }
    }

    #[test]
    fn test_spritesheet_config_deserialization_formats() {
        // Empty array format from Lua json_encode({})
        let json_seq = r#"{"name":"crab","spritesheet":[],"initial_state":"idle"}"#;
        let manifest_seq: AssetManifest =
            serde_json::from_str(json_seq).expect("Should deserialize empty seq spritesheet");
        assert_eq!(manifest_seq.name, "crab");
        assert_eq!(manifest_seq.spritesheet.path, None);

        // Empty object format
        let json_map = r#"{"name":"crab","spritesheet":{},"initial_state":"idle"}"#;
        let manifest_map: AssetManifest =
            serde_json::from_str(json_map).expect("Should deserialize empty map spritesheet");
        assert_eq!(manifest_map.name, "crab");

        // Null format
        let json_null = r#"{"name":"crab","spritesheet":null,"initial_state":"idle"}"#;
        let manifest_null: AssetManifest =
            serde_json::from_str(json_null).expect("Should deserialize null spritesheet");
        assert_eq!(manifest_null.name, "crab");
    }
}
