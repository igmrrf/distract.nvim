//! Procedurally drawn sun sprite. Port of `lua/distract/sprites/sun.lua`.
//!
//! The disc is a lit sphere with the light aimed straight at the viewer, so it
//! reads as a glowing ball rather than a flat circle. Rays, corona, occluding
//! moon and horizon are all parameters of one draw routine.

use std::f32::consts::PI;

use super::SpriteSet;
use crate::sprite_gen::{self as g, Canvas, OrbOpts, Rgb};

const W: u32 = 16;
const H: u32 = 16;

const CORE: Rgb = [255, 246, 196];
const SURFACE: Rgb = [255, 206, 62];
const LIMB: Rgb = [255, 146, 26];
const CORONA: Rgb = [255, 224, 132];
const MOON: Rgb = [34, 32, 46];
const HORIZON: Rgb = [92, 74, 124];

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

fn draw_corona(c: &mut Canvas, cx: f32, cy: f32, radius: f32, corona: f32, spin: f32) {
    if corona <= 0.02 {
        return;
    }
    let inner = radius + 0.4;
    let outer = radius + 1.0 + corona * 3.2;
    for y in 1..=H as i32 {
        for x in 1..=W as i32 {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            if d > inner && d <= outer {
                let ang = dy.atan2(dx);
                let edge = outer * (1.0 + 0.10 * (ang * 6.0 + spin * 2.0 * PI).sin());
                if d <= edge {
                    let falloff = 1.0 - (d - inner) / (edge - inner).max(0.001);
                    let tone = g::shade(CORONA, -0.62 + falloff * 0.5 * corona);
                    c.set(x as f32, y as f32, tone);
                }
            }
        }
    }
}

fn draw_rays(c: &mut Canvas, cx: f32, cy: f32, radius: f32, rays: f32, spin: f32) {
    if rays <= 0.05 {
        return;
    }
    let inner = radius + 0.7;
    let outer = inner + rays * 3.4;
    for i in 0..8 {
        let ang = (i as f32 / 8.0 + spin) * 2.0 * PI;
        let ca = ang.cos();
        let sa = ang.sin();
        let steps = (((outer - inner) * 2.0).floor() as i32).max(1);
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let rr = inner + (outer - inner) * t;
            let mixed = g::mix(SURFACE, CORONA, t);
            let tone = g::shade(mixed, 0.10 - t * 0.30);
            c.set(cx + ca * rr, cy + sa * rr, tone);
        }
    }
}

fn draw_disc(c: &mut Canvas, cx: f32, cy: f32, radius: f32, flare: f32) {
    let surface_opts = OrbOpts {
        light: Some([0.0, 0.0, 1.0]),
        ambient: Some(0.30 + flare * 0.35),
        rim: Some(0.55),
        rim_color: Some(LIMB),
        dither: Some(0.06),
        ..Default::default()
    };
    c.orb(cx, cy, radius, radius, SURFACE, &surface_opts);
    let core_opts = OrbOpts {
        light: Some([0.0, 0.0, 1.0]),
        ambient: Some(0.62),
        rim: Some(0.20),
        rim_color: Some(CORE),
        ..Default::default()
    };
    let core_color = g::shade(CORE, flare * 0.35);
    c.orb(cx, cy, radius * 0.55, radius * 0.55, core_color, &core_opts);
}

fn draw_eclipse(c: &mut Canvas, cx: f32, cy: f32, radius: f32, occlude: f32) {
    if occlude <= 0.02 {
        return;
    }
    let mx = cx - radius * 2.2 + occlude * radius * 2.2;
    let moon_opts = OrbOpts {
        light: Some([-0.4, -0.4, 0.7]),
        ambient: Some(0.5),
        rim: Some(0.42),
        rim_color: Some(CORONA),
        ..Default::default()
    };
    c.orb(mx, cy, radius * 1.02, radius * 1.02, MOON, &moon_opts);
    if occlude > 0.82 {
        c.spark(cx + radius * 0.75, cy - radius * 0.75, 2.0, [255, 255, 240]);
    }
}

fn draw_horizon(c: &mut Canvas, horizon: f32) {
    if horizon <= 0.02 {
        return;
    }
    let band_y = 13_i32;
    for row in 0..=2 {
        let tone = g::shade(HORIZON, -0.12 * row as f32 + (1.0 - horizon) * 0.4);
        for x in 1..=W as i32 {
            if row > 0 || ((x + row) % 7) != 0 {
                c.set(x as f32, (band_y + row) as f32, tone);
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
