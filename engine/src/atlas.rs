//! Sprite atlas packing.
//!
//! Every frame of every loaded asset is packed into one image, uploaded to the
//! GPU once, and then drawn as instanced quads. The previous design re-uploaded
//! a full-screen framebuffer every frame — about 2 GB/s at 4K — to draw a few
//! kilobytes of actual pixel data.
//!
//! Packing is a shelf algorithm: frames are sorted tallest first and laid out
//! left to right in rows. Sprite frames are small and uniform per asset, so
//! this wastes very little and is easy to reason about.

use std::collections::HashMap;

use image::RgbaImage;

use crate::asset::AssetManager;

/// One frame's rectangle in the atlas, normalised to 0..1 as `[u0, v0, u1, v1]`.
pub type UvRect = [f32; 4];

/// Transparent gutter between packed frames, so nearest-neighbour sampling at
/// a rectangle edge cannot pick up a neighbour's pixel.
const PADDING: u32 = 1;

#[derive(Debug)]
pub struct Atlas {
    pub image: RgbaImage,
    /// Asset name -> per-frame UV rectangle, indexed the same way as
    /// `LoadedAsset::frames`.
    pub rects: HashMap<String, Vec<UvRect>>,
}

impl Atlas {
    pub fn width(&self) -> u32 {
        self.image.width()
    }

    pub fn height(&self) -> u32 {
        self.image.height()
    }

    /// UV rectangle for one frame, mirrored horizontally when `flip_x` is set.
    ///
    /// Mirroring by swapping the u bounds is why the loader no longer keeps a
    /// second flipped copy of every frame alive for the process lifetime.
    pub fn uv(&self, asset: &str, frame: usize, flip_x: bool) -> Option<UvRect> {
        let frames = self.rects.get(asset)?;
        if frames.is_empty() {
            return None;
        }
        let r = frames[frame % frames.len()];
        Some(if flip_x { [r[2], r[1], r[0], r[3]] } else { r })
    }

    /// Packs every frame the manager currently holds.
    pub fn build(manager: &AssetManager, max_dim: u32) -> Result<Self, String> {
        // (asset name, frame index, w, h)
        let mut items: Vec<(&str, usize, u32, u32)> = Vec::new();
        for (name, asset) in manager.iter() {
            for (i, frame) in asset.frames.iter().enumerate() {
                let (w, h) = frame.dimensions();
                items.push((name.as_str(), i, w, h));
            }
        }

        if items.is_empty() {
            return Ok(Self {
                image: RgbaImage::new(1, 1),
                rects: HashMap::new(),
            });
        }

        // Tallest first keeps shelves tight.
        items.sort_by(|a, b| b.3.cmp(&a.3).then(b.2.cmp(&a.2)));

        let widest = items.iter().map(|i| i.2).max().unwrap_or(1) + PADDING * 2;
        let total_area: u64 = items
            .iter()
            .map(|i| (i.2 + PADDING) as u64 * (i.3 + PADDING) as u64)
            .sum();

        // Start from a square-ish guess and grow by powers of two until
        // everything fits, so the common case settles in one attempt.
        let mut width = ((total_area as f64).sqrt().ceil() as u32)
            .max(widest)
            .next_power_of_two()
            .min(max_dim);

        loop {
            if let Some(packed) = try_pack(&items, width, max_dim) {
                return Ok(Self::render(&items, manager, width, packed.0, packed.1));
            }
            if width >= max_dim {
                return Err(format!(
                    "sprite atlas does not fit in {0}x{0}px; reduce frame sizes or frame counts",
                    max_dim
                ));
            }
            width = (width * 2).min(max_dim);
        }
    }

    fn render(
        items: &[(&str, usize, u32, u32)],
        manager: &AssetManager,
        width: u32,
        height: u32,
        placements: Vec<(u32, u32)>,
    ) -> Self {
        let mut image = RgbaImage::new(width, height);
        let mut rects: HashMap<String, Vec<UvRect>> = HashMap::new();

        for (name, asset) in manager.iter() {
            rects.insert(name.clone(), vec![[0.0; 4]; asset.frames.len()]);
        }

        for (item, &(x, y)) in items.iter().zip(placements.iter()) {
            let (name, frame_idx, w, h) = *item;
            let Some(asset) = manager.get(name) else {
                continue;
            };
            let src = &asset.frames[frame_idx];
            image::imageops::replace(&mut image, src, x as i64, y as i64);

            if let Some(slots) = rects.get_mut(name) {
                slots[frame_idx] = [
                    x as f32 / width as f32,
                    y as f32 / height as f32,
                    (x + w) as f32 / width as f32,
                    (y + h) as f32 / height as f32,
                ];
            }
        }

        Self { image, rects }
    }
}

