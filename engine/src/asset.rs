use std::collections::HashMap;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::path::Path;

use image::RgbaImage;

use crate::manifest::AssetManifest;
use crate::sprites;

/// Upper bounds on what a single asset may decode to.
///
/// Without these a user-supplied GIF is decoded in full, every frame, at source
/// resolution: the two samples in `assets/` are 1600x1200 and 1920x1080, so a
/// 60-frame animation at that size is hundreds of megabytes of `RgbaImage`
/// before anything else happens. Past the limit the load fails with a message
/// naming the limit rather than exhausting memory.
pub const MAX_FRAME_DIM: u32 = 1024;
pub const MAX_FRAMES: usize = 512;
pub const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

/// How large a GIF's own canvas may be before it is resampled to the frame size
/// its manifest declares.
///
/// A screen-sized animation is a legitimate source for a sprite that is drawn a
/// few dozen pixels across, so the source bound is looser than the decoded one
/// and matches `distract.gif`'s `MAX_CANVAS_DIM`. Without a declared frame size
/// the source *is* the frame, and `MAX_FRAME_DIM` still applies.
pub const MAX_SOURCE_DIM: u32 = 4096;

/// Holds loaded and sliced frames for an asset.
///
/// Frames are stored once, in their authored orientation. Mirroring happens at
/// blit time via [`crate::compositor::Compositor::blend_sprite`] and at draw
/// time in the GPU path, rather than keeping a second flipped copy of every
/// frame alive for the process lifetime.
#[derive(Debug, Clone)]
pub struct LoadedAsset {
    pub name: String,
    pub manifest: AssetManifest,
    pub frames: Vec<RgbaImage>,
    pub frame_w: u32,
    pub frame_h: u32,
    /// How long each frame is shown for, when the source file says so.
    ///
    /// Only GIFs carry timing of their own; everything else leaves this empty
    /// and is animated at whatever rate its manifest declares. `ecs.rs` applies
    /// the same precedence `lua/distract/engine.lua` does, so one manifest runs
    /// at one speed on both backends.
    pub frame_delays_ms: Vec<u32>,
    /// Hash of the manifest this asset was built from, so a repeated
    /// registration of the same manifest can be skipped.
    pub manifest_hash: u64,
}

pub struct AssetManager {
    assets: HashMap<String, LoadedAsset>,
    generation: u64,
}

impl AssetManager {
    pub fn new() -> Self {
        let mut mgr = Self {
            assets: HashMap::new(),
            generation: 0,
        };
        // Register default procedural assets. These are built from the shared
        // sprite generator and cannot fail.
        for manifest in [
            AssetManifest::default_cat(),
            AssetManifest::default_crab(),
            AssetManifest::default_sun(),
        ] {
            if let Err(err) = mgr.register_manifest(manifest) {
                // A built-in failing to load is a bug, not a user error.
                log::error!("built-in asset failed to load: {}", err);
            }
        }
        mgr
    }
}

impl Default for AssetManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Stable hash of a manifest.
///
/// Serialising through `serde_json::Value` first is deliberate: the manifest's
/// `HashMap` fields have a per-instance iteration order, so hashing the direct
/// JSON encoding would produce a different hash for two identical manifests.
/// `Value`'s object type is ordered, so the encoding is canonical.
fn manifest_hash(manifest: &AssetManifest) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    match serde_json::to_value(manifest) {
        Ok(value) => value.to_string().hash(&mut hasher),
        // An unserialisable manifest can never match a previous hash, which is
        // the safe direction: it reloads rather than reusing stale frames.
        Err(_) => (
            manifest.name.as_str(),
            std::ptr::addr_of!(*manifest) as usize,
        )
            .hash(&mut hasher),
    }
    hasher.finish()
}

impl AssetManager {
    pub fn get(&self, name: &str) -> Option<&LoadedAsset> {
        self.assets.get(name)
    }

