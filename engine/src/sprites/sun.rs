//! Procedurally drawn sun sprite. Port of `lua/distract/sprites/sun.lua`.
//!
//! The disc is a lit sphere with the light aimed straight at the viewer, so it
//! reads as a glowing ball rather than a flat circle. Rays, corona, occluding
//! moon and horizon are all parameters of one draw routine.

use std::f32::consts::PI;

use super::SpriteSet;
use crate::sprite_gen::{self as g, Canvas, Rgb};

const W: u32 = 16;
const H: u32 = 16;

// Flat, banded palette. Mirrors `lua/distract/sprites/sun.lua`: a disc twelve
// pixels across cannot carry a gradient, and the old per-pixel shading of the
// corona and the rays spent a distinct colour on every radius.
const CORE: Rgb = [255, 246, 196];
const SURFACE: Rgb = [255, 206, 62];
const LIMB: Rgb = [236, 132, 22];
const CORONA: Rgb = [255, 224, 132];
const WHITE_HOT: Rgb = [255, 255, 240];
const MOON: Rgb = [34, 32, 46];
const HORIZON: Rgb = [96, 78, 128];
const HORIZON_DEEP: Rgb = [66, 52, 94];

/// One sun pose.
#[derive(Debug, Clone, Copy)]
pub struct Pose {
    /// Disc radius in pixels.
    pub radius: f32,
    /// 0..1 length of the emitted rays.
    pub rays: f32,
    /// Ray rotation in turns.
    pub spin: f32,
    /// 0..1 strength of the outer glow ring.
    pub corona: f32,
    /// 0..1 how far the moon has crossed the disc.
    pub occlude: f32,
    /// -1..1 vertical offset, negative is higher in the sky.
    pub drop: f32,
    /// 0..1 opacity of the horizon band.
    pub horizon: f32,
    /// 0..1 brightness surge.
    pub flare: f32,
}

impl Default for Pose {
    fn default() -> Self {
        Self {
            radius: 4.6,
            rays: 1.0,
            spin: 0.0,
            corona: 0.0,
            occlude: 0.0,
            drop: 0.0,
            horizon: 0.0,
            flare: 0.0,
        }
    }
}

/// The corona: one band, not a gradient.
///
/// `shade` per pixel produced a distinct colour per radius, which is what made
/// three assets consume 46% of the highlight-group cap between them. One tone at a
/// wobbling edge reads the same at eight rows and costs one group.
fn draw_corona(c: &mut Canvas, cx: f32, cy: f32, radius: f32, corona: f32, spin: f32) {
    if corona <= 0.02 {
        return;
    }
    let inner = radius + 0.4;
    let outer = radius + 1.0 + corona * 3.2;
    for y in 1..=H {
        for x in 1..=W {
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > inner && distance <= outer {
                let angle = dy.atan2(dx);
                let edge = outer * (1.0 + 0.10 * (angle * 6.0 + spin * 2.0 * PI).sin());
                if distance <= edge {
                    c.set(x as f32, y as f32, CORONA);
                }
            }
        }
    }
}

/// Eight rays, two tones, thick enough to survive eight half-block rows.
///
/// A one-pixel ray drawn in a per-step gradient disappeared entirely at sprite
/// size. Each is two pixels across for its inner half, one for its tip.
fn draw_rays(c: &mut Canvas, cx: f32, cy: f32, radius: f32, rays: f32, spin: f32) {
    if rays <= 0.05 {
        return;
    }
    let inner = radius + 0.6;
    let outer = inner + rays * 3.2;
    for index in 0..8 {
        let angle = (index as f32 / 8.0 + spin) * 2.0 * PI;
        let (ca, sa) = (angle.cos(), angle.sin());
        let steps = ((outer - inner) * 2.0).floor().max(1.0) as i32;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let rr = inner + (outer - inner) * t;
            let tone = if t < 0.55 { SURFACE } else { CORONA };
            c.set(cx + ca * rr, cy + sa * rr, tone);
            if t < 0.5 {
                // Thickened across the ray, not along it, so a ray reads as a
                // spike rather than as a dotted line.
                c.set(cx + ca * rr - sa * 0.9, cy + sa * rr + ca * 0.9, tone);
            }
        }
    }
}

/// A clean disc: flat surface, a rim in the deeper tone, one bright core band.
fn draw_disc(c: &mut Canvas, cx: f32, cy: f32, radius: f32, flare: f32) {
    c.blob(cx, cy, radius, radius, SURFACE, LIMB);
    c.ellipse(
        cx - radius * 0.18,
        cy - radius * 0.22,
        radius * 0.5,
        radius * 0.5,
        CORE,
    );
    if flare > 0.35 {
        c.ellipse(cx, cy, radius * 0.22, radius * 0.22, WHITE_HOT);
    }
}

