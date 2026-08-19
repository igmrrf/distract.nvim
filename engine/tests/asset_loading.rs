//! What an asset decodes to, and what the registry does with it.
//!
//! Moved out of `asset.rs` when the decoding was split from the registry: these
//! exercise both sides and belong to neither. Every bound asserted here is a
//! denial-of-service guard -- a user-supplied GIF is untrusted input, and without
//! the limits a 1920x1080 sixty-frame animation decodes to hundreds of megabytes
//! before anything else runs.

use distract_engine::asset::AssetManager;
use distract_engine::asset_decode::{
    MAX_FRAME_DIM, MAX_FRAMES, MAX_SOURCE_DIM, check_budget, check_dimensions,
    flip_image_horizontal, resample_gif_frame,
};
use distract_engine::manifest::AssetManifest;
use distract_engine::sprites;
use image::{ImageBuffer, Rgba, RgbaImage};
use std::fs::File;
use std::path::Path;

#[test]
fn test_asset_manager_init() {
    let mgr = AssetManager::new();
    assert!(mgr.get("cat").is_some());
    assert!(mgr.get("crab").is_some());
    assert!(mgr.get("sun").is_some());
    assert!(mgr.get("nonexistent").is_none());
}

#[test]
fn builtins_use_the_shared_procedural_sprite_set() {
    let mgr = AssetManager::new();

    let cat = mgr.get("cat").unwrap();
    assert_eq!(cat.frames.len(), sprites::cat_set().frames.len());
    assert_eq!((cat.frame_w, cat.frame_h), (24, 16));

    let crab = mgr.get("crab").unwrap();
    assert_eq!(crab.frames.len(), sprites::crab_set().frames.len());

    let sun = mgr.get("sun").unwrap();
    assert_eq!((sun.frame_w, sun.frame_h), (16, 16));
}

#[test]
fn every_manifest_frame_index_resolves_to_real_art() {
    // The overlay used to have four frames against manifests referencing up
    // to 28, so states wrapped onto each other's pictures.
    let mgr = AssetManager::new();
    for name in ["cat", "crab", "sun"] {
        let asset = mgr.get(name).unwrap();
        for (state, def) in &asset.manifest.states {
            for &idx in &def.animation.frames {
                assert!(
                    idx < asset.frames.len(),
                    "{}/{} references frame {} of {}",
                    name,
                    state,
                    idx,
                    asset.frames.len()
                );
            }
        }
    }
}

#[test]
fn test_horizontal_flip() {
    let mut img = ImageBuffer::new(2, 2);
    let red = Rgba([255, 0, 0, 255]);
    let blue = Rgba([0, 0, 255, 255]);
    img.put_pixel(0, 0, red);
    img.put_pixel(1, 0, blue);

    let flipped = flip_image_horizontal(&img);
    assert_eq!(*flipped.get_pixel(0, 0), blue);
    assert_eq!(*flipped.get_pixel(1, 0), red);
}

#[test]
fn test_custom_manifest_registration() {
    let mut mgr = AssetManager::new();
    let mut custom = AssetManifest::default_cat();
    custom.name = "robot_cat".to_string();
    assert_eq!(mgr.register_manifest(custom), Ok(true));
    assert!(mgr.get("robot_cat").is_some());
}

#[test]
fn a_missing_spritesheet_for_a_custom_asset_is_an_error() {
    let mut manifest = AssetManifest::default_cat();
    manifest.name = "ghost".to_string();
    manifest.asset_type = "sprite".to_string();
    manifest.spritesheet.path = Some("/definitely/not/here.png".to_string());

    let err = AssetManager::load_asset(manifest, 0).unwrap_err();
    assert!(err.contains("not found"), "unexpected message: {}", err);
}

#[test]
fn oversized_frames_are_rejected_with_the_limit_named() {
    let err = check_dimensions(MAX_FRAME_DIM + 1, 10, "spritesheet frame").unwrap_err();
    assert!(err.contains(&MAX_FRAME_DIM.to_string()));
}

#[test]
fn zero_sized_frames_are_rejected() {
    assert!(check_dimensions(0, 10, "GIF").is_err());
    assert!(check_dimensions(10, 0, "GIF").is_err());
}

#[test]
fn frame_count_and_byte_budget_are_both_enforced() {
    assert!(check_budget(MAX_FRAMES + 1, 8, 8).is_err());
    // 512 frames of 1024x1024 RGBA is 2 GiB, well past the byte budget.
    assert!(check_budget(MAX_FRAMES, MAX_FRAME_DIM, MAX_FRAME_DIM).is_err());
    assert!(check_budget(8, 32, 32).is_ok());
}

