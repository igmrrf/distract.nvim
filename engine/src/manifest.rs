use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::render::RenderMode;

pub use crate::physics_config::{
    BALLISTIC, GROUNDED, LOCOMOTION_CLASSES, OMNIDIRECTIONAL, PathParams, PhysicsConfig,
    ResolvedPath, WrapMode,
};

pub use crate::spritesheet::SpritesheetConfig;

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
    /// How this asset is drawn, overriding the configured render mode.
    ///
    /// An asset that only reads as a flat overlay -- a speech bubble, a UI badge
    /// -- says so here, and keeps saying it in a 3D session. `None` follows the
    /// configuration.
    #[serde(default)]
    pub render: Option<RenderMode>,
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
        crate::manifests::cat::manifest()
    }

    /// Built-in Crab manifest with sideways walk, claw clipping, and burrowing.
    pub fn default_crab() -> Self {
        crate::manifests::crab::manifest()
    }

    /// Built-in Sun manifest: an omnidirectional body on a sine path.
    pub fn default_sun() -> Self {
        crate::manifests::sun::manifest()
    }
}
