//! Procedurally drawn crab sprite. Port of `lua/distract/sprites/crab.lua`.
//!
//! Same pose-function approach as the cat: a handful of scalars (claw opening,
//! leg phase, eyestalk height, how far it has sunk into the sand) drive one
//! draw routine, and each state samples them along a curve.

use std::f32::consts::PI;

use super::SpriteSet;
use crate::sprite_gen::{self as g, Canvas, OrbOpts, Rgb};

const W: u32 = 24;
const H: u32 = 16;

const SHELL: Rgb = [226, 62, 52];
const SHELL_DARK: Rgb = [158, 30, 26];
const CLAW: Rgb = [250, 116, 74];
const LEG: Rgb = [176, 40, 34];
const EYE_WHITE: Rgb = [248, 248, 252];
const EYE_DARK: Rgb = [26, 24, 32];
const SAND: Rgb = [198, 170, 122];
const ZZZ: Rgb = [186, 214, 255];

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

pub fn draw(p: &Pose) -> Canvas {
    let mut c = Canvas::new(W, H);

    let cx = 12.0;
    let cy = 8.4 + p.bob * 0.6 + p.sink * 4.0;
    let (shell_rx, shell_ry) = (5.6f32, 3.4f32);

    // Legs: four per side, stepping in counter-phase.
    if p.sink < 0.75 {
        for i in 0..4 {
            let hip_x = cx - 3.4 + i as f32 * 2.2;
            let dir: f32 = if i < 2 { -1.0 } else { 1.0 };
            let phase = i as f32 * 0.25;
            let swing = ((p.leg + phase) * 2.0 * PI).sin();
            let foot_x = hip_x + dir * (2.6 + swing * 1.3);
            let foot_y = cy + 3.6 + (-swing).max(0.0) * 1.1;
            c.limb(hip_x, cy + shell_ry * 0.5, foot_x, foot_y, 1.05, LEG);
        }
    }

    // Claws: an upper and lower pincer whose gap closes as clamp goes to 1.
    for side in [-1.0f32, 1.0] {
        let base_x = cx + side * (shell_rx + 0.6);
        let base_y = cy - 0.4 - p.raise * 3.4;
        let reach_x = base_x + side * 2.0;
        // Arm
        c.limb(
            cx + side * shell_rx * 0.7,
            cy - 0.2,
            base_x + side * 1.4,
            base_y,
            1.2,
            SHELL,
        );
        // Pincer halves swing apart around the arm axis.
        let gap = (1.0 - p.clamp) * 2.6;
        c.orb(
            reach_x,
            base_y - gap * 0.5 - 0.4,
            2.2,
            1.5,
            CLAW,
            &OrbOpts {
                ambient: Some(0.46),
                rim: Some(0.26),
                specular: Some(0.32),
                ..Default::default()
            },
        );
        c.orb(
            reach_x,
            base_y + gap * 0.5 + 0.4,
            2.2,
            1.5,
            g::shade(CLAW, -0.16),
            &OrbOpts {
                ambient: Some(0.46),
                rim: Some(0.20),
                specular: Some(0.24),
                ..Default::default()
            },
        );
    }

    // Shell, with a darker inner carapace band for depth.
    c.orb(
        cx,
        cy,
        shell_rx,
        shell_ry,
        SHELL,
        &OrbOpts {
            ambient: Some(0.34),
            rim: Some(0.30),
            specular: Some(0.34),
            ..Default::default()
        },
    );
    c.orb(
        cx,
        cy + 0.5,
        shell_rx * 0.66,
        shell_ry * 0.52,
        SHELL_DARK,
        &OrbOpts {
            ambient: Some(0.42),
            rim: Some(0.12),
            specular: Some(0.16),
            ..Default::default()
        },
    );

    // Eyestalks rise out of the shell and carry the eyes.
    for side in [-1.0f32, 1.0] {
        let sx = cx + side * 2.1;
        let top = cy - shell_ry - 1.0 - p.stalk * 2.0;
        c.limb(sx, cy - shell_ry * 0.6, sx, top + 0.6, 0.85, SHELL);
        if p.eye > 0.3 {
            // A single-pixel pupil: at this size a wider one swallows the white
            // and the eyestalk stops reading as an eye.
            c.orb(
                sx,
                top,
                1.4,
                1.4,
                EYE_WHITE,
                &OrbOpts {
                    ambient: Some(0.62),
                    rim: Some(0.30),
                    specular: Some(0.42),
                    ..Default::default()
                },
            );
            c.set(sx, top, EYE_DARK);
        } else {
            c.line(sx - 1.0, top, sx + 1.0, top, g::shade(SHELL_DARK, -0.25));
        }
    }

    // Sand mound rises over the crab as it burrows.
    if p.sink > 0.05 {
        let mound_w = (4.0 + p.sink * 7.0).floor() as i32;
        let mound_y = 13.0;
        for row in 0..=(p.sink * 3.0).floor() as i32 {
            let half = mound_w - row * 2;
            for dx in -half..=half {
                c.set(
                    cx + dx as f32,
                    mound_y - row as f32,
                    g::shade(SAND, -0.08 * row as f32 + 0.05 * (dx as f32 * 0.9).cos()),
                );
            }
        }
    }

    if p.zzz > 0.05 {
        let rise = (p.zzz * 4.0).floor();
        for i in 0..3 {
            let size = (3 - i) as f32;
            let zy = cy - shell_ry - 4.0 - (i * 2) as f32 + rise;
            let zx = cx + 5.0 + i as f32;
            let tone = g::shade(ZZZ, -0.12 * i as f32);
            c.line(zx, zy, zx + size, zy, tone);
            c.line(zx + size, zy, zx, zy + size, tone);
            c.line(zx, zy + size, zx + size, zy + size, tone);
        }
    }

    c
}