    /// Every loaded asset, in unspecified order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &LoadedAsset)> {
        self.assets.iter()
    }

    /// Bumped whenever frames change. The GPU atlas rebuilds only when this
    /// moves, so a spawn that re-sends an unchanged manifest costs nothing.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Registers a manifest, loading its frames.
    ///
    /// Neovim resends the manifest on every spawn, so an unchanged manifest is
    /// a no-op: without this, spawning ten cats decodes the same spritesheet
    /// ten times. Returns `true` when frames were actually (re)loaded.
    pub fn register_manifest(&mut self, manifest: AssetManifest) -> Result<bool, String> {
        let name = manifest.name.clone();
        // Checked here rather than per frame: a manifest that cannot work is
        // worth one message when it arrives, not thirty a second forever.
        manifest.validate_capabilities()?;
        let hash = manifest_hash(&manifest);

        if let Some(existing) = self.assets.get(&name) {
            if existing.manifest_hash == hash {
                return Ok(false);
            }
        }

        let loaded = Self::load_asset(manifest, hash)?;
        self.assets.insert(name, loaded);
        self.generation += 1;
        Ok(true)
    }

    pub fn load_asset(manifest: AssetManifest, hash: u64) -> Result<LoadedAsset, String> {
        let name = manifest.name.clone();
        let mut frames = Vec::new();
        let mut frame_delays_ms = Vec::new();
        let mut frame_w = 32;
        let mut frame_h = 32;

        if let Some(ref path_str) = manifest.spritesheet.path {
            let path = Path::new(path_str);
            if path.exists() {
                let (loaded, w, h) = if path_str.to_lowercase().ends_with(".gif") {
                    let decoded = load_gif(path, declared_frame_size(&manifest))?;
                    frame_delays_ms = decoded.delays_ms;
                    (decoded.frames, decoded.width, decoded.height)
                } else {
                    load_spritesheet(path, &manifest)?
                };
                frames = loaded;
                frame_w = w;
                frame_h = h;
            } else if manifest.asset_type != "procedural" && !sprites::is_builtin(&name) {
                return Err(format!("Spritesheet file not found at path '{}'", path_str));
            }
        }

        // If no frames were loaded from file, draw the built-in procedural set.
        if frames.is_empty() {
            let set = sprites::get(&name);
            frames = set.frames.clone();
            frame_w = set.width;
            frame_h = set.height;
            frame_delays_ms.clear();
        }

        Ok(LoadedAsset {
            name,
            manifest,
            frames,
            frame_w,
            frame_h,
            frame_delays_ms,
            manifest_hash: hash,
        })
    }
}

fn check_dimensions(w: u32, h: u32, what: &str) -> Result<(), String> {
    if w == 0 || h == 0 {
        return Err(format!("{} has a zero dimension ({}x{})", what, w, h));
    }
    if w > MAX_FRAME_DIM || h > MAX_FRAME_DIM {
        return Err(format!(
            "{} is {}x{}, over the {}px per-side limit. Scale the asset down before using it.",
            what, w, h, MAX_FRAME_DIM
        ));
    }
    Ok(())
}

fn check_budget(frames: usize, w: u32, h: u32) -> Result<(), String> {
    if frames > MAX_FRAMES {
        return Err(format!(
            "asset has more than {} frames; refusing to decode the rest",
            MAX_FRAMES
        ));
    }
    let total = frames as u64 * w as u64 * h as u64 * 4;
    if total > MAX_TOTAL_BYTES {
        return Err(format!(
            "asset would decode to {} MiB, over the {} MiB limit",
            total / (1024 * 1024),
            MAX_TOTAL_BYTES / (1024 * 1024)
        ));
    }
    Ok(())
}

/// One decoded animation: its frames, their shared size, and their own timing.
pub struct DecodedGif {
    pub frames: Vec<RgbaImage>,
    pub width: u32,
    pub height: u32,
    /// One entry per frame, in milliseconds.
    pub delays_ms: Vec<u32>,
}

