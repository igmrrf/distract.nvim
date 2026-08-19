//! Turning a file into frames: GIF decoding, spritesheet slicing, and the
//! bounds that stop a screen-sized source from exhausting memory.
//!
//! Split from `asset.rs`, which owned both the registry of loaded assets and the
//! decoding of every format they can come from. The registry answers "what art
//! does this name have"; this answers "what does this file decode to", and only
//! the latter has to know about GIF frame disposal, declared grids and
//! resampling.

use std::fs::File;
use std::path::Path;

use image::RgbaImage;

use crate::manifest::AssetManifest;

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

pub fn check_dimensions(w: u32, h: u32, what: &str) -> Result<(), String> {
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

pub fn check_budget(frames: usize, w: u32, h: u32) -> Result<(), String> {
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
pub fn declared_frame_size(manifest: &AssetManifest) -> Option<(u32, u32)> {
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
pub fn load_gif(path: &Path, declared: Option<(u32, u32)>) -> Result<DecodedGif, String> {
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

pub fn resample_gif_frame(
    image: RgbaImage,
    declared: Option<(u32, u32)>,
) -> Result<RgbaImage, String> {
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
pub fn load_spritesheet(
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
