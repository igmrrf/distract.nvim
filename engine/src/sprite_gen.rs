//! Procedural sprite generator.
//!
//! A direct port of `lua/distract/sprite_gen.lua`, so both backends draw from
//! one design language instead of the overlay falling back to a separate,
//! simpler four-frame set.
//!
//! A canvas is a grid of RGB triples (or `None` for transparent) which the
//! terminal backend turns into half-block rows and the overlay backend uploads
//! into a texture atlas. Drawing them means animation can be produced by
//! sampling a pose function, so a state's frames are smooth by construction
//! rather than by getting each hand-drawn frame right by eye.
//!
//! Volume comes from [`Canvas::orb`], which shades an ellipse as if it were a
//! lit hemisphere: Lambert diffuse from a light direction, a rim term at
//! grazing angles, and a specular highlight. That is what gives flat pixel art
//! its rounded, three-dimensional read.
//!
//! Coordinates are 1-based to match the Lua original: keeping the two in the
//! same coordinate space is what makes the ports comparable line by line.

use image::{Rgba, RgbaImage};

pub type Rgb = [u8; 3];

/// Default key light: above, slightly to the entity's left, angled toward the
/// viewer. Shared by every asset so they look lit by the same source.
pub const DEFAULT_LIGHT: [f32; 3] = [-0.5, -0.62, 0.6];

// =========================================================================
// Canvas
// =========================================================================

#[derive(Debug, Clone)]
pub struct Canvas {
    pub w: u32,
    pub h: u32,
    px: Vec<Option<Rgb>>,
}