/// The frame size a manifest declares, when it declares both halves of one.
fn declared_frame_size(manifest: &AssetManifest) -> Option<(u32, u32)> {
    match (
        manifest.spritesheet.frame_width,
        manifest.spritesheet.frame_height,
    ) {
        (Some(width), Some(height)) => Some((width, height)),
        _ => None,
    }
}

/// Decodes a GIF frame by frame, resampled to the declared frame size.
///
/// Frames are pulled lazily and checked as they arrive, so an oversized
/// animation is rejected before the whole thing is in memory rather than after.
/// Frame sizes are also validated against each other: taking the dimensions
/// from whichever frame happened to decode last silently mis-sizes every
/// earlier frame.
///
/// A GIF authored at screen size is a legitimate source for a sprite drawn a
/// few dozen pixels across, so a declared frame size resamples the animation
/// here rather than leaving the overlay to draw a 1600-cell-wide cat. The
/// in-terminal decoder resamples to the same declared size, which is what keeps
/// one manifest describing one footprint on every backend.
///
/// # Errors
///
/// Fails when the file cannot be opened or decoded, when the source canvas is
/// over [`MAX_SOURCE_DIM`], when the resulting frame is over [`MAX_FRAME_DIM`],
/// when frames disagree about their size, or when the animation is empty.
fn load_gif(path: &Path, declared: Option<(u32, u32)>) -> Result<DecodedGif, String> {
    use image::AnimationDecoder;

    let file =
        File::open(path).map_err(|e| format!("Could not open '{}': {}", path.display(), e))?;
    let decoder = image::codecs::gif::GifDecoder::new(file)
        .map_err(|e| format!("Could not decode '{}' as a GIF: {}", path.display(), e))?;

    let mut frames: Vec<RgbaImage> = Vec::new();
    let mut delays_ms: Vec<u32> = Vec::new();
    let (mut frame_w, mut frame_h) = (0u32, 0u32);

    for (idx, frame) in decoder.into_frames().enumerate() {
        let frame = frame.map_err(|e| format!("GIF frame {} failed to decode: {}", idx, e))?;
        let (numer, denom) = frame.delay().numer_denom_ms();
        let img = resample_gif_frame(frame.into_buffer(), declared)?;
        let (w, h) = img.dimensions();

        if idx == 0 {
            check_dimensions(w, h, "GIF")?;
            frame_w = w;
            frame_h = h;
        } else if w != frame_w || h != frame_h {
            return Err(format!(
                "GIF frame {} is {}x{} but frame 0 is {}x{}; frames must agree",
                idx, w, h, frame_w, frame_h
            ));
        }

        frames.push(img);
        delays_ms.push(numer.checked_div(denom).unwrap_or(0));
        check_budget(frames.len(), frame_w, frame_h)?;
    }

    if frames.is_empty() {
        return Err(format!("GIF '{}' contains no frames", path.display()));
    }

    Ok(DecodedGif {
        frames,
        width: frame_w,
        height: frame_h,
        delays_ms,
    })
}

fn resample_gif_frame(image: RgbaImage, declared: Option<(u32, u32)>) -> Result<RgbaImage, String> {
    let (source_w, source_h) = image.dimensions();
    if source_w > MAX_SOURCE_DIM || source_h > MAX_SOURCE_DIM {
        return Err(format!(
            "GIF is {}x{}, over the {}px source limit",
            source_w, source_h, MAX_SOURCE_DIM
        ));
    }

    let Some((width, height)) = declared else {
        return Ok(image);
    };
    if width == source_w && height == source_h {
        return Ok(image);
    }
    check_dimensions(width, height, "declared GIF frame")?;

    Ok(image::imageops::resize(
        &image,
        width,
        height,
        image::imageops::FilterType::Triangle,
    ))
}

