//! The positional path primitives a state may follow on top of its velocity.
//!
//! Split from `ecs.rs` because a path is a pure function of the entity's anchor
//! and its phase: given the same manifest and the same phase it returns the same
//! offset, with no reference to the world, the clock or the viewport. The
//! physics-parity fixtures cover every primitive here.

use crate::entity::Entity;
use crate::manifest::PhysicsConfig;

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
pub(crate) fn apply_path(
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
