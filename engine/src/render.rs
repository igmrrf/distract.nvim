//! What the renderer draws, and how.
//!
//! One settings block, pushed from Neovim exactly as the floor and the viewport
//! scope are, so the terminal backends and the overlay read the same numbers.
//! Nothing here measures anything for itself.

use serde::{Deserialize, Serialize};

use crate::camera::{Camera, DEFAULT_DEPTH_PER_UNIT, DEFAULT_FOV_Y_DEGREES};
use crate::voxel::{DEFAULT_DEPTH, DEFAULT_MAX_WIDTH, VoxelOptions};

/// How an entity is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RenderMode {
    /// One textured quad per entity, ordered by `z_index`. The default, and what
    /// every asset has always been drawn as.
    #[default]
    #[serde(rename = "2d", alias = "flat")]
    Flat,
    /// A voxel model extruded from the entity's current frame, depth-tested under
    /// a perspective camera.
    #[serde(rename = "3d", alias = "voxel")]
    Voxel,
}

impl RenderMode {
    pub fn is_voxel(self) -> bool {
        matches!(self, RenderMode::Voxel)
    }
}

/// Direction and strength of the single directional light.
///
/// One light, because the geometry is what gives a voxel pet its form; the
/// analytical multi-point model in `sprite_gen` is how a *flat* sprite gets one,
/// and running both would put two contradictory shading models on one asset.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Light {
    /// Direction the light travels, in world axes. Normalised on use.
    pub direction: [f32; 3],
    /// Floor brightness for a face the light does not reach, 0..1. A face in
    /// full shadow at 0 is pure black, which reads as a hole in the model.
    pub ambient: f32,
}

impl Default for Light {
    fn default() -> Self {
        Self {
            direction: [-0.4, 0.8, -0.45],
            ambient: 0.42,
        }
    }
}

/// Everything the renderer needs that is not the world itself.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RenderSettings {
    pub mode: RenderMode,
    pub fov_y_degrees: f32,
    /// Depth of one unit of `z`, as a fraction of the eye distance.
    pub depth_per_unit: f32,
    /// How far a model is turned off head-on, in degrees.
    ///
    /// Zero renders a voxel pet face-on, where it is indistinguishable from its
    /// own sprite: the depth is there and nothing reveals it. A small turn is
    /// what makes the model read as a model.
    pub yaw_degrees: f32,
    pub voxel_max_width: u32,
    pub voxel_depth: u32,
    pub light: Light,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            mode: RenderMode::Flat,
            fov_y_degrees: DEFAULT_FOV_Y_DEGREES,
            depth_per_unit: DEFAULT_DEPTH_PER_UNIT,
            yaw_degrees: 22.0,
            voxel_max_width: DEFAULT_MAX_WIDTH,
            voxel_depth: DEFAULT_DEPTH,
            light: Light::default(),
        }
    }
}

/// Bounds every field arrives with, because they arrive over IPC from a user's
/// configuration and an unbounded one is either a divide by zero or a mesh that
/// exhausts memory.
pub const MIN_FOV_Y_DEGREES: f32 = 10.0;
pub const MAX_FOV_Y_DEGREES: f32 = 120.0;
pub const MAX_VOXEL_MAX_WIDTH: u32 = 128;
pub const MAX_VOXEL_DEPTH: u32 = 64;
pub const MAX_DEPTH_PER_UNIT: f32 = 0.5;

impl RenderSettings {
    /// Clamps every field into its documented range.
    ///
    /// Called on the way in rather than at the point of use: a value that is
    /// wrong is wrong once, and clamping per frame would hide which field it was.
    pub fn sanitised(self) -> Self {
        Self {
            mode: self.mode,
            fov_y_degrees: self
                .fov_y_degrees
                .clamp(MIN_FOV_Y_DEGREES, MAX_FOV_Y_DEGREES),
            depth_per_unit: self.depth_per_unit.clamp(0.0, MAX_DEPTH_PER_UNIT),
            yaw_degrees: self.yaw_degrees % 360.0,
            voxel_max_width: self.voxel_max_width.clamp(1, MAX_VOXEL_MAX_WIDTH),
            voxel_depth: self.voxel_depth.clamp(1, MAX_VOXEL_DEPTH),
            light: Light {
                direction: self.light.direction,
                ambient: self.light.ambient.clamp(0.0, 1.0),
            },
        }
    }

