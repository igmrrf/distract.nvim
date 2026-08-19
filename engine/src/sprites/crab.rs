//! Procedurally drawn crab sprite. Port of `lua/distract/sprites/crab.lua`.
//!
//! Same pose-function approach as the cat: a handful of scalars (claw opening,
//! leg phase, eyestalk height, how far it has sunk into the sand) drive one
//! draw routine, and each state samples them along a curve.

use std::f32::consts::PI;

use super::SpriteSet;
use crate::sprite_gen::{self as g, Canvas, Rgb};

const W: u32 = 24;
const H: u32 = 16;

// Flat, banded palette. Mirrors `lua/distract/sprites/crab.lua`. Four shell tones
// did not read at 24x16 -- a carapace twelve pixels across cannot carry a gradient
// -- and every distinct colour pair is a Neovim highlight group.
const CONTOUR: Rgb = [44, 14, 18];
const SHELL: Rgb = [232, 54, 46];
const SHELL_DARK: Rgb = [150, 24, 28];
const SHELL_LIGHT: Rgb = [255, 132, 96];
const CLAW: Rgb = [252, 108, 64];
const CLAW_TOOTH: Rgb = [255, 246, 230];
const EYE_WHITE: Rgb = [255, 255, 255];
const EYE_DARK: Rgb = [24, 20, 32];
const SAND: Rgb = [224, 192, 138];
const SAND_DEEP: Rgb = [186, 154, 104];
const ZZZ: Rgb = [176, 212, 255];

// The bands the old five-term model spent on gradient, aliased so every drawing
// call still names the part of the crab it is drawing.
const SHELL_SPEC: Rgb = SHELL_LIGHT;
const SHELL_GROOVE: Rgb = SHELL_DARK;
const LEG_DARK: Rgb = SHELL_DARK;

/// The rim is a darker shell tone rather than the near-black contour, for the same
/// reason the cat's is: a near-black outline merges into a dark editor background
/// and takes the silhouette's edge with it. CONTOUR is kept for the eye pupil and
/// the closed-eye line, which must read as holes.
const RIM: Rgb = SHELL_DARK;
const WHITE: Rgb = EYE_WHITE;
const SPARKLE: Rgb = CLAW_TOOTH;
const ZZZ_FADE: Rgb = ZZZ;

/// One crab pose.
#[derive(Debug, Clone, Copy)]
pub struct Pose {
    /// 0..1 phase of the sideways scuttle.
    pub leg: f32,
    /// 0..1 claws closed (0 wide open, 1 snapped shut).
    pub clamp: f32,
    /// 0..1 claws lifted overhead.
    pub raise: f32,
    /// 0..1 eyestalks extended.
    pub stalk: f32,
    /// 0..1 eye opening.
    pub eye: f32,
    /// 0..1 buried in sand.
    pub sink: f32,
    /// -1..1 shell bob.
    pub bob: f32,
    /// 0..1 sleep marks.
    pub zzz: f32,
}

impl Default for Pose {
    fn default() -> Self {
        Self {
            leg: 0.0,
            clamp: 0.5,
            raise: 0.0,
            stalk: 1.0,
            eye: 1.0,
            sink: 0.0,
            bob: 0.0,
            zzz: 0.0,
        }
    }
}

fn draw_legs(c: &mut Canvas, cx: f32, cy: f32, shell_ry: f32, leg: f32, sink: f32) {
    if sink >= 0.75 {
        return;
    }
    for i in 0..=3 {
        let hip_x = cx - 3.4 + i as f32 * 2.2;
        let dir = if i < 2 { -1.0_f32 } else { 1.0_f32 };
        let phase = i as f32 * 0.25;
        let swing = ((leg + phase) * 2.0 * PI).sin();
        let foot_x = hip_x + dir * (2.8 + swing * 1.3);
        let foot_y = cy + 3.6 + (-swing).max(0.0) * 1.1;
        // Fill and contour are the same tone: a leg one pixel across cannot
        // carry both, and the leg is already the darkest thing on the crab.
        c.limb(
            [hip_x, cy + shell_ry * 0.5],
            [foot_x, foot_y],
            1.15,
            LEG_DARK,
            LEG_DARK,
        );
    }
}