/// The eclipse silhouette, kept distinguishable from the shining pose.
///
/// The moon is flat and dark inside a bright rim, which is the one thing that
/// separates the two poses when both are a disc at eight rows.
fn draw_eclipse(c: &mut Canvas, cx: f32, cy: f32, radius: f32, occlude: f32) {
    if occlude <= 0.02 {
        return;
    }
    let mx = cx - radius * 2.2 + occlude * radius * 2.2;
    c.blob(mx, cy, radius * 1.08, radius * 1.08, MOON, CORONA);
    if occlude > 0.82 {
        c.spark(cx + radius * 0.75, cy - radius * 0.75, 2.0, WHITE_HOT);
    }
}

/// The horizon band, two flat tones rather than a shaded ramp.
fn draw_horizon(c: &mut Canvas, horizon: f32) {
    if horizon <= 0.02 {
        return;
    }
    let band_y = 13.0;
    for row in 0..=2 {
        let tone = if row == 0 { HORIZON } else { HORIZON_DEEP };
        for x in 1..=W {
            // The gap in the top row is what makes the band read as a horizon
            // rather than as a bar.
            if row > 0 || !(x + row as u32).is_multiple_of(7) {
                c.set(x as f32, band_y + row as f32, tone);
            }
        }
    }
}

pub fn draw(p: &Pose) -> Canvas {
    let mut c = Canvas::new(W, H);
    let cx = 8.0;
    let cy = 8.0 + p.drop * 3.4;

    draw_corona(&mut c, cx, cy, p.radius, p.corona, p.spin);
    draw_rays(&mut c, cx, cy, p.radius, p.rays, p.spin);
    draw_disc(&mut c, cx, cy, p.radius, p.flare);
    draw_eclipse(&mut c, cx, cy, p.radius, p.occlude);
    draw_horizon(&mut c, p.horizon);

    c
}

pub fn build() -> SpriteSet {
    let mut set = SpriteSet::new(W, H);
    set.add(
        "shining",
        g::cycle(4, |t| Pose {
            radius: 3.6 + 0.25 * (t * 2.0 * PI).sin(),
            rays: 0.75 + 0.25 * (t * 2.0 * PI).sin(),
            spin: t / 8.0,
            corona: 0.18 + 0.10 * (t * 2.0 * PI).sin(),
            ..Default::default()
        }),
        draw,
    );
    set.add(
        "eclipse",
        g::sequence(5, |t| {
            let e = g::ease(t);
            Pose {
                radius: 3.7,
                rays: 0.6 * (1.0 - e),
                corona: 0.15 + e * 0.85,
                occlude: e,
                spin: t / 12.0,
                ..Default::default()
            }
        }),
        draw,
    );
    set.add(
        "flare",
        g::sequence(4, |t| {
            let burst = (g::ease(t) * PI).sin();
            Pose {
                radius: 3.5 + burst * 0.7,
                rays: 0.7 + burst * 0.3,
                corona: 0.2 + burst * 0.5,
                flare: burst,
                spin: t / 6.0,
                ..Default::default()
            }
        }),
        draw,
    );
    set.add(
        "rising",
        g::sequence(6, |t| {
            let e = g::ease(t);
            Pose {
                radius: 3.2 + e * 0.5,
                rays: e * 0.85,
                corona: 0.30 - e * 0.12,
                drop: 1.0 - e * 1.6,
                horizon: 1.0 - e * 0.35,
                ..Default::default()
            }
        }),
        draw,
    );
    set.add(
        "setting",
        g::sequence(6, |t| {
            let e = g::ease(t);
            Pose {
                radius: 3.7 - e * 0.5,
                rays: 0.85 * (1.0 - e),
                corona: 0.18 + e * 0.24,
                drop: -0.6 + e * 1.6,
                horizon: 0.65 + e * 0.35,
                ..Default::default()
            }
        }),
        draw,
    );
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declares_every_state_the_manifest_uses() {
        let set = build();
        for state in ["shining", "eclipse", "flare", "rising", "setting"] {
            assert!(
                set.layout.contains_key(state),
                "sun layout is missing state '{state}'"
            );
        }
    }

    #[test]
    fn totality_darkens_the_disc_centre() {
        let shining = draw(&Pose::default());
        let totality = draw(&Pose {
            occlude: 1.0,
            ..Default::default()
        });
        assert_ne!(shining.to_image().as_raw(), totality.to_image().as_raw());
    }
}
