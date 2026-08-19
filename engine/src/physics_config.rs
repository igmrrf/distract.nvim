//! How one state moves: the physics a state declares, and the path it follows.
//!
//! Split from `manifest.rs` because these are two questions with one file
//! between them. `manifest.rs` answers "what is this asset" -- its frames,
//! states, transitions and capabilities. This answers "how does a state move",
//! which is what both engines integrate every tick and what the physics-parity
//! fixtures pin. The locomotion class names live here for the same reason: the
//! validator, the ECS and the Lua engine must all spell them the same way.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
