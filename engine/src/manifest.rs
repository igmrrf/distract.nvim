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
    /// Special pathing type: e.g. "arc", "sine", "wander"
    pub path_type: Option<String>,
    pub path_amplitude: Option<f32>,
    pub path_frequency: Option<f32>,
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
            path_type: None,
            path_amplitude: None,
            path_frequency: None,
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
                    ..Default::default()
                },
                transitions: TransitionConfig {
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
