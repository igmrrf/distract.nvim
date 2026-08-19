//! Camera and projection.
//!
//! World space is overlay pixels: x right, y down, origin top-left — the same
//! frame every placement, floor and obstacle is already expressed in. `z` is the
//! third axis, dimensionless, and **positive is toward the viewer**, because that
//! is what the existing depth cue already means: `position.parallax_factor`
//! returns `1 + z * per_unit`, so a positive `z` makes a sprite larger.
//!
//! The camera looks down `-Z` from `eye_distance()` in front of the viewport
//! centre. That distance is chosen so the plane `z = 0` maps exactly 1:1 to
//! pixels, which is what lets 3D be switched on without moving any entity that
//! never asked for depth. `agrees_with_orthographic_on_the_z0_plane` is that
//! property as a test.
//!
//! No wgpu type appears here, so every projection is testable without a GPU.

/// Column-major 4x4, matching WGSL's `mat4x4<f32>` memory layout: `cols[c][r]`.
pub type Mat4 = [[f32; 4]; 4];

/// Near plane, in pixels in front of the eye. Anything nearer is clipped.
pub const NEAR_PLANE_PX: f32 = 1.0;
/// How far behind the `z = 0` plane the far plane sits, in viewport heights.
const FAR_PLANE_VIEWPORT_HEIGHTS: f32 = 4.0;
/// Default vertical field of view, in degrees.
pub const DEFAULT_FOV_Y_DEGREES: f32 = 45.0;
/// Default depth of one `z` unit, as a fraction of the eye distance.
pub const DEFAULT_DEPTH_PER_UNIT: f32 = 0.05;
/// How close to the eye a `z` may bring an entity, as a fraction of the eye
/// distance. Depth is clamped to this rather than allowed to reach the eye,
/// where the projection divides by zero.
const MAX_DEPTH_FRACTION: f32 = 0.9;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Projection {
    /// The 2D mapping: pixels straight to clip space, no perspective divide.
    Orthographic,
    Perspective {
        fov_y_radians: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub projection: Projection,
    /// Viewport size in physical pixels.
    pub width: f32,
    pub height: f32,
    /// Pixels of depth per unit of `z`, as a fraction of the eye distance.
    pub depth_per_unit: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            projection: Projection::Orthographic,
            width: 1.0,
            height: 1.0,
            depth_per_unit: DEFAULT_DEPTH_PER_UNIT,
        }
    }
}

impl Camera {
    pub fn orthographic(width: f32, height: f32) -> Self {
        Self {
            projection: Projection::Orthographic,
            width: width.max(1.0),
            height: height.max(1.0),
            depth_per_unit: DEFAULT_DEPTH_PER_UNIT,
        }
    }

    pub fn perspective(width: f32, height: f32, fov_y_degrees: f32) -> Self {
        Self {
            projection: Projection::Perspective {
                fov_y_radians: fov_y_degrees.to_radians(),
            },
            width: width.max(1.0),
            height: height.max(1.0),
            depth_per_unit: DEFAULT_DEPTH_PER_UNIT,
        }
    }

    pub fn is_perspective(&self) -> bool {
        matches!(self.projection, Projection::Perspective { .. })
    }

    /// How far in front of the `z = 0` plane the eye sits, in pixels.
    ///
    /// Solving `tan(fov/2) = (height/2) / d` for `d` is what makes the plane
    /// exactly fill the frustum vertically, and so map 1:1 to pixels.
    pub fn eye_distance(&self) -> f32 {
        match self.projection {
            Projection::Orthographic => self.height * 0.5,
            Projection::Perspective { fov_y_radians } => {
                let half = (fov_y_radians * 0.5).clamp(0.01, 1.5);
                (self.height * 0.5) / half.tan()
            }
        }
    }

    pub fn far_plane(&self) -> f32 {
        self.eye_distance() + self.height * FAR_PLANE_VIEWPORT_HEIGHTS
    }

    /// A dimensionless `z` as a signed pixel offset along the view axis.
    pub fn depth_px(&self, z: f32) -> f32 {
        let eye = self.eye_distance();
        let limit = eye * MAX_DEPTH_FRACTION;
        (z * self.depth_per_unit * eye).clamp(-limit, limit)
    }

