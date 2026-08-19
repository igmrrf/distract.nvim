//! Voxel meshing contract.
//!
//! Out of `src/voxel.rs` for the same reason the IPC wire-format tests are out of
//! `src/ipc.rs`: the module plus its tests is over the file cap, and the tests
//! are the half that reads as a document.

use distract_engine::voxel::*;
use image::RgbaImage;

const OPAQUE: [u8; 4] = [200, 100, 50, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

fn frame_from(rows: &[&[[u8; 4]]]) -> RgbaImage {
    let height = rows.len() as u32;
    let width = rows[0].len() as u32;
    RgbaImage::from_fn(width, height, |col, row| {
        image::Rgba(rows[row as usize][col as usize])
    })
}

#[test]
fn a_lone_voxel_is_a_full_box() {
    let mesh = build(&frame_from(&[&[OPAQUE]]), VoxelOptions::default());
    assert_eq!(mesh.quad_count(), 6, "nothing hides any of its faces");
    assert_eq!(mesh.vertices.len(), 24);
    assert_eq!(mesh.extent, [1, 1, DEFAULT_DEPTH]);
}

#[test]
fn a_face_between_two_solid_voxels_is_never_emitted() {
    let mesh = build(&frame_from(&[&[OPAQUE, OPAQUE]]), VoxelOptions::default());
    // Six faces each, less the two that meet.
    assert_eq!(mesh.quad_count(), 10);
}

#[test]
fn a_transparent_frame_produces_no_geometry() {
    let mesh = build(&frame_from(&[&[CLEAR, CLEAR]]), VoxelOptions::default());
    assert!(mesh.is_empty());
    assert_eq!(mesh.quad_count(), 0);
}

#[test]
fn a_pixel_below_the_alpha_threshold_is_not_solid() {
    let barely_there = [10, 10, 10, OPAQUE_ALPHA_THRESHOLD - 1];
    let mesh = build(&frame_from(&[&[barely_there]]), VoxelOptions::default());
    assert!(mesh.is_empty());
}

#[test]
fn a_voxel_carries_the_source_pixel_colour_unchanged() {
    let mesh = build(&frame_from(&[&[OPAQUE]]), VoxelOptions::default());
    for vertex in &mesh.vertices {
        assert_eq!(vertex.colour, [OPAQUE[0], OPAQUE[1], OPAQUE[2], u8::MAX]);
    }
}

#[test]
fn the_model_is_centred_on_x_and_z_and_hangs_from_its_top() {
    let mesh = build(
        &frame_from(&[&[OPAQUE, OPAQUE]]),
        VoxelOptions {
            max_width: 8,
            depth: 2,
        },
    );
    let xs: Vec<f32> = mesh
        .vertices
        .iter()
        .map(|vertex| vertex.position[0])
        .collect();
    let ys: Vec<f32> = mesh
        .vertices
        .iter()
        .map(|vertex| vertex.position[1])
        .collect();
    let zs: Vec<f32> = mesh
        .vertices
        .iter()
        .map(|vertex| vertex.position[2])
        .collect();

    assert_eq!(xs.iter().cloned().fold(f32::MAX, f32::min), -1.0);
    assert_eq!(xs.iter().cloned().fold(f32::MIN, f32::max), 1.0);
    assert_eq!(
        ys.iter().cloned().fold(f32::MAX, f32::min),
        0.0,
        "the top is y = 0"
    );
    assert_eq!(ys.iter().cloned().fold(f32::MIN, f32::max), 1.0);
    assert_eq!(zs.iter().cloned().fold(f32::MAX, f32::min), -1.0);
    assert_eq!(zs.iter().cloned().fold(f32::MIN, f32::max), 1.0);
}

#[test]
fn every_face_normal_is_one_unit_on_exactly_one_axis() {
    let mesh = build(&frame_from(&[&[OPAQUE]]), VoxelOptions::default());
    for vertex in &mesh.vertices {
        let nonzero = vertex.normal[0..3]
            .iter()
            .filter(|axis| **axis != 0)
            .count();
        assert_eq!(nonzero, 1, "normal was {:?}", vertex.normal);
    }
}

#[test]
fn a_frame_wider_than_the_cap_is_resampled_and_keeps_its_aspect() {
    let wide = RgbaImage::from_fn(192, 96, |_, _| image::Rgba(OPAQUE));
    let mesh = build(
        &wide,
        VoxelOptions {
            max_width: 48,
            depth: 4,
        },
    );
    assert_eq!(mesh.extent, [48, 24, 4]);
}

#[test]
fn art_already_narrow_enough_is_extruded_at_its_own_size() {
    let narrow = RgbaImage::from_fn(24, 16, |_, _| image::Rgba(OPAQUE));
    let mesh = build(&narrow, VoxelOptions::default());
    assert_eq!(mesh.extent, [24, 16, DEFAULT_DEPTH]);
}

#[test]
fn resampling_keeps_a_shape_rather_than_averaging_it_away() {
    let mut checker = RgbaImage::from_fn(96, 4, |_, _| image::Rgba(CLEAR));
    for col in 0..48 {
        for row in 0..4 {
            checker.put_pixel(col, row, image::Rgba(OPAQUE));
        }
    }
    let grid = VoxelGrid::fit(&checker, 48);
    assert!(grid.is_opaque(0, 0), "the solid half survives");
    assert!(!grid.is_opaque(47, 0), "and so does the empty half");
}

#[test]
fn a_solid_slab_costs_two_quads_a_pixel_plus_its_silhouette() {
    let solid = RgbaImage::from_fn(8, 8, |_, _| image::Rgba(OPAQUE));
    let mesh = build(&solid, VoxelOptions::default());
    // 64 fronts, 64 backs, and one side face per edge pixel.
    assert_eq!(mesh.quad_count(), 64 * 2 + 8 * 4);
}