/// Lays items out on shelves of the given width. Returns the resulting height
/// and one origin per item, or `None` when the result would exceed `max_dim`.
fn try_pack(
    items: &[(&str, usize, u32, u32)],
    width: u32,
    max_dim: u32,
) -> Option<(u32, Vec<(u32, u32)>)> {
    let mut placements = Vec::with_capacity(items.len());
    let (mut x, mut y, mut shelf_h) = (PADDING, PADDING, 0u32);

    for &(_, _, w, h) in items {
        if w + PADDING * 2 > width {
            return None;
        }
        if x + w + PADDING > width {
            // New shelf.
            x = PADDING;
            y += shelf_h + PADDING;
            shelf_h = 0;
        }
        if y + h + PADDING > max_dim {
            return None;
        }
        placements.push((x, y));
        x += w + PADDING;
        shelf_h = shelf_h.max(h);
    }

    let height = (y + shelf_h + PADDING).next_power_of_two().min(max_dim);
    if height > max_dim {
        return None;
    }
    Some((height, placements))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::AssetManifest;

    fn manager() -> AssetManager {
        AssetManager::new()
    }

    #[test]
    fn packs_every_frame_of_every_builtin() {
        let mgr = manager();
        let atlas = Atlas::build(&mgr, 8192).unwrap();

        for (name, asset) in mgr.iter() {
            let rects = atlas.rects.get(name).unwrap();
            assert_eq!(rects.len(), asset.frames.len(), "{}", name);
        }
    }

    #[test]
    fn every_rect_is_inside_the_atlas_and_has_area() {
        let mgr = manager();
        let atlas = Atlas::build(&mgr, 8192).unwrap();

        for (name, rects) in &atlas.rects {
            for (i, r) in rects.iter().enumerate() {
                assert!(r[0] >= 0.0 && r[1] >= 0.0, "{} frame {}", name, i);
                assert!(r[2] <= 1.0 && r[3] <= 1.0, "{} frame {}", name, i);
                assert!(r[2] > r[0] && r[3] > r[1], "{} frame {} is empty", name, i);
            }
        }
    }

    #[test]
    fn rects_do_not_overlap() {
        let mgr = manager();
        let atlas = Atlas::build(&mgr, 8192).unwrap();
        let (aw, ah) = (atlas.width() as f32, atlas.height() as f32);

        let mut boxes: Vec<(f32, f32, f32, f32)> = Vec::new();
        for rects in atlas.rects.values() {
            for r in rects {
                boxes.push((r[0] * aw, r[1] * ah, r[2] * aw, r[3] * ah));
            }
        }

        for i in 0..boxes.len() {
            for j in (i + 1)..boxes.len() {
                let (a, b) = (boxes[i], boxes[j]);
                let overlap = a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3;
                assert!(!overlap, "{:?} overlaps {:?}", a, b);
            }
        }
    }

    #[test]
    fn packed_pixels_match_the_source_frame() {
        let mgr = manager();
        let atlas = Atlas::build(&mgr, 8192).unwrap();
        let cat = mgr.get("cat").unwrap();

        let r = atlas.rects["cat"][3];
        let x0 = (r[0] * atlas.width() as f32).round() as u32;
        let y0 = (r[1] * atlas.height() as f32).round() as u32;

        for y in 0..cat.frame_h {
            for x in 0..cat.frame_w {
                assert_eq!(
                    atlas.image.get_pixel(x0 + x, y0 + y),
                    cat.frames[3].get_pixel(x, y),
                    "mismatch at {},{}",
                    x,
                    y
                );
            }
        }
    }

    #[test]
    fn flipping_swaps_the_horizontal_bounds_only() {
        let mgr = manager();
        let atlas = Atlas::build(&mgr, 8192).unwrap();

        let normal = atlas.uv("cat", 0, false).unwrap();
        let flipped = atlas.uv("cat", 0, true).unwrap();
        assert_eq!(flipped[0], normal[2]);
        assert_eq!(flipped[2], normal[0]);
        assert_eq!(flipped[1], normal[1]);
        assert_eq!(flipped[3], normal[3]);
    }

    #[test]
    fn out_of_range_frame_indices_wrap_rather_than_vanish() {
        let mgr = manager();
        let atlas = Atlas::build(&mgr, 8192).unwrap();
        let n = mgr.get("cat").unwrap().frames.len();
        assert_eq!(atlas.uv("cat", n, false), atlas.uv("cat", 0, false));
    }

    #[test]
    fn unknown_asset_has_no_rectangle() {
        let mgr = manager();
        let atlas = Atlas::build(&mgr, 8192).unwrap();
        assert!(atlas.uv("no_such_asset", 0, false).is_none());
    }

    #[test]
    fn an_atlas_that_cannot_fit_reports_rather_than_truncating() {
        let mgr = manager();
        // 32px is smaller than a single 24x16 cat frame plus padding once the
        // whole set is in play.
        let err = Atlas::build(&mgr, 32).unwrap_err();
        assert!(err.contains("does not fit"), "unexpected message: {}", err);
    }

    #[test]
    fn a_custom_asset_joins_the_atlas() {
        let mut mgr = manager();
        let mut custom = AssetManifest::default_crab();
        custom.name = "second_crab".to_string();
        mgr.register_manifest(custom).unwrap();

        let atlas = Atlas::build(&mgr, 8192).unwrap();
        assert!(atlas.uv("second_crab", 0, false).is_some());
        assert!(atlas.uv("cat", 0, false).is_some());
    }
}