    /// How much larger the projection draws something at depth `z`.
    ///
    /// Exactly 1 at `z = 0` and under an orthographic projection, which is why
    /// switching modes cannot resize a pet that declared no depth.
    pub fn scale_at(&self, z: f32) -> f32 {
        if !self.is_perspective() {
            return 1.0;
        }
        let eye = self.eye_distance();
        eye / (eye - self.depth_px(z)).max(f32::EPSILON)
    }

    /// The view-projection matrix, ready for the uniform buffer.
    pub fn view_projection(&self) -> Mat4 {
        let eye = self.eye_distance();
        let near = NEAR_PLANE_PX;
        let far = self.far_plane().max(near + 1.0);
        let depth_span = far - near;

        let mut matrix = [[0.0f32; 4]; 4];
        // x: pixels to clip x. The viewport centre is the optical axis, so the
        // translation is exactly -1 for a camera centred on the window.
        matrix[0][0] = 2.0 / self.width;
        matrix[3][0] = -1.0;
        // y: pixels down to clip up.
        matrix[1][1] = -2.0 / self.height;
        matrix[3][1] = 1.0;

        match self.projection {
            Projection::Orthographic => {
                // View depth is `eye - z`, mapped linearly into 0..1 so the same
                // depth buffer works in either mode.
                matrix[2][2] = -1.0 / depth_span;
                matrix[3][2] = (eye - near) / depth_span;
                matrix[3][3] = 1.0;
            }
            Projection::Perspective { .. } => {
                let factor = far / depth_span;
                matrix[2][2] = -factor / eye;
                matrix[3][2] = factor * (eye - near) / eye;
                // The perspective divide: w is the view depth over the eye
                // distance, which is 1 on the `z = 0` plane.
                matrix[2][3] = -1.0 / eye;
                matrix[3][3] = 1.0;
            }
        }
        matrix
    }

    /// Projects a world point to normalised device coordinates.
    ///
    /// All three components are pixels: an entity's dimensionless `z` becomes the
    /// third through `depth_px` before it reaches a matrix, on the CPU and in the
    /// shader alike. `None` when the point is behind the eye, where the divide is
    /// meaningless.
    pub fn project(&self, point_px: [f32; 3]) -> Option<[f32; 3]> {
        let matrix = self.view_projection();
        let world = [point_px[0], point_px[1], point_px[2], 1.0];
        let mut clip = [0.0f32; 4];
        for (row, clip_component) in clip.iter_mut().enumerate() {
            *clip_component = (0..4)
                .map(|column| matrix[column][row] * world[column])
                .sum();
        }
        if clip[3] <= f32::EPSILON {
            return None;
        }
        Some([clip[0] / clip[3], clip[1] / clip[3], clip[2] / clip[3]])
    }

