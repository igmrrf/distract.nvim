//! Procedural sprite generator shared by every built-in asset.
//!
//! A canvas is a grid of RGB triples (or `None` for transparent) that the
//! terminal backend turns into half-block rows and the overlay backend
//! uploads into a texture atlas. [`Canvas::orb`] and [`Canvas::cel_orb`] are
//! the two shading models sprites are built from: `orb` is continuous
//! Lambertian shading (used for the sun's smooth disc), `cel_orb` quantises
//! that same lighting into flat shadow/base/highlight bands with a hard
//! outline (used for the cat and crab's cartoon look).
//!
//! This is a line-by-line port of `lua/distract/sprite_gen.lua` so both
//! backends draw from one design language; the two must be kept in parity by
//! hand, since nothing enforces it at compile time.

use image::{Rgba, RgbaImage};

use crate::shading::{DEFAULT_LIGHT, dither, mix, normalize, shade};

pub use crate::shading::{CelOrbOpts, OrbOpts, Rgb};

/// A grid of RGB pixels, `None` where transparent. Coordinates are 1-based to
/// match the Lua port, so the two stay comparable line by line.
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
        let (xf, yf) = (x.floor(), y.floor());
        if xf >= 1.0 && yf >= 1.0 && xf <= self.w as f32 && yf <= self.h as f32 {
            let idx = ((yf as u32 - 1) * self.w + (xf as u32 - 1)) as usize;
            self.px[idx] = Some(color);
        }
    }

    /// Reads a pixel, or `None` when it is transparent or out of bounds.
    pub fn get(&self, x: f32, y: f32) -> Option<Rgb> {
        let (xf, yf) = (x.floor(), y.floor());
        if xf < 1.0 || yf < 1.0 || xf > self.w as f32 || yf > self.h as f32 {
            return None;
        }
        self.px[((yf as u32 - 1) * self.w + (xf as u32 - 1)) as usize]
    }

    /// Converts the canvas into an RGBA image. Transparent cells become fully
    /// transparent black rather than being dropped, so every frame of an asset
    /// has identical dimensions.
    pub fn to_image(&self) -> RgbaImage {
        let mut img = RgbaImage::new(self.w, self.h);
        for y in 0..self.h {
            for x in 0..self.w {
                let rgba = match self.px[(y * self.w + x) as usize] {
                    Some(c) => Rgba([c[0], c[1], c[2], 255]),
                    None => Rgba([0, 0, 0, 0]),
                };
                img.put_pixel(x, y, rgba);
            }
        }
        img
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

    /// Axis-aligned filled rectangle with its top-left at (x, y).
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgb) {
        for dy in 0..(h.floor() as i32) {
            for dx in 0..(w.floor() as i32) {
                self.set(x + dx as f32, y + dy as f32, color);
            }
        }
    }

    /// Flat-filled ellipse with a genuine one-pixel contour around it.
    ///
    /// The silhouette primitive. At 24x16 a sprite is 24 columns by eight
    /// half-block rows, and a five-term lighting model spends every one of them
    /// on gradient nobody can see; a flat fill inside a dark outline is what
    /// actually reads, and it collapses the number of distinct colours -- and so
    /// of Neovim highlight groups -- an asset needs.
    ///
    /// The contour is the *rim*: a pixel inside the ellipse whose
    /// four-neighbourhood leaves it. Drawing it as two ellipses instead -- a
    /// contour disc with a smaller fill disc inset -- looks equivalent and is
    /// not, because the radii quantise to whole pixels: at a head-sized
    /// `rx = 2.4` the inset fill collapses to a single plus and the head renders
    /// as a dark blob with a fur pixel in it.
    pub fn blob(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, fill: Rgb, contour: Rgb) {
        let (rx, ry) = (rx.max(0.5), ry.max(0.5));
        let inside = |dx: i32, dy: i32| {
            let (nx, ny) = (dx as f32 / rx, dy as f32 / ry);
            nx * nx + ny * ny <= 1.0
        };

        let (rxi, ryi) = (rx.floor() as i32, ry.floor() as i32);
        for dy in -ryi..=ryi {
            for dx in -rxi..=rxi {
                if !inside(dx, dy) {
                    continue;
                }
                let on_rim = !inside(dx - 1, dy)
                    || !inside(dx + 1, dy)
                    || !inside(dx, dy - 1)
                    || !inside(dx, dy + 1);
                let color = if on_rim { contour } else { fill };
                self.set(cx + dx as f32, cy + dy as f32, color);
            }
        }
    }

    /// Flat capsule from (x0, y0) to (x1, y1) with a one-pixel contour.
    ///
    /// Two passes, so one step's contour cannot be painted over the previous
    /// step's fill and leave a dark seam down the middle of a leg.
    pub fn limb(&mut self, from: [f32; 2], to: [f32; 2], radius: f32, fill: Rgb, contour: Rgb) {
        let steps = (((to[0] - from[0]).powi(2) + (to[1] - from[1]).powi(2)).sqrt() * 2.0)
            .floor()
            .max(1.0) as i32;
        for pass in 0..2 {
            let color = if pass == 0 { contour } else { fill };
            let inset = if pass == 0 { 0.0 } else { 1.0 };
            for index in 0..=steps {
                let t = index as f32 / steps as f32;
                let r = radius * (1.0 - 0.25 * t) - inset;
                self.ellipse(
                    from[0] + (to[0] - from[0]) * t,
                    from[1] + (to[1] - from[1]) * t,
                    r,
                    r,
                    color,
                );
            }
        }
    }

    /// Bresenham line between two points, inclusive of both endpoints.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Rgb) {
        let (mut x0i, mut y0i) = (x0.floor() as i32, y0.floor() as i32);
        let (x1i, y1i) = (x1.floor() as i32, y1.floor() as i32);
        let dx = (x1i - x0i).abs();
        let dy = -(y1i - y0i).abs();
        let sx = if x0i < x1i { 1 } else { -1 };
        let sy = if y0i < y1i { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.set(x0i as f32, y0i as f32, color);
            if x0i == x1i && y0i == y1i {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0i += sx;
            }
            if e2 <= dx {
                err += dx;
                y0i += sy;
            }
        }
    }

    /// Shaded ellipse, lit as a continuous Lambertian hemisphere with a warm
    /// bounce fill light and a grazing-angle rim light.
    pub fn orb(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, base: Rgb, opts: &OrbOpts) {
        let light = normalize(opts.light.unwrap_or(DEFAULT_LIGHT));
        let amb = opts.ambient.unwrap_or(0.34);
        let fill_dir = normalize(
            opts.fill_dir
                .unwrap_or([-light[0] * 0.7, 0.8, -light[2] * 0.5]),
        );
        let fill_strength = opts.fill.unwrap_or(0.15);
        let fill_color = opts.fill_color.unwrap_or([255, 230, 200]);
        let (rx, ry) = (rx.max(0.5), ry.max(0.5));
        let (rxi, ryi) = (rx.floor() as i32, ry.floor() as i32);
        for dy in -ryi..=ryi {
            for dx in -rxi..=rxi {
                let (nx, ny) = (dx as f32 / rx, dy as f32 / ry);
                let r2 = nx * nx + ny * ny;
                if r2 <= 1.0 {
                    let nz = (1.0 - r2).max(0.0).sqrt();
                    let diff = (nx * light[0] + ny * light[1] + nz * light[2]).max(0.0);
                    let mut lvl = amb + (1.0 - amb) * diff;
                    if let Some(ds) = opts.dither {
                        lvl += dither(cx + dx as f32, cy + dy as f32, ds);
                    }
                    let mut col = shade(base, (lvl - 1.0) * 0.85);
                    if fill_strength > 0.0 {
                        let fill_diffuse =
                            (nx * fill_dir[0] + ny * fill_dir[1] + nz * fill_dir[2]).max(0.0);
                        col = mix(col, fill_color, fill_diffuse * fill_strength);
                    }
                    if let Some(rs) = opts.rim {
                        col = mix(
                            col,
                            opts.rim_color.unwrap_or([220, 235, 255]),
                            (1.0 - nz).powi(3) * rs,
                        );
                    }
                    if let Some(flat) = opts.flatten {
                        col = mix(col, base, flat);
                    }
                    self.set(cx + dx as f32, cy + dy as f32, col);
                }
            }
        }
    }

    /// Shaded ellipse quantised into flat shadow/base/highlight bands with a
    /// hard silhouette outline, for a cel-shaded/cartoon read.
    pub fn cel_orb(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, base: Rgb, opts: &CelOrbOpts) {
        let light = normalize(opts.light.unwrap_or(DEFAULT_LIGHT));
        let shadow = opts.shadow.unwrap_or_else(|| shade(base, -0.36));
        let highlight = opts.highlight.unwrap_or_else(|| shade(base, 0.28));
        let thresh = opts.outline_threshold.unwrap_or(0.84);
        let rim = opts.rim.unwrap_or(0.0);
        let (rx, ry) = (rx.max(0.5), ry.max(0.5));
        let (rxi, ryi) = (rx.floor() as i32, ry.floor() as i32);
        for dy in -ryi..=ryi {
            for dx in -rxi..=rxi {
                let (nx, ny) = (dx as f32 / rx, dy as f32 / ry);
                let r2 = nx * nx + ny * ny;
                if r2 > 1.0 {
                    continue;
                }
                if let Some(outline) = opts.outline {
                    if r2 >= thresh {
                        self.set(cx + dx as f32, cy + dy as f32, outline);
                        continue;
                    }
                }
                let nz = (1.0 - r2).max(0.0).sqrt();
                let diff = nx * light[0] + ny * light[1] + nz * light[2];
                let mut col = if diff > 0.42 {
                    highlight
                } else if diff < -0.05 || ny > 0.35 {
                    shadow
                } else {
                    base
                };
                if rim > 0.0 && nz < 0.35 && diff > -0.1 {
                    col = mix(col, opts.rim_color.unwrap_or([255, 255, 255]), rim);
                }
                self.set(cx + dx as f32, cy + dy as f32, col);
            }
        }
    }

    /// Shaded capsule from `start` to `end`, tapering toward the far end;
    /// used for limbs and tails drawn with cel shading.
    pub fn cel_limb(
        &mut self,
        start: [f32; 2],
        end: [f32; 2],
        radius: f32,
        base: Rgb,
        opts: &CelOrbOpts,
    ) {
        let (x0, y0, x1, y1) = (start[0], start[1], end[0], end[1]);
        let dist = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
        let steps = ((dist * 2.0).floor() as i32).max(1);
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let x = x0 + (x1 - x0) * t;
            let y = y0 + (y1 - y0) * t;
            let r = radius * (1.0 - 0.25 * t);
            self.cel_orb(x, y, r, r, base, opts);
        }
    }

    /// Filled flat-colour triangle, used for angular details like ears.
    pub fn triangle(&mut self, p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], color: Rgb) {
        let min_x = p0[0].min(p1[0].min(p2[0])).floor() as i32;
        let max_x = p0[0].max(p1[0].max(p2[0])).floor() as i32;
        let min_y = p0[1].min(p1[1].min(p2[1])).floor() as i32;
        let max_y = p0[1].max(p1[1].max(p2[1])).floor() as i32;
        let edge = |a: [f32; 2], b: [f32; 2], px: f32, py: f32| -> f32 {
            (px - a[0]) * (b[1] - a[1]) - (py - a[1]) * (b[0] - a[0])
        };
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let w0 = edge(p1, p2, px, py);
                let w1 = edge(p2, p0, px, py);
                let w2 = edge(p0, p1, px, py);
                if (w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0) || (w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0) {
                    self.set(x as f32, y as f32, color);
                }
            }
        }
    }

    /// 4-pointed micro sparkle / specular star, fading outward from its centre.
    pub fn spark(&mut self, cx: f32, cy: f32, radius: f32, color: Rgb) {
        let (cxi, cyi) = (cx.floor() as i32, cy.floor() as i32);
        let r = radius.floor() as i32;
        self.set(cx, cy, color);
        for d in 1..=r {
            let fade = shade(color, -0.3 * d as f32);
            self.set((cxi + d) as f32, cy, fade);
            self.set((cxi - d) as f32, cy, fade);
            self.set(cx, (cyi + d) as f32, fade);
            self.set(cx, (cyi - d) as f32, fade);
        }
    }

    /// Filled annulus between `inner_r` and `outer_r`, flat coloured.
    pub fn ring(&mut self, cx: f32, cy: f32, inner_r: f32, outer_r: f32, color: Rgb) {
        let r_max = outer_r.floor() as i32;
        for dy in -r_max..=r_max {
            for dx in -r_max..=r_max {
                let dist = ((dx * dx + dy * dy) as f32).sqrt();
                if dist >= inner_r && dist <= outer_r {
                    self.set(cx + dx as f32, cy + dy as f32, color);
                }
            }
        }
    }
}

