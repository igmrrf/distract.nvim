use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use image::{ImageBuffer, Rgba, RgbaImage};
use crate::manifest::AssetManifest;

/// Holds loaded and sliced frames for an asset.
#[derive(Debug, Clone)]
pub struct LoadedAsset {
    pub name: String,
    pub manifest: AssetManifest,
    pub frames: Vec<RgbaImage>,
    pub flipped_frames: Vec<RgbaImage>,
    pub frame_w: u32,
    pub frame_h: u32,
}

pub struct AssetManager {
    assets: HashMap<String, LoadedAsset>,
}

impl AssetManager {
    pub fn new() -> Self {
        let mut mgr = Self {
            assets: HashMap::new(),
        };
        // Register default procedural assets
        let _ = mgr.register_manifest(AssetManifest::default_cat());
        let _ = mgr.register_manifest(AssetManifest::default_crab());
        let _ = mgr.register_manifest(AssetManifest::default_sun());
        mgr
    }
}

impl Default for AssetManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetManager {
    pub fn get(&self, name: &str) -> Option<&LoadedAsset> {
        self.assets.get(name)
    }

    pub fn register_manifest(&mut self, manifest: AssetManifest) -> Result<(), String> {
        let name = manifest.name.clone();
        let loaded = Self::load_asset(manifest)?;
        self.assets.insert(name, loaded);
        Ok(())
    }

    pub fn load_asset(manifest: AssetManifest) -> Result<LoadedAsset, String> {
        let name = manifest.name.clone();
        let mut frames = Vec::new();
        let mut frame_w = 32;
        let mut frame_h = 32;

        if let Some(ref path_str) = manifest.spritesheet.path {
            let path = Path::new(path_str);
            if path.exists() {
                if path_str.to_lowercase().ends_with(".gif") {
                    if let Ok(file) = File::open(path) {
                        use image::AnimationDecoder;
                        if let Ok(decoder) = image::codecs::gif::GifDecoder::new(file) {
                            if let Ok(gif_frames) = decoder.into_frames().collect_frames() {
                                for frame in gif_frames {
                                    let img = frame.buffer().clone();
                                    frame_w = img.width();
                                    frame_h = img.height();
                                    frames.push(img);
                                }
                            }
                        }
                    }
                } else if let Ok(img_dyn) = image::open(path) {
                    let rgba = img_dyn.to_rgba8();
                    let (img_w, img_h) = rgba.dimensions();

                    let margin_x = manifest.spritesheet.margin_x.unwrap_or(0);
                    let margin_y = manifest.spritesheet.margin_y.unwrap_or(0);
                    let spacing_x = manifest.spritesheet.spacing_x.unwrap_or(0);
                    let spacing_y = manifest.spritesheet.spacing_y.unwrap_or(0);

                    let cols = manifest.spritesheet.columns.unwrap_or_else(|| {
                        if let Some(fw) = manifest.spritesheet.frame_width {
                            ((img_w.saturating_sub(margin_x) + spacing_x) / (fw + spacing_x)).max(1)
                        } else {
                            4
                        }
                    });

                    let rows = manifest.spritesheet.rows.unwrap_or_else(|| {
                        if let Some(fh) = manifest.spritesheet.frame_height {
                            ((img_h.saturating_sub(margin_y) + spacing_y) / (fh + spacing_y)).max(1)
                        } else {
                            1
                        }
                    });

                    let fw = manifest.spritesheet.frame_width.unwrap_or_else(|| {
                        (img_w.saturating_sub(margin_x + spacing_x * cols.saturating_sub(1)) / cols).max(1)
                    });
                    let fh = manifest.spritesheet.frame_height.unwrap_or_else(|| {
                        (img_h.saturating_sub(margin_y + spacing_y * rows.saturating_sub(1)) / rows).max(1)
                    });

                    frame_w = fw;
                    frame_h = fh;

                    for r in 0..rows {
                        for c in 0..cols {
                            let x = margin_x + c * (fw + spacing_x);
                            let y = margin_y + r * (fh + spacing_y);
                            if x + fw <= img_w && y + fh <= img_h {
                                let sub_img = image::imageops::crop_imm(&rgba, x, y, fw, fh).to_image();
                                frames.push(sub_img);
                            }
                        }
                    }
                }
            } else if manifest.asset_type != "procedural" && name != "cat" && name != "crab" && name != "sun" {
                return Err(format!("Spritesheet file not found at path '{}'", path_str));
            }
        }

        // If no frames were loaded from file, generate procedural frames
        if frames.is_empty() {
            let (gen_frames, w, h) = Self::generate_procedural(&name);
            frames = gen_frames;
            frame_w = w;
            frame_h = h;
        }


        // Generate horizontally flipped copies for all frames
        let flipped_frames: Vec<RgbaImage> = frames.iter().map(flip_image_horizontal).collect();

        Ok(LoadedAsset {
            name,
            manifest,
            frames,
            flipped_frames,
            frame_w,
            frame_h,
        })
    }