    pub fn to_uniform(&self) -> [f32; 16] {
        let matrix = self.view_projection();
        let mut flat = [0.0f32; 16];
        for column in 0..4 {
            for row in 0..4 {
                flat[column * 4 + row] = matrix[column][row];
            }
        }
        flat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: f32 = 1600.0;
    const HEIGHT: f32 = 900.0;

    fn close(left: f32, right: f32) -> bool {
        (left - right).abs() < 1e-4
    }

    #[test]
    fn the_orthographic_projection_reproduces_the_two_dimensional_pixel_mapping() {
        let camera = Camera::orthographic(WIDTH, HEIGHT);
        // The mapping the sprite shader has always used.
        for (x, y) in [(0.0, 0.0), (WIDTH, HEIGHT), (400.0, 250.0)] {
            let expected = (x / WIDTH * 2.0 - 1.0, 1.0 - y / HEIGHT * 2.0);
            let ndc = camera.project([x, y, 0.0]).expect("in front of the eye");
            assert!(close(ndc[0], expected.0), "x: {} vs {}", ndc[0], expected.0);
            assert!(close(ndc[1], expected.1), "y: {} vs {}", ndc[1], expected.1);
        }
    }

    #[test]
    fn agrees_with_orthographic_on_the_z0_plane() {
        let ortho = Camera::orthographic(WIDTH, HEIGHT);
        let perspective = Camera::perspective(WIDTH, HEIGHT, DEFAULT_FOV_Y_DEGREES);

        for (x, y) in [(0.0, 0.0), (WIDTH, HEIGHT), (37.0, 611.0), (800.0, 450.0)] {
            let flat = ortho.project([x, y, 0.0]).expect("in front of the eye");
            let deep = perspective
                .project([x, y, 0.0])
                .expect("in front of the eye");
            assert!(
                close(flat[0], deep[0]),
                "x diverged: {} vs {}",
                flat[0],
                deep[0]
            );
            assert!(
                close(flat[1], deep[1]),
                "y diverged: {} vs {}",
                flat[1],
                deep[1]
            );
        }
    }

    #[test]
    fn the_eye_distance_makes_the_viewport_exactly_fill_the_frustum() {
        let camera = Camera::perspective(WIDTH, HEIGHT, DEFAULT_FOV_Y_DEGREES);
        let half_fov = (DEFAULT_FOV_Y_DEGREES / 2.0).to_radians();
        assert!(close(
            camera.eye_distance(),
            (HEIGHT / 2.0) / half_fov.tan()
        ));
    }

    #[test]
    fn a_nearer_entity_projects_larger_and_a_further_one_smaller() {
        let camera = Camera::perspective(WIDTH, HEIGHT, DEFAULT_FOV_Y_DEGREES);
        assert!(close(camera.scale_at(0.0), 1.0));
        assert!(
            camera.scale_at(2.0) > 1.0,
            "positive z is toward the viewer"
        );
        assert!(camera.scale_at(-2.0) < 1.0);
    }

    #[test]
    fn depth_never_reaches_the_eye() {
        let camera = Camera::perspective(WIDTH, HEIGHT, DEFAULT_FOV_Y_DEGREES);
        let eye = camera.eye_distance();
        assert!(
            camera.depth_px(1_000.0) < eye,
            "a runaway z would divide by zero"
        );
        assert!(camera.scale_at(1_000.0).is_finite());
    }

    #[test]
    fn an_orthographic_projection_shows_no_depth_scaling_at_all() {
        let camera = Camera::orthographic(WIDTH, HEIGHT);
        assert!(close(camera.scale_at(3.0), 1.0));
        assert!(close(camera.scale_at(-3.0), 1.0));
    }

    #[test]
    fn the_depth_buffer_range_runs_from_the_near_plane_to_the_far_plane() {
        for camera in [
            Camera::orthographic(WIDTH, HEIGHT),
            Camera::perspective(WIDTH, HEIGHT, DEFAULT_FOV_Y_DEGREES),
        ] {
            let eye = camera.eye_distance();
            let at_near = camera
                .project([WIDTH / 2.0, HEIGHT / 2.0, eye - NEAR_PLANE_PX])
                .expect("on the near plane");
            let at_far = camera
                .project([WIDTH / 2.0, HEIGHT / 2.0, eye - camera.far_plane()])
                .expect("on the far plane");
            assert!(close(at_near[2], 0.0), "near depth was {}", at_near[2]);
            assert!(close(at_far[2], 1.0), "far depth was {}", at_far[2]);
        }
    }

    #[test]
    fn something_further_away_has_a_larger_depth_value() {
        let camera = Camera::perspective(WIDTH, HEIGHT, DEFAULT_FOV_Y_DEGREES);
        let near = camera.project([800.0, 450.0, 100.0]).expect("in front");
        let far = camera.project([800.0, 450.0, -100.0]).expect("in front");
        assert!(near[2] < far[2], "the depth test is less-than");
    }

    #[test]
    fn a_point_behind_the_eye_has_no_projection() {
        let camera = Camera::perspective(WIDTH, HEIGHT, DEFAULT_FOV_Y_DEGREES);
        let eye = camera.eye_distance();
        assert!(camera.project([0.0, 0.0, eye + 10.0]).is_none());
    }

    #[test]
    fn the_uniform_is_column_major() {
        let camera = Camera::orthographic(WIDTH, HEIGHT);
        let matrix = camera.view_projection();
        let flat = camera.to_uniform();
        assert_eq!(flat[0], matrix[0][0]);
        assert_eq!(flat[12], matrix[3][0], "the translation column comes last");
    }
}