impl Canvas {
    /// Creates a `w` x `h` fully transparent canvas.
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            px: vec![None; (w * h) as usize],
        }
    }

    /// Writes a pixel. Coordinates are 1-based; out-of-bounds writes are dropped.
    pub fn set(&mut self, x: f32, y: f32, color: Rgb) {
        let (x, y) = (x.floor(), y.floor());
        if x < 1.0 || y < 1.0 || x > self.w as f32 || y > self.h as f32 {
            return;
        }
        let idx = ((y as u32 - 1) * self.w + (x as u32 - 1)) as usize;
        self.px[idx] = Some(color);
    }

    /// Reads a pixel, or `None` when it is transparent or out of bounds.
    pub fn get(&self, x: f32, y: f32) -> Option<Rgb> {
        let (x, y) = (x.floor(), y.floor());
        if x < 1.0 || y < 1.0 || x > self.w as f32 || y > self.h as f32 {
            return None;
        }
        self.px[((y as u32 - 1) * self.w + (x as u32 - 1)) as usize]
    }

    /// Converts the canvas into an RGBA image. Transparent cells become a fully
    /// transparent black rather than being dropped, so every frame of an asset
    /// has identical dimensions.
    pub fn to_image(&self) -> RgbaImage {
        let mut img = RgbaImage::new(self.w, self.h);
        for y in 0..self.h {
            for x in 0..self.w {
                let px = self.px[(y * self.w + x) as usize];
                let rgba = match px {
                    Some(c) => Rgba([c[0], c[1], c[2], 255]),
                    None => Rgba([0, 0, 0, 0]),
                };
                img.put_pixel(x, y, rgba);
            }
        }
        img
    }

    // =====================================================================
    // Primitives
    // =====================================================================

    /// Axis-aligned filled rectangle. Clips to the canvas.
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgb) {
        for dy in 0..h.floor().max(0.0) as i32 {
            for dx in 0..w.floor().max(0.0) as i32 {
                self.set(x + dx as f32, y + dy as f32, color);
            }
        }
    }

    /// Filled ellipse centred on (cx, cy) with radii rx, ry.
    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, color: Rgb) {
        let (rx, ry) = (rx.max(0.5), ry.max(0.5));
        let (rxi, ryi) = (rx.floor() as i32, ry.floor() as i32);
        for dy in -ryi..=ryi {
            for dx in -rxi..=rxi {
                let (nx, ny) = (dx as f32 / rx, dy as f32 / ry);
                if nx * nx + ny * ny <= 1.0 {
                    self.set(cx + dx as f32, cy + dy as f32, color);
                }
            }
        }
    }

    /// Bresenham line between two points, inclusive of both endpoints.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Rgb) {
        let (mut x0, mut y0) = (x0.floor() as i32, y0.floor() as i32);
        let (x1, y1) = (x1.floor() as i32, y1.floor() as i32);
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            self.set(x0 as f32, y0 as f32, color);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    // =====================================================================
    // Volumetric Shading & Vector Primitives
    // =====================================================================

    /// Shaded ellipse, lit as a hemisphere with multi-point lighting.
    pub fn orb(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, base: Rgb, opts: &OrbOpts) {
        let light = normalize(opts.light.unwrap_or(DEFAULT_LIGHT));
        let fill_dir = normalize(
            opts.fill_dir
                .unwrap_or([-light[0] * 0.7, 0.8, -light[2] * 0.5]),
        );
        let ambient = opts.ambient.unwrap_or(0.34);
        let rim_strength = opts.rim.unwrap_or(0.30);
        let rim_color = opts.rim_color.unwrap_or([220, 235, 255]);
        let fill_strength = opts.fill.unwrap_or(0.15);
        let fill_color = opts.fill_color.unwrap_or([255, 230, 200]);
        let spec_strength = opts.specular.unwrap_or(0.45);
        let shininess = opts.shininess.unwrap_or(12.0);
        let dither_strength = opts.dither.unwrap_or(0.0);
        let flatten = opts.flatten.unwrap_or(0.0);

        let (rx, ry) = (rx.max(0.5), ry.max(0.5));
        let (rxi, ryi) = (rx.floor() as i32, ry.floor() as i32);

        for dy in -ryi..=ryi {
            for dx in -rxi..=rxi {
                let (nx, ny) = (dx as f32 / rx, dy as f32 / ry);
                let r2 = nx * nx + ny * ny;
                if r2 > 1.0 {
                    continue;
                }

                let nz = (1.0 - r2).max(0.0).sqrt();
                let diffuse = (nx * light[0] + ny * light[1] + nz * light[2]).max(0.0);
                let fill_diffuse =
                    (nx * fill_dir[0] + ny * fill_dir[1] + nz * fill_dir[2]).max(0.0);

                let mut level = ambient + (1.0 - ambient) * diffuse;
                if dither_strength > 0.0 {
                    level += dither(cx + dx as f32, cy + dy as f32, dither_strength);
                }
                let mut color = shade(base, (level - 1.0) * 0.85);

                // Warm bounce fill light
                if fill_strength > 0.0 {
                    color = mix(color, fill_color, fill_diffuse * fill_strength);
                }

                // Rim light: strongest where the surface turns away from the viewer
                if rim_strength > 0.0 {
                    let rim = (1.0 - nz).powi(3);
                    color = mix(color, rim_color, rim * rim_strength);
                }

                // Specular: a tight highlight where the surface points at the light
                if spec_strength > 0.0 {
                    let spec = diffuse.powf(shininess);
                    color = mix(color, [255, 255, 255], spec * spec_strength);
                }

                if flatten > 0.0 {
                    color = mix(color, base, flatten);
                }

                self.set(cx + dx as f32, cy + dy as f32, color);
            }
        }
    }

    /// Shaded capsule along an arbitrary axis; used for limbs and tails.
    pub fn limb(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, radius: f32, base: Rgb) {
        self.limb_with(x0, y0, x1, y1, radius, base, &LimbOpts::default())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn limb_with(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        radius: f32,
        base: Rgb,
        opts: &LimbOpts,
    ) {
        let dist = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
        let steps = ((dist * 2.0).floor() as i32).max(1);
        let orb_opts = OrbOpts {
            light: opts.light,
            ambient: Some(opts.ambient.unwrap_or(0.42)),
            rim: Some(opts.rim.unwrap_or(0.16)),
            fill: Some(opts.fill.unwrap_or(0.10)),
            specular: Some(opts.specular.unwrap_or(0.12)),
            shininess: Some(opts.shininess.unwrap_or(8.0)),
            dither: opts.dither,
            ..Default::default()
        };
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let x = x0 + (x1 - x0) * t;
            let y = y0 + (y1 - y0) * t;
            let r = radius * (1.0 - 0.25 * t);
            self.orb(x, y, r, r, base, &orb_opts);
        }
    }

    /// 4-pointed micro sparkle / specular star.
    pub fn spark(&mut self, cx: f32, cy: f32, radius: f32, color: Rgb) {
        let (cx, cy) = (cx.floor() as i32, cy.floor() as i32);
        let r = radius.floor() as i32;
        self.set(cx as f32, cy as f32, color);
        for d in 1..=r {
            let fade = shade(color, -0.3 * d as f32);
            self.set((cx + d) as f32, cy as f32, fade);
            self.set((cx - d) as f32, cy as f32, fade);
            self.set(cx as f32, (cy + d) as f32, fade);
            self.set(cx as f32, (cy - d) as f32, fade);
        }
    }

    /// Anti-aliased curved arc for coronal loops and facial curves.
    #[allow(clippy::too_many_arguments)]
    pub fn arc(
        &mut self,
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        start_angle: f32,
        end_angle: f32,
        color: Rgb,
        steps: Option<u32>,
    ) {
        let steps = steps.unwrap_or(16);
        let d_theta = (end_angle - start_angle) / steps as f32;
        for i in 0..steps {
            let a0 = start_angle + i as f32 * d_theta;
            let a1 = start_angle + (i + 1) as f32 * d_theta;
            let x0 = cx + a0.cos() * rx;
            let y0 = cy + a0.sin() * ry;
            let x1 = cx + a1.cos() * rx;
            let y1 = cy + a1.sin() * ry;
            self.line(x0, y0, x1, y1, color);
        }
    }
}