    pub fn voxel_options(&self) -> VoxelOptions {
        VoxelOptions {
            max_width: self.voxel_max_width,
            depth: self.voxel_depth,
        }
    }

    /// The camera these settings describe, for a viewport of this size.
    ///
    /// Flat mode gets an orthographic camera, which reproduces the 2D pixel
    /// mapping exactly, so the depth buffer means the same thing in both modes.
    pub fn camera(&self, width: f32, height: f32) -> Camera {
        let mut camera = if self.mode.is_voxel() {
            Camera::perspective(width, height, self.fov_y_degrees)
        } else {
            Camera::orthographic(width, height)
        };
        camera.depth_per_unit = self.depth_per_unit;
        camera
    }

    /// The light direction as a unit vector, or straight down when the
    /// configured direction has no length to normalise.
    pub fn light_direction(&self) -> [f32; 3] {
        let [x, y, z] = self.light.direction;
        let length = (x * x + y * y + z * z).sqrt();
        if length < f32::EPSILON {
            return [0.0, 1.0, 0.0];
        }
        [x / length, y / length, z / length]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_is_the_default_mode() {
        assert_eq!(RenderSettings::default().mode, RenderMode::Flat);
        assert!(!RenderMode::default().is_voxel());
    }

    #[test]
    fn the_wire_names_are_the_ones_a_user_writes() {
        let voxel: RenderMode = serde_json::from_str("\"3d\"").expect("3d parses");
        let flat: RenderMode = serde_json::from_str("\"2d\"").expect("2d parses");
        assert_eq!(voxel, RenderMode::Voxel);
        assert_eq!(flat, RenderMode::Flat);
        assert_eq!(serde_json::to_string(&voxel).expect("encodes"), "\"3d\"");
    }

    #[test]
    fn every_field_is_clamped_into_its_range() {
        let wild = RenderSettings {
            mode: RenderMode::Voxel,
            fov_y_degrees: 5_000.0,
            depth_per_unit: 40.0,
            yaw_degrees: 20.0,
            voxel_max_width: 100_000,
            voxel_depth: 0,
            light: Light {
                direction: [0.0, 1.0, 0.0],
                ambient: 12.0,
            },
        }
        .sanitised();

        assert_eq!(wild.fov_y_degrees, MAX_FOV_Y_DEGREES);
        assert_eq!(wild.depth_per_unit, MAX_DEPTH_PER_UNIT);
        assert_eq!(wild.voxel_max_width, MAX_VOXEL_MAX_WIDTH);
        assert_eq!(wild.voxel_depth, 1, "a zero-depth slab has no geometry");
        assert_eq!(wild.light.ambient, 1.0);
    }

    #[test]
    fn flat_mode_asks_for_the_projection_that_matches_the_sprite_pass() {
        let camera = RenderSettings::default().camera(1600.0, 900.0);
        assert!(!camera.is_perspective());
    }

    #[test]
    fn voxel_mode_asks_for_a_perspective_camera_at_the_configured_field_of_view() {
        let settings = RenderSettings {
            mode: RenderMode::Voxel,
            fov_y_degrees: 60.0,
            ..Default::default()
        };
        let camera = settings.camera(1600.0, 900.0);
        assert!(camera.is_perspective());
        assert!((camera.eye_distance() - 450.0 / (30.0f32.to_radians()).tan()).abs() < 1e-3);
    }

    #[test]
    fn a_light_with_no_direction_falls_back_to_straight_down() {
        let settings = RenderSettings {
            light: Light {
                direction: [0.0, 0.0, 0.0],
                ambient: 0.4,
            },
            ..Default::default()
        };
        assert_eq!(settings.light_direction(), [0.0, 1.0, 0.0]);
    }

    #[test]
    fn the_light_direction_is_normalised() {
        let settings = RenderSettings {
            light: Light {
                direction: [0.0, 8.0, 0.0],
                ambient: 0.4,
            },
            ..Default::default()
        };
        assert_eq!(settings.light_direction(), [0.0, 1.0, 0.0]);
    }
}