    /// Generates high quality pixel-art procedural sprites for built-in assets.
    fn generate_procedural(name: &str) -> (Vec<RgbaImage>, u32, u32) {
        match name {
            "crab" => generate_procedural_crab(),
            "sun" => generate_procedural_sun(),
            _ => generate_procedural_cat(),
        }
    }
}

/// Helper to horizontally flip an RGBA image buffer.
pub fn flip_image_horizontal(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut flipped = ImageBuffer::new(w, h);
    for y in 0..h {
        for x in 0..w {
            flipped.put_pixel(w - 1 - x, y, *img.get_pixel(x, y));
        }
    }
    flipped
}

/// Generates procedural 4-frame cat sprite (Idle, Walk 1, Walk 2, Sleep).
fn generate_procedural_cat() -> (Vec<RgbaImage>, u32, u32) {
    let w = 32;
    let h = 32;
    let mut frames = Vec::new();

    let orange = Rgba([245, 140, 40, 255]);
    let dark_orange = Rgba([200, 100, 20, 255]);
    let white = Rgba([255, 255, 255, 255]);
    let pink = Rgba([255, 160, 180, 255]);
    let eye_color = Rgba([40, 40, 40, 255]);

    // Frame 0: Idle
    let mut f0 = ImageBuffer::new(w, h);
    draw_rect(&mut f0, 8, 14, 16, 10, orange); // Body
    draw_rect(&mut f0, 18, 10, 10, 8, orange); // Head
    draw_rect(&mut f0, 20, 7, 3, 3, dark_orange); // Left ear
    draw_rect(&mut f0, 25, 7, 3, 3, dark_orange); // Right ear
    draw_rect(&mut f0, 24, 12, 2, 2, eye_color); // Eye
    draw_rect(&mut f0, 27, 14, 1, 1, pink); // Nose
    draw_rect(&mut f0, 6, 12, 3, 6, dark_orange); // Tail up
    draw_rect(&mut f0, 10, 24, 3, 5, orange); // Front paw
    draw_rect(&mut f0, 19, 24, 3, 5, orange); // Back paw
    draw_rect(&mut f0, 10, 27, 3, 2, white);
    draw_rect(&mut f0, 19, 27, 3, 2, white);
    frames.push(f0);

    // Frame 1: Walk 1 (Legs forward/back)
    let mut f1 = ImageBuffer::new(w, h);
    draw_rect(&mut f1, 8, 13, 16, 10, orange);
    draw_rect(&mut f1, 19, 9, 10, 8, orange);
    draw_rect(&mut f1, 21, 6, 3, 3, dark_orange);
    draw_rect(&mut f1, 26, 6, 3, 3, dark_orange);
    draw_rect(&mut f1, 25, 11, 2, 2, eye_color);
    draw_rect(&mut f1, 28, 13, 1, 1, pink);
    draw_rect(&mut f1, 5, 14, 4, 4, dark_orange); // Tail wag
    draw_rect(&mut f1, 8, 23, 3, 6, orange); // Leg extended back
    draw_rect(&mut f1, 21, 23, 3, 6, orange); // Leg extended forward
    draw_rect(&mut f1, 8, 27, 3, 2, white);
    draw_rect(&mut f1, 21, 27, 3, 2, white);
    frames.push(f1);

    // Frame 2: Walk 2 (Legs alternate)
    let mut f2 = ImageBuffer::new(w, h);
    draw_rect(&mut f2, 8, 14, 16, 10, orange);
    draw_rect(&mut f2, 18, 10, 10, 8, orange);
    draw_rect(&mut f2, 20, 7, 3, 3, dark_orange);
    draw_rect(&mut f2, 25, 7, 3, 3, dark_orange);
    draw_rect(&mut f2, 24, 12, 2, 2, eye_color);
    draw_rect(&mut f2, 27, 14, 1, 1, pink);
    draw_rect(&mut f2, 6, 11, 3, 6, dark_orange);
    draw_rect(&mut f2, 13, 23, 3, 6, orange); // Legs together/crossing
    draw_rect(&mut f2, 16, 23, 3, 6, orange);
    draw_rect(&mut f2, 13, 27, 3, 2, white);
    draw_rect(&mut f2, 16, 27, 3, 2, white);
    frames.push(f2);

    // Frame 3: Sleep / Laying down
    let mut f3 = ImageBuffer::new(w, h);
    draw_rect(&mut f3, 6, 18, 18, 8, orange); // Laying body
    draw_rect(&mut f3, 20, 17, 8, 7, orange); // Head resting
    draw_rect(&mut f3, 21, 15, 2, 2, dark_orange);
    draw_rect(&mut f3, 26, 15, 2, 2, dark_orange);
    draw_rect(&mut f3, 24, 20, 3, 1, eye_color); // Closed eye line (sleeping)
    draw_rect(&mut f3, 4, 19, 3, 5, dark_orange); // Curled tail
    // Zzz
    draw_rect(&mut f3, 18, 7, 4, 1, white);
    draw_rect(&mut f3, 20, 8, 2, 1, white);
    draw_rect(&mut f3, 18, 9, 4, 1, white);
    frames.push(f3);

    (frames, w, h)
}