fn draw_claws(c: &mut Canvas, cx: f32, cy: f32, shell_rx: f32, raise: f32, clamp: f32) {
    for side in [-1.0_f32, 1.0_f32] {
        let base_x = cx + side * (shell_rx + 0.6);
        let base_y = cy - 0.4 - raise * 3.4;
        let reach_x = base_x + side * 2.2;
        // The arm is thin so the pincer at the end of it is the wide part.
        c.limb(
            [cx + side * shell_rx * 0.7, cy - 0.2],
            [base_x + side * 1.4, base_y],
            1.1,
            SHELL_DARK,
            SHELL_DARK,
        );
        let gap = 1.2 + (1.0 - clamp) * 3.4;
        // Two prongs with daylight between them, which is the only way a pincer
        // reads as a pincer at this size.
        c.blob(reach_x, base_y - gap * 0.5, 2.4, 1.3, CLAW, RIM);
        c.blob(reach_x, base_y + gap * 0.5, 2.4, 1.3, CLAW, RIM);
        c.set(reach_x + side * 1.6, base_y - gap * 0.3, CLAW_TOOTH);
        c.set(reach_x + side * 1.6, base_y + gap * 0.3, CLAW_TOOTH);
        if clamp > 0.85 {
            c.spark(reach_x + side * 1.2, base_y, 1.0, SPARKLE);
        }
    }
}

fn draw_eyestalks(c: &mut Canvas, cx: f32, cy: f32, stalk: f32, eye: f32) {
    for side in [-1.0_f32, 1.0_f32] {
        let sx = cx + side * 2.1;
        let sy = cy - 2.8 - stalk * 1.6;
        c.line(sx, cy - 1.2, sx, sy + 1.2, RIM);
        c.line(sx, cy - 1.0, sx, sy + 1.4, SHELL);
        if eye > 0.3 {
            c.blob(sx, sy, 1.6, 1.6, EYE_WHITE, RIM);
            c.set(sx, sy, EYE_DARK);
            c.set(sx + 0.4, sy - 0.4, WHITE);
        } else {
            c.line(sx - 1.0, sy, sx + 1.0, sy, CONTOUR);
        }
    }
}

fn draw_sand_and_sleep(c: &mut Canvas, cx: f32, cy: f32, sink: f32, zzz: f32) {
    if sink > 0.05 {
        let mound_w = (4.0 + sink * 7.0).floor() as i32;
        let mound_y = 13_i32;
        let rows = (sink * 3.0).floor() as i32;
        // Two flat tones, not a per-pixel ramp: `shade` per column gave the mound
        // a distinct colour per pixel, which is a highlight group per pixel.
        for row in 0..=rows {
            let half = mound_w - row * 2;
            let tone = if row == 0 { SAND } else { SAND_DEEP };
            for dx in -half..=half {
                c.set(cx + dx as f32, (mound_y - row) as f32, tone);
            }
        }
    }
    if zzz > 0.05 {
        let rise = (zzz * 4.0).floor() as i32;
        for i in 0..=1 {
            let size = 2 - i;
            let zy = cy as i32 - 4 - i * 2 + rise;
            let zx = cx as i32 + 4 + i + (rise as f32 * 0.5) as i32;
            let tone = if i == 0 { ZZZ } else { ZZZ_FADE };
            c.line(zx as f32, zy as f32, (zx + size) as f32, zy as f32, tone);
            c.line(
                (zx + size) as f32,
                zy as f32,
                zx as f32,
                (zy + size) as f32,
                tone,
            );
            c.line(
                zx as f32,
                (zy + size) as f32,
                (zx + size) as f32,
                (zy + size) as f32,
                tone,
            );
        }
    }
}