pub fn build() -> SpriteSet {
    let mut set = SpriteSet::new(W, H);

    // Idle: at rest with the claws held open and low, legs planted. The open
    // claws and grounded stance are what separate this from the first frame of
    // a walk.
    set.add(
        "idle",
        g::cycle(4, |t| Pose {
            bob: (t * 2.0 * PI).sin(),
            clamp: 0.26 + 0.10 * (t * 2.0 * PI).sin(),
            raise: 0.0,
            stalk: 0.82 + 0.18 * (t * 2.0 * PI).sin(),
            leg: 0.0,
            ..Default::default()
        }),
        draw,
    );

    // Scuttle: legs stepping, shell rocking with the gait, claws drawn in and
    // up out of the way. Starts mid-stride so it never opens on a planted pose.
    set.add(
        "walk",
        g::cycle(4, |t| Pose {
            leg: t + 0.125,
            bob: 0.6 * (t * 4.0 * PI).sin(),
            clamp: 0.76,
            raise: 0.12,
            stalk: 0.95,
            ..Default::default()
        }),
        draw,
    );

    // Fast scuttle: claws tucked in, body lower, harder rock.
    set.add(
        "walk_fast",
        g::cycle(4, |t| Pose {
            leg: t,
            bob: (t * 4.0 * PI).sin(),
            clamp: 0.85,
            raise: 0.15,
            stalk: 0.6,
            ..Default::default()
        }),
        draw,
    );

    // Clip: two snaps. `beat` alone repeats its values across the sequence,
    // which would make two frames identical, so the claws also climb steadily
    // throughout.
    set.add(
        "clip_claws",
        g::sequence(4, |t| {
            let beat = (t * 1.5 * PI).sin().abs();
            Pose {
                raise: 0.35 + t * 0.45 + 0.20 * beat,
                clamp: 1.0 - beat,
                stalk: 1.0 - t * 0.15,
                bob: 0.4 * beat,
                ..Default::default()
            }
        }),
        draw,
    );

    // Burrow: sinks into a rising sand mound, eyestalks retracting last. It
    // starts already breaking the sand so the opening pose cannot be mistaken
    // for a walk.
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

    // Sleep: settled, eyes shut, claws slack, Zzz drifting up.
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
            assert!(set.layout.contains_key(state), "missing state {}", state);
        }
    }

    #[test]
    fn burrowing_hides_the_legs_and_raises_sand() {
        let up = draw(&Pose::default()).to_image();
        let down = draw(&Pose {
            sink: 1.0,
            ..Default::default()
        })
        .to_image();
        assert_ne!(up, down);
    }
}