/// Generates procedural 4-frame crab sprite (Stand, Walk, Claws open, Claws closed/clip).
fn generate_procedural_crab() -> (Vec<RgbaImage>, u32, u32) {
    let w = 32;
    let h = 32;
    let mut frames = Vec::new();

    let red = Rgba([230, 50, 40, 255]);
    let dark_red = Rgba([180, 30, 25, 255]);
    let claw_color = Rgba([250, 100, 60, 255]);
    let eye_white = Rgba([255, 255, 255, 255]);
    let eye_black = Rgba([20, 20, 20, 255]);

    // Frame 0: Stand
    let mut f0 = ImageBuffer::new(w, h);
    draw_rect(&mut f0, 8, 14, 16, 10, red); // Shell
    draw_rect(&mut f0, 10, 16, 12, 6, dark_red);
    // Eyestalks
    draw_rect(&mut f0, 11, 9, 2, 5, red);
    draw_rect(&mut f0, 19, 9, 2, 5, red);
    draw_rect(&mut f0, 10, 8, 4, 3, eye_white);
    draw_rect(&mut f0, 18, 8, 4, 3, eye_white);
    draw_rect(&mut f0, 12, 9, 1, 1, eye_black);
    draw_rect(&mut f0, 20, 9, 1, 1, eye_black);
    // Left Claw
    draw_rect(&mut f0, 2, 11, 5, 5, claw_color);
    draw_rect(&mut f0, 6, 13, 3, 3, red);
    // Right Claw
    draw_rect(&mut f0, 25, 11, 5, 5, claw_color);
    draw_rect(&mut f0, 23, 13, 3, 3, red);
    // Legs
    draw_rect(&mut f0, 6, 24, 2, 4, dark_red);
    draw_rect(&mut f0, 10, 24, 2, 4, dark_red);
    draw_rect(&mut f0, 20, 24, 2, 4, dark_red);
    draw_rect(&mut f0, 24, 24, 2, 4, dark_red);
    frames.push(f0.clone());

    // Frame 1: Walk sideways (Legs shifted)
    let mut f1 = ImageBuffer::new(w, h);
    draw_rect(&mut f1, 8, 13, 16, 10, red);
    draw_rect(&mut f1, 10, 15, 12, 6, dark_red);
    draw_rect(&mut f1, 11, 8, 2, 5, red);
    draw_rect(&mut f1, 19, 8, 2, 5, red);
    draw_rect(&mut f1, 10, 7, 4, 3, eye_white);
    draw_rect(&mut f1, 18, 7, 4, 3, eye_white);
    draw_rect(&mut f1, 12, 8, 1, 1, eye_black);
    draw_rect(&mut f1, 20, 8, 1, 1, eye_black);
    draw_rect(&mut f1, 2, 10, 5, 5, claw_color);
    draw_rect(&mut f1, 25, 10, 5, 5, claw_color);
    // Legs angled sideways
    draw_rect(&mut f1, 4, 23, 2, 5, dark_red);
    draw_rect(&mut f1, 12, 23, 2, 5, dark_red);
    draw_rect(&mut f1, 18, 23, 2, 5, dark_red);
    draw_rect(&mut f1, 26, 23, 2, 5, dark_red);
    frames.push(f1);

    // Frame 2: Claw Clip Open
    let mut f2 = f0.clone();
    draw_rect(&mut f2, 1, 9, 6, 3, claw_color); // Open upper claw
    draw_rect(&mut f2, 1, 14, 6, 3, claw_color); // Open lower claw
    draw_rect(&mut f2, 25, 9, 6, 3, claw_color);
    draw_rect(&mut f2, 25, 14, 6, 3, claw_color);
    frames.push(f2);

    // Frame 3: Claw Clip Closed / Snapped
    let mut f3 = f0.clone();
    draw_rect(&mut f3, 1, 11, 7, 4, dark_red); // Snapped pincers shut
    draw_rect(&mut f3, 24, 11, 7, 4, dark_red);
    frames.push(f3);

    (frames, w, h)
}