/// Bayer 4x4 ordered dithering matrix normalised to -0.5 .. 0.5.
const BAYER_4X4: [[f32; 4]; 4] = [
    [-0.46875, 0.03125, -0.34375, 0.15625],
    [0.28125, -0.21875, 0.40625, -0.09375],
    [-0.28125, 0.21875, -0.40625, 0.09375],
    [0.46875, -0.03125, 0.34375, -0.15625],
];

/// Retrieves the Bayer dither offset for coordinate (x, y).
pub fn dither(x: f32, y: f32, strength: f32) -> f32 {
    let xi = (x.floor() as i32).rem_euclid(4) as usize;
    let yi = (y.floor() as i32).rem_euclid(4) as usize;
    BAYER_4X4[yi][xi] * strength
}

/// Options for [`Canvas::orb`]. Every field falls back to the documented
/// default when left `None`.
#[derive(Debug, Clone, Default)]
pub struct OrbOpts {
    /// Direction the light comes from (default [`DEFAULT_LIGHT`]).
    pub light: Option<[f32; 3]>,
    /// Direction the fill bounce light comes from.
    pub fill_dir: Option<[f32; 3]>,
    /// Floor brightness in shadow, 0..1 (default 0.34).
    pub ambient: Option<f32>,
    /// Strength of the grazing-angle rim light, 0..1 (default 0.30).
    pub rim: Option<f32>,
    /// Colour of the rim light (default a cool white).
    pub rim_color: Option<Rgb>,
    /// Strength of warm bounce fill light, 0..1 (default 0.15).
    pub fill: Option<f32>,
    /// Colour of the fill light.
    pub fill_color: Option<Rgb>,
    /// Strength of the highlight, 0..1 (default 0.45).
    pub specular: Option<f32>,
    /// Specular exponent; higher is tighter (default 12).
    pub shininess: Option<f32>,
    /// Dithering strength (default 0.0).
    pub dither: Option<f32>,
    /// 0..1, blends the shading back toward flat (default 0).
    pub flatten: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct LimbOpts {
    pub light: Option<[f32; 3]>,
    pub ambient: Option<f32>,
    pub rim: Option<f32>,
    pub fill: Option<f32>,
    pub specular: Option<f32>,
    pub shininess: Option<f32>,
    pub dither: Option<f32>,
}

// =========================================================================
// Colour
// =========================================================================

fn clamp8(v: f32) -> u8 {
    (v + 0.5).floor().clamp(0.0, 255.0) as u8
}

/// Darkens (`amount` < 0) or lightens (`amount` > 0) a colour. `amount` is
/// clamped to -1..1, where -1 is black and 1 is white.
pub fn shade(color: Rgb, amount: f32) -> Rgb {
    let amount = amount.clamp(-1.0, 1.0);
    if amount < 0.0 {
        let k = 1.0 + amount;
        return [
            clamp8(color[0] as f32 * k),
            clamp8(color[1] as f32 * k),
            clamp8(color[2] as f32 * k),
        ];
    }
    [
        clamp8(color[0] as f32 + (255.0 - color[0] as f32) * amount),
        clamp8(color[1] as f32 + (255.0 - color[1] as f32) * amount),
        clamp8(color[2] as f32 + (255.0 - color[2] as f32) * amount),
    ]
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

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len == 0.0 {
        return [0.0, 0.0, 1.0];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

// =========================================================================
// Pose sampling
// =========================================================================

/// Samples `n` poses for a looping animation. `t` runs 0 .. (n-1)/n so the last
/// frame flows back into the first without repeating it.
pub fn cycle<P>(n: usize, pose_fn: impl Fn(f32) -> P) -> Vec<P> {
    (0..n).map(|i| pose_fn(i as f32 / n as f32)).collect()
}

/// Samples `n` poses for a one-shot animation. `t` runs 0 .. 1 inclusive.
pub fn sequence<P>(n: usize, pose_fn: impl Fn(f32) -> P) -> Vec<P> {
    if n <= 1 {
        return vec![pose_fn(0.0)];
    }
    (0..n).map(|i| pose_fn(i as f32 / (n - 1) as f32)).collect()
}

/// Smooth ease in/out over 0..1, for pose curves that should not start or stop
/// abruptly.
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_starts_transparent_and_records_writes() {
        let mut c = Canvas::new(4, 3);
        assert_eq!(c.get(2.0, 2.0), None);
        c.set(2.0, 2.0, [10, 20, 30]);
        assert_eq!(c.get(2.0, 2.0), Some([10, 20, 30]));
    }

    #[test]
    fn canvas_writes_are_one_based_and_clipped() {
        let mut c = Canvas::new(2, 2);
        // 0 and w+1 are both outside a 1-based canvas.
        c.set(0.0, 1.0, [1, 1, 1]);
        c.set(3.0, 1.0, [1, 1, 1]);
        c.set(1.0, 0.0, [1, 1, 1]);
        c.set(1.0, 3.0, [1, 1, 1]);
        let img = c.to_image();
        assert!(img.pixels().all(|p| p[3] == 0));

        c.set(1.0, 1.0, [9, 9, 9]);
        assert_eq!(c.to_image().get_pixel(0, 0), &Rgba([9, 9, 9, 255]));
    }

    #[test]
    fn to_image_keeps_transparent_cells_rather_than_dropping_them() {
        let mut c = Canvas::new(3, 1);
        c.set(2.0, 1.0, [5, 5, 5]);
        let img = c.to_image();
        assert_eq!(img.dimensions(), (3, 1));
        assert_eq!(img.get_pixel(0, 0)[3], 0);
        assert_eq!(img.get_pixel(1, 0)[3], 255);
        assert_eq!(img.get_pixel(2, 0)[3], 0);
    }

    #[test]
    fn shade_moves_toward_black_and_white() {
        assert_eq!(shade([100, 100, 100], -1.0), [0, 0, 0]);
        assert_eq!(shade([100, 100, 100], 1.0), [255, 255, 255]);
        assert_eq!(shade([100, 100, 100], 0.0), [100, 100, 100]);
    }

    #[test]
    fn mix_interpolates_and_clamps_t() {
        assert_eq!(mix([0, 0, 0], [200, 100, 50], 0.5), [100, 50, 25]);
        assert_eq!(mix([0, 0, 0], [200, 100, 50], 2.0), [200, 100, 50]);
        assert_eq!(mix([0, 0, 0], [200, 100, 50], -1.0), [0, 0, 0]);
    }

    #[test]
    fn cycle_never_repeats_the_first_pose_at_the_end() {
        let ts = cycle(4, |t| t);
        assert_eq!(ts, vec![0.0, 0.25, 0.5, 0.75]);
    }

    #[test]
    fn sequence_runs_inclusive_of_one() {
        assert_eq!(sequence(3, |t| t), vec![0.0, 0.5, 1.0]);
        assert_eq!(sequence(1, |t| t), vec![0.0]);
    }

    #[test]
    fn orb_shades_a_gradient_rather_than_a_flat_disc() {
        let mut c = Canvas::new(16, 16);
        c.orb(8.0, 8.0, 6.0, 6.0, [200, 100, 60], &OrbOpts::default());
        let img = c.to_image();

        let mut distinct = std::collections::HashSet::new();
        for p in img.pixels() {
            if p[3] > 0 {
                distinct.insert([p[0], p[1], p[2]]);
            }
        }
        // A flat fill would produce exactly one colour; volume needs many.
        assert!(distinct.len() > 10, "orb produced {} tones", distinct.len());
    }

    #[test]
    fn line_covers_both_endpoints() {
        let mut c = Canvas::new(8, 8);
        c.line(2.0, 2.0, 6.0, 5.0, [1, 2, 3]);
        assert_eq!(c.get(2.0, 2.0), Some([1, 2, 3]));
        assert_eq!(c.get(6.0, 5.0), Some([1, 2, 3]));
    }
}
