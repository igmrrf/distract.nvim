//! Colour arithmetic the sprite generators share.
//!
//! Split from `sprite_gen.rs`, which held the shading model next to the drawing
//! primitives. These are the functions `lua/distract/sprite_gen.lua` has to
//! reproduce exactly for the two engines to draw the same art, so the parity
//! budgets in `tests/sprite_parity_spec.lua` are measured against this file
//! rather than against the drawing.

/// One opaque colour. Every sprite pixel is either this or transparent.
pub type Rgb = [u8; 3];

/// Default key light: above, slightly to the entity's left, angled toward the
/// viewer. Shared by every asset so they look lit by the same source.
pub const DEFAULT_LIGHT: [f32; 3] = [-0.5, -0.62, 0.6];

/// Bayer 4x4 ordered dithering matrix, normalised to -0.5..0.5.
const BAYER_4X4: [[f32; 4]; 4] = [
    [-0.46875, 0.03125, -0.34375, 0.15625],
    [0.28125, -0.21875, 0.40625, -0.09375],
    [-0.28125, 0.21875, -0.40625, 0.09375],
    [0.46875, -0.03125, 0.34375, -0.15625],
];

/// Retrieves the Bayer dither offset for coordinate (x, y), scaled by `strength`.
pub fn dither(x: f32, y: f32, strength: f32) -> f32 {
    let xi = (x.floor() as i32).rem_euclid(4) as usize;
    let yi = (y.floor() as i32).rem_euclid(4) as usize;
    BAYER_4X4[yi][xi] * strength
}

fn clamp8(v: f32) -> u8 {
    (v + 0.5).floor().clamp(0.0, 255.0) as u8
}

/// Darkens (`amount` < 0) or lightens (`amount` > 0) a colour. `amount` is
/// clamped to -1..1, where -1 is black and 1 is white.
pub fn shade(color: Rgb, amount: f32) -> Rgb {
    let amt = amount.clamp(-1.0, 1.0);
    if amt < 0.0 {
        let f = 1.0 + amt;
        [
            clamp8(color[0] as f32 * f),
            clamp8(color[1] as f32 * f),
            clamp8(color[2] as f32 * f),
        ]
    } else {
        [
            clamp8(color[0] as f32 + (255.0 - color[0] as f32) * amt),
            clamp8(color[1] as f32 + (255.0 - color[1] as f32) * amt),
            clamp8(color[2] as f32 + (255.0 - color[2] as f32) * amt),
        ]
    }
}

/// Linear interpolation between two colours, `t` in 0..1.
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [
        clamp8(a[0] as f32 + (b[0] as f32 - a[0] as f32) * t),
        clamp8(a[1] as f32 + (b[1] as f32 - a[1] as f32) * t),
        clamp8(a[2] as f32 + (b[2] as f32 - a[2] as f32) * t),
    ]
}

pub fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len == 0.0 {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

/// Options for [`Canvas::orb`]'s continuous Lambertian shading. Every field
/// falls back to the documented default when left `None`.
#[derive(Debug, Clone, Default)]
pub struct OrbOpts {
    /// Direction the key light comes from (default [`DEFAULT_LIGHT`]).
    pub light: Option<[f32; 3]>,
    /// Floor brightness in shadow, 0..1 (default 0.34).
    pub ambient: Option<f32>,
    /// Direction the warm bounce fill light comes from (default derived from `light`).
    pub fill_dir: Option<[f32; 3]>,
    /// Strength of the warm bounce fill light, 0..1 (default 0.15).
    pub fill: Option<f32>,
    /// Colour of the fill light (default a warm cream).
    pub fill_color: Option<Rgb>,
    /// Strength of the grazing-angle rim light, 0..1 (default 0.30).
    pub rim: Option<f32>,
    /// Colour of the rim light (default a cool white).
    pub rim_color: Option<Rgb>,
    /// Ordered-dither strength applied to the shading level (default 0.0).
    pub dither: Option<f32>,
    /// 0..1, blends the shading back toward flat (default 0).
    pub flatten: Option<f32>,
}

/// Options for [`Canvas::cel_orb`]'s quantised shadow/base/highlight shading.
/// Every field falls back to the documented default when left `None`.
#[derive(Debug, Clone, Default)]
pub struct CelOrbOpts {
    /// Direction the key light comes from (default [`DEFAULT_LIGHT`]).
    pub light: Option<[f32; 3]>,
    /// Flat shadow-band colour (default `base` darkened by 0.36).
    pub shadow: Option<Rgb>,
    /// Flat highlight-band colour (default `base` lightened by 0.28).
    pub highlight: Option<Rgb>,
    /// Outline colour drawn at the silhouette edge; omit for no outline.
    pub outline: Option<Rgb>,
    /// Normalised radius² beyond which the outline is drawn (default 0.84).
    pub outline_threshold: Option<f32>,
    /// Strength of the grazing-angle rim light, 0..1 (default 0.0).
    pub rim: Option<f32>,
    /// Colour of the rim light (default white).
    pub rim_color: Option<Rgb>,
}