#[test]
fn a_real_spritesheet_slices_into_the_declared_grid() {
    let dir = std::env::temp_dir().join("distract_asset_tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("sheet_4x1.png");

    // 4 frames of 8x8 laid out in a row, each a distinct solid colour.
    let mut sheet: RgbaImage = ImageBuffer::new(32, 8);
    for (i, tone) in [40u8, 90, 140, 190].iter().enumerate() {
        for y in 0..8 {
            for x in 0..8 {
                sheet.put_pixel(i as u32 * 8 + x, y, Rgba([*tone, *tone, *tone, 255]));
            }
        }
    }
    sheet.save(&path).unwrap();

    let mut manifest = AssetManifest::default_cat();
    manifest.name = "sheet_test".to_string();
    manifest.spritesheet.path = Some(path.to_string_lossy().to_string());
    manifest.spritesheet.frame_width = Some(8);
    manifest.spritesheet.frame_height = Some(8);
    manifest.spritesheet.columns = Some(4);
    manifest.spritesheet.rows = Some(1);

    let loaded = AssetManager::load_asset(manifest, 0).unwrap();
    assert_eq!(loaded.frames.len(), 4);
    assert_eq!((loaded.frame_w, loaded.frame_h), (8, 8));
    // Frames must come out in sheet order, not all be the same crop.
    assert_eq!(loaded.frames[0].get_pixel(0, 0)[0], 40);
    assert_eq!(loaded.frames[3].get_pixel(0, 0)[0], 190);

    let _ = std::fs::remove_file(&path);
}

/// Writes an `frames`-frame GIF of solid colours, each shown for `delay_ms`.
fn write_test_gif(path: &Path, size: (u32, u32), tones: &[u8]) {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame};

    let file = File::create(path).unwrap();
    let mut encoder = GifEncoder::new(file);
    encoder.set_repeat(Repeat::Infinite).unwrap();

    for tone in tones {
        let mut buffer: RgbaImage = ImageBuffer::new(size.0, size.1);
        for pixel in buffer.pixels_mut() {
            *pixel = Rgba([*tone, *tone, *tone, 255]);
        }
        encoder
            .encode_frame(Frame::from_parts(
                buffer,
                0,
                0,
                Delay::from_numer_denom_ms(80, 1),
            ))
            .unwrap();
    }
}

fn gif_manifest(path: &Path, name: &str) -> AssetManifest {
    let mut manifest = AssetManifest::default_cat();
    manifest.name = name.to_string();
    manifest.spritesheet.path = Some(path.to_string_lossy().to_string());
    manifest.spritesheet.columns = None;
    manifest.spritesheet.rows = None;
    manifest.spritesheet.frame_width = None;
    manifest.spritesheet.frame_height = None;
    manifest
}

#[test]
fn a_gif_carries_the_delays_its_file_declares() {
    let dir = std::env::temp_dir().join("distract_asset_tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("delays.gif");
    write_test_gif(&path, (8, 8), &[10, 200]);

    let loaded = AssetManager::load_asset(gif_manifest(&path, "gif_delays"), 0).unwrap();

    assert_eq!(loaded.frames.len(), 2);
    assert_eq!(loaded.frame_delays_ms, vec![80, 80]);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_declared_frame_size_resamples_the_gif_to_it() {
    let dir = std::env::temp_dir().join("distract_asset_tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("resampled.gif");
    write_test_gif(&path, (64, 32), &[128]);

    let mut manifest = gif_manifest(&path, "gif_resampled");
    manifest.spritesheet.frame_width = Some(16);
    manifest.spritesheet.frame_height = Some(8);

    let loaded = AssetManager::load_asset(manifest, 0).unwrap();

    assert_eq!((loaded.frame_w, loaded.frame_h), (16, 8));
    assert_eq!(loaded.frames[0].dimensions(), (16, 8));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn procedural_art_reports_no_source_timing() {
    let loaded = AssetManager::load_asset(AssetManifest::default_cat(), 0).unwrap();
    assert!(loaded.frame_delays_ms.is_empty());
}

#[test]
fn a_gif_over_the_source_limit_is_refused_before_it_is_resampled() {
    let oversized: RgbaImage = ImageBuffer::new(MAX_SOURCE_DIM + 1, 1);
    let err = resample_gif_frame(oversized, Some((16, 16))).unwrap_err();
    assert!(err.contains(&MAX_SOURCE_DIM.to_string()));
}

#[test]
fn an_undeclared_gif_frame_still_answers_to_the_frame_limit() {
    let oversized: RgbaImage = ImageBuffer::new(MAX_FRAME_DIM + 1, 8);
    let passed_through = resample_gif_frame(oversized, None).unwrap();
    assert!(check_dimensions(passed_through.width(), passed_through.height(), "GIF").is_err());
}

#[test]
fn an_oversized_spritesheet_frame_is_refused_rather_than_decoded() {
    let dir = std::env::temp_dir().join("distract_asset_tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("huge_frame.png");

    // One frame far wider than the per-side limit.
    let sheet: RgbaImage = ImageBuffer::new(64, 8);
    sheet.save(&path).unwrap();

    let mut manifest = AssetManifest::default_cat();
    manifest.name = "huge".to_string();
    manifest.spritesheet.path = Some(path.to_string_lossy().to_string());
    manifest.spritesheet.frame_width = Some(MAX_FRAME_DIM + 1);
    manifest.spritesheet.frame_height = Some(8);
    manifest.spritesheet.columns = Some(1);
    manifest.spritesheet.rows = Some(1);

    let err = AssetManager::load_asset(manifest, 0).unwrap_err();
    assert!(err.contains("limit"), "unexpected message: {}", err);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_corrupt_image_file_reports_rather_than_falling_back_silently() {
    let dir = std::env::temp_dir().join("distract_asset_tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("not_an_image.png");
    std::fs::write(&path, b"this is not a PNG").unwrap();

    let mut manifest = AssetManifest::default_cat();
    manifest.name = "broken".to_string();
    manifest.spritesheet.path = Some(path.to_string_lossy().to_string());

    let err = AssetManager::load_asset(manifest, 0).unwrap_err();
    assert!(err.contains("decode"), "unexpected message: {}", err);

    let _ = std::fs::remove_file(&path);
}