pub fn draw(p: &Pose) -> Canvas {
    let mut c = Canvas::new(W, H);
    let cx = 12.0;
    // Seated low enough that the legs reach the bottom of the canvas: an asset's
    // cell footprint is its whole canvas, so empty rows underneath would float the
    // crab above the floor it is anchored to.
    let cy = 9.8 + p.bob * 0.6 + p.sink * 4.0;
    let shell_rx = 5.6;
    let shell_ry = 3.4;

    draw_legs(&mut c, cx, cy, shell_ry, p.leg, p.sink);
    draw_claws(&mut c, cx, cy, shell_rx, p.raise, p.clamp);

    c.blob(cx, cy, shell_rx, shell_ry, SHELL, RIM);

    c.blob(
        cx,
        cy + 0.5,
        shell_rx * 0.66,
        shell_ry * 0.52,
        SHELL_DARK,
        RIM,
    );

    c.set(cx - 2.0, cy - 1.2, SHELL_SPEC);
    c.set(cx - 1.0, cy - 1.4, WHITE);
    c.set(cx, cy - 1.4, WHITE);
    c.set(cx + 1.0, cy - 1.4, SHELL_SPEC);
    c.set(cx - 2.5, cy + 0.2, SHELL_GROOVE);
    c.set(cx + 2.5, cy + 0.2, SHELL_GROOVE);

    draw_eyestalks(&mut c, cx, cy, p.stalk, p.eye);
    draw_sand_and_sleep(&mut c, cx, cy, p.sink, p.zzz);

    c
}

pub fn build() -> SpriteSet {
    let mut set = SpriteSet::new(W, H);
    set.add(
        "idle",
        g::cycle(4, |t| {
            let b = (t * 2.0 * PI).sin();
            Pose {
                bob: b,
                clamp: 0.26 + 0.10 * b,
                raise: 0.0,
                stalk: 0.82 + 0.18 * b,
                leg: 0.0,
                eye: 1.0,
                ..Default::default()
            }
        }),
        draw,
    );
    set.add(
        "walk",
        g::cycle(4, |t| Pose {
            leg: t + 0.125,
            bob: 0.6 * (t * 4.0 * PI).sin(),
            clamp: 0.76,
            raise: 0.12,
            stalk: 0.95,
            eye: 1.0,
            ..Default::default()
        }),
        draw,
    );
    set.add(
        "walk_fast",
        g::cycle(4, |t| Pose {
            leg: t,
            bob: 1.0 * (t * 4.0 * PI).sin(),
            clamp: 0.85,
            raise: 0.15,
            stalk: 0.6,
            eye: 1.0,
            ..Default::default()
        }),
        draw,
    );
    set.add(
        "clip_claws",
        g::sequence(4, |t| {
            let beat = ((t * 1.5 * PI).sin()).abs();
            Pose {
                raise: 0.35 + t * 0.45 + 0.20 * beat,
                clamp: 1.0 - beat,
                stalk: 1.0 - t * 0.15,
                bob: 0.4 * beat,
                eye: 1.0,
                ..Default::default()
            }
        }),
        draw,
    );
    set.add(
        "burrow",
        g::sequence(5, |t| {
            let e = g::ease(t);
            Pose {
                sink: 0.14 + e * 0.86,
                stalk: 1.0 - e * 0.9,
                clamp: 0.4 + e * 0.6,
                raise: 0.30 * (1.0 - e),
                leg: t * 0.5,
                eye: 1.0 - e * 0.8,
                ..Default::default()
            }
        }),
        draw,
    );
    set.add(
        "sleep",
        g::cycle(4, |t| Pose {
            bob: 0.4 * (t * 2.0 * PI).sin(),
            clamp: 0.9,
            stalk: 0.18,
            eye: 0.0,
            zzz: t,
            ..Default::default()
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
        for state in ["idle", "walk", "walk_fast", "clip_claws", "burrow", "sleep"] {
            assert!(
                set.layout.contains_key(state),
                "crab layout is missing state '{state}'"
            );
        }
    }

    #[test]
    fn burrowing_hides_the_legs_and_raises_sand() {
        let standing = draw(&Pose::default());
        let buried = draw(&Pose {
            sink: 1.0,
            ..Default::default()
        });
        assert_ne!(standing.to_image().as_raw(), buried.to_image().as_raw());
    }
}