/// Slices a still image into a grid of frames.
fn load_spritesheet(
    path: &Path,
    manifest: &AssetManifest,
) -> Result<(Vec<RgbaImage>, u32, u32), String> {
    let img_dyn =
        image::open(path).map_err(|e| format!("Could not decode '{}': {}", path.display(), e))?;
    let rgba = img_dyn.to_rgba8();
    let (img_w, img_h) = rgba.dimensions();

    let sheet = &manifest.spritesheet;
    let margin_x = sheet.margin_x.unwrap_or(0);
    let margin_y = sheet.margin_y.unwrap_or(0);
    let spacing_x = sheet.spacing_x.unwrap_or(0);
    let spacing_y = sheet.spacing_y.unwrap_or(0);

    let cols = sheet.columns.unwrap_or_else(|| {
        if let Some(fw) = sheet.frame_width {
            ((img_w.saturating_sub(margin_x) + spacing_x) / (fw + spacing_x)).max(1)
        } else {
            4
        }
    });

    let rows = sheet.rows.unwrap_or_else(|| {
        if let Some(fh) = sheet.frame_height {
            ((img_h.saturating_sub(margin_y) + spacing_y) / (fh + spacing_y)).max(1)
        } else {
            1
        }
    });

    let fw = sheet.frame_width.unwrap_or_else(|| {
        (img_w.saturating_sub(margin_x + spacing_x * cols.saturating_sub(1)) / cols).max(1)
    });
    let fh = sheet.frame_height.unwrap_or_else(|| {
        (img_h.saturating_sub(margin_y + spacing_y * rows.saturating_sub(1)) / rows).max(1)
    });

    check_dimensions(fw, fh, "spritesheet frame")?;
    check_budget((cols as usize).saturating_mul(rows as usize), fw, fh)?;

    let mut frames = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let x = margin_x + c * (fw + spacing_x);
            let y = margin_y + r * (fh + spacing_y);
            if x + fw <= img_w && y + fh <= img_h {
                frames.push(image::imageops::crop_imm(&rgba, x, y, fw, fh).to_image());
            }
        }
    }

    if frames.is_empty() {
        return Err(format!(
            "spritesheet '{}' is {}x{}, too small for a {}x{} grid of {}x{} frames",
            path.display(),
            img_w,
            img_h,
            cols,
            rows,
            fw,
            fh
        ));
    }

    Ok((frames, fw, fh))
}

/// Helper to horizontally flip an RGBA image buffer.
///
/// Retained for tests and for callers that genuinely need a mirrored copy; the
/// render paths mirror by reading the source column in reverse instead.
pub fn flip_image_horizontal(img: &RgbaImage) -> RgbaImage {
    image::imageops::flip_horizontal(img)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    fn hash_of(m: &AssetManifest) -> u64 {
        manifest_hash(m)
    }

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
    fn identical_manifests_hash_identically() {
        // Two separately built manifests have independently seeded HashMaps, so
        // this is exactly the case a naive JSON hash would get wrong.
        let a = AssetManifest::default_cat();
        let b = AssetManifest::default_cat();
        assert_eq!(hash_of(&a), hash_of(&b));
    }

    #[test]
    fn changed_manifests_hash_differently() {
        let a = AssetManifest::default_cat();
        let mut b = AssetManifest::default_cat();
        b.initial_state = "sleep".to_string();
        assert_ne!(hash_of(&a), hash_of(&b));
    }

    #[test]
    fn re_registering_an_unchanged_manifest_is_a_no_op() {
        let mut mgr = AssetManager::new();
        // The built-in cat is already registered from the same manifest.
        assert_eq!(
            mgr.register_manifest(AssetManifest::default_cat()),
            Ok(false)
        );

        let mut changed = AssetManifest::default_cat();
        changed.initial_state = "sleep".to_string();
        assert_eq!(mgr.register_manifest(changed), Ok(true));
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
}