/// Generates procedural 4-frame Sun / Celestial body (Pulse 1, Pulse 2, Eclipse start, Total Eclipse).
fn generate_procedural_sun() -> (Vec<RgbaImage>, u32, u32) {
    let w = 48;
    let h = 48;
    let mut frames = Vec::new();

    let gold = Rgba([255, 215, 0, 255]);
    let bright_yellow = Rgba([255, 250, 180, 255]);
    let orange_glow = Rgba([255, 140, 20, 180]);
    let moon_dark = Rgba([20, 20, 30, 250]);
    let corona_glow = Rgba([255, 220, 100, 220]);

    // Frame 0: Sun Pulse 1
    let mut f0 = ImageBuffer::new(w, h);
    draw_circle(&mut f0, 24, 24, 18, orange_glow);
    draw_circle(&mut f0, 24, 24, 14, gold);
    draw_circle(&mut f0, 24, 24, 9, bright_yellow);
    // Solar rays
    for i in 0..8 {
        let angle = (i as f32) * std::f32::consts::PI / 4.0;
        let rx = 24.0 + angle.cos() * 20.0;
        let ry = 24.0 + angle.sin() * 20.0;
        draw_rect(&mut f0, rx as i32 - 1, ry as i32 - 1, 3, 3, gold);
    }
    frames.push(f0);

    // Frame 1: Sun Pulse 2
    let mut f1 = ImageBuffer::new(w, h);
    draw_circle(&mut f1, 24, 24, 20, orange_glow);
    draw_circle(&mut f1, 24, 24, 15, gold);
    draw_circle(&mut f1, 24, 24, 10, bright_yellow);
    for i in 0..8 {
        let angle = (i as f32) * std::f32::consts::PI / 4.0 + (std::f32::consts::PI / 8.0);
        let rx = 24.0 + angle.cos() * 22.0;
        let ry = 24.0 + angle.sin() * 22.0;
        draw_rect(&mut f1, rx as i32 - 1, ry as i32 - 1, 3, 3, gold);
    }
    frames.push(f1);

    // Frame 2: Partial Eclipse
    let mut f2 = ImageBuffer::new(w, h);
    draw_circle(&mut f2, 24, 24, 16, orange_glow);
    draw_circle(&mut f2, 24, 24, 13, gold);
    // Moon disc overlapping half of the sun
    draw_circle(&mut f2, 20, 24, 12, moon_dark);
    frames.push(f2);

    // Frame 3: Total Eclipse with glowing Corona
    let mut f3 = ImageBuffer::new(w, h);
    draw_circle(&mut f3, 24, 24, 18, corona_glow);
    draw_circle(&mut f3, 24, 24, 15, moon_dark); // Black moon disc blocking center
    draw_circle(&mut f3, 24, 24, 13, Rgba([10, 10, 15, 255]));
    // Diamond ring / flare bead
    draw_circle(&mut f3, 36, 16, 4, bright_yellow);
    frames.push(f3);

    (frames, w, h)
}

fn draw_rect(img: &mut RgbaImage, x: i32, y: i32, width: u32, height: u32, color: Rgba<u8>) {
    let (iw, ih) = img.dimensions();
    for dy in 0..height as i32 {
        for dx in 0..width as i32 {
            let px = x + dx;
            let py = y + dy;
            if px >= 0 && px < iw as i32 && py >= 0 && py < ih as i32 {
                img.put_pixel(px as u32, py as u32, color);
            }
        }
    }
}

fn draw_circle(img: &mut RgbaImage, cx: i32, cy: i32, radius: i32, color: Rgba<u8>) {
    let (iw, ih) = img.dimensions();
    let r2 = radius * radius;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy <= r2 {
                let px = cx + dx;
                let py = cy + dy;
                if px >= 0 && px < iw as i32 && py >= 0 && py < ih as i32 {
                    img.put_pixel(px as u32, py as u32, color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_manager_init() {
        let mgr = AssetManager::new();
        assert!(mgr.get("cat").is_some());
        assert!(mgr.get("crab").is_some());
        assert!(mgr.get("sun").is_some());
        assert!(mgr.get("nonexistent").is_none());
    }

    #[test]
    fn test_procedural_generation_outputs() {
        let mgr = AssetManager::new();

        let cat = mgr.get("cat").unwrap();
        assert_eq!(cat.frames.len(), 4);
        assert_eq!(cat.flipped_frames.len(), 4);
        assert_eq!(cat.frame_w, 32);
        assert_eq!(cat.frame_h, 32);

        let crab = mgr.get("crab").unwrap();
        assert_eq!(crab.frames.len(), 4);
        assert_eq!(crab.frame_w, 32);
        assert_eq!(crab.frame_h, 32);

        let sun = mgr.get("sun").unwrap();
        assert_eq!(sun.frames.len(), 4);
        assert_eq!(sun.frame_w, 48);
        assert_eq!(sun.frame_h, 48);
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
        let result = mgr.register_manifest(custom);
        assert!(result.is_ok());
        assert!(mgr.get("robot_cat").is_some());
    }
}