/// Samples `n` poses for a looping animation. `t` runs 0 .. (n-1)/n so the last
/// frame flows back into the first without repeating it.
pub fn cycle<P>(n: usize, mut pose_fn: impl FnMut(f32) -> P) -> Vec<P> {
    (0..n).map(|i| pose_fn(i as f32 / n as f32)).collect()
}

/// Samples `n` poses for a one-shot animation. `t` runs 0 .. 1 inclusive.
pub fn sequence<P>(n: usize, mut pose_fn: impl FnMut(f32) -> P) -> Vec<P> {
    if n == 1 {
        return vec![pose_fn(0.0)];
    }
    (0..n).map(|i| pose_fn(i as f32 / (n - 1) as f32)).collect()
}

/// Smooth ease in/out over 0..1, for pose curves that should not start or stop
/// abruptly.
pub fn ease(t: f32) -> f32 {
    let ct = t.clamp(0.0, 1.0);
    ct * ct * (3.0 - 2.0 * ct)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_writes_are_one_based_and_clipped() {
        // The only assertion that reaches inside the canvas: `px` is private, and
        // clipping is precisely the thing that leaves no observable trace.
        let mut c = Canvas::new(8, 4);
        c.set(0.0, 0.0, [1, 2, 3]);
        c.set(9.0, 4.0, [1, 2, 3]);
        assert_eq!(c.px.iter().flatten().count(), 0);
    }

    #[test]
    fn orb_shades_a_gradient_rather_than_a_flat_disc() {
        let mut c = Canvas::new(16, 16);
        c.orb(8.0, 8.0, 6.0, 6.0, [200, 100, 50], &OrbOpts::default());
        let colors: std::collections::HashSet<Rgb> = c.px.iter().flatten().copied().collect();
        assert!(colors.len() > 6);
    }
}
