//! The drawing primitives every procedural asset is built from.
//!
//! Moved out of `sprite_gen.rs` when the shading model was split from the
//! drawing. The canvas is 1-based on both engines, which is the invariant these
//! pin: `Canvas.set` drops anything at x < 1 or y < 1, so a layout laid out from
//! row 0 renders a sprite whose bottom row is empty and which floats above the
//! floor it is anchored to.

use distract_engine::shading::{mix, shade};
use distract_engine::sprite_gen::{Canvas, cycle, sequence};
use image::Rgba;

#[test]
fn canvas_starts_transparent_and_records_writes() {
    let mut c = Canvas::new(8, 4);
    assert_eq!(c.get(1.0, 1.0), None);
    c.set(1.0, 1.0, [255, 0, 0]);
    assert_eq!(c.get(1.0, 1.0), Some([255, 0, 0]));
}

#[test]
fn line_covers_both_endpoints() {
    let mut c = Canvas::new(8, 8);
    c.line(2.0, 2.0, 6.0, 6.0, [1, 1, 1]);
    assert_eq!(c.get(2.0, 2.0), Some([1, 1, 1]));
    assert_eq!(c.get(6.0, 6.0), Some([1, 1, 1]));
}

#[test]
fn shade_moves_toward_black_and_white() {
    let base = [100, 100, 100];
    assert_eq!(shade(base, -1.0), [0, 0, 0]);
    assert_eq!(shade(base, 1.0), [255, 255, 255]);
}

#[test]
fn mix_interpolates_and_clamps_t() {
    assert_eq!(mix([0, 0, 0], [100, 100, 100], 0.5), [50, 50, 50]);
}

#[test]
fn cycle_never_repeats_the_first_pose_at_the_end() {
    let poses = cycle(4, |t| t);
    assert_eq!(poses.len(), 4);
    assert!(poses[3] < 1.0);
}

#[test]
fn sequence_runs_inclusive_of_one() {
    let poses = sequence(5, |t| t);
    assert_eq!(poses[0], 0.0);
    assert_eq!(poses[4], 1.0);
}

#[test]
fn to_image_keeps_transparent_cells_rather_than_dropping_them() {
    let mut c = Canvas::new(4, 4);
    c.set(2.0, 2.0, [255, 128, 64]);
    let img = c.to_image();
    assert_eq!(img.get_pixel(1, 1), &Rgba([255, 128, 64, 255]));
    assert_eq!(img.get_pixel(0, 0), &Rgba([0, 0, 0, 0]));
}
