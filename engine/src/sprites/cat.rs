//! Procedurally drawn cat sprite. Port of `lua/distract/sprites/cat.lua`.
//!
//! Every state is a pose function of a few scalars (body lift, leg phase, head
//! tilt, eye opening). Frames come from sampling those scalars, so animation is
//! smooth by construction and a new state is a new curve rather than a new set
//! of hand-drawn pixels.

use std::f32::consts::PI;

use super::SpriteSet;
use crate::sprite_gen::{self as g, Canvas, LimbOpts, OrbOpts, Rgb};

const W: u32 = 24;
const H: u32 = 16;

const FUR: Rgb = [236, 142, 56];
const FUR_DARK: Rgb = [176, 92, 28];
const BELLY: Rgb = [252, 226, 196];
const PAW: Rgb = [250, 244, 236];
const NOSE: Rgb = [255, 154, 176];
const EYE: Rgb = [38, 34, 46];
const EYE_LIT: Rgb = [126, 232, 214];
const ZZZ: Rgb = [186, 214, 255];
const MOUTH: Rgb = [122, 46, 62];

/// One cat pose.
#[derive(Debug, Clone, Copy)]
pub struct Pose {
    /// 0..1 body raised off the ground.
    pub lift: f32,
    /// 0..1 phase of the four-beat gait.
    pub leg: f32,
    /// 0..1 body extended forward (sprint / mid-air).
    pub stretch: f32,
    /// -1..1 head lowered (+) or raised (-).
    pub head_dip: f32,
    /// 0..1 eye opening, 0 shut.
    pub eye: f32,
    /// 0..1 mouth opening for the yawn.
    pub mouth: f32,
    /// 0..1 curled up asleep.
    pub curl: f32,
    /// -1..1 tail sway.
    pub tail: f32,
    /// 0..1 sleep marks fading in.
    pub zzz: f32,
}

impl Default for Pose {
    fn default() -> Self {
        Self {
            lift: 0.0,
            leg: 0.0,
            stretch: 0.0,
            head_dip: 0.0,
            eye: 1.0,
            mouth: 0.0,
            curl: 0.0,
            tail: 0.0,
            zzz: 0.0,
        }
    }
}

pub fn draw(p: &Pose) -> Canvas {
    let mut c = Canvas::new(W, H);

    // Ground line drops as the body lifts; curling flattens the whole silhouette.
    let base_y = 12.0 - p.lift * 3.0 + p.curl * 1.5;
    let body_cx = 10.0 + p.stretch * 0.8;
    let body_cy = base_y - 2.6 + p.curl * 1.2;
    let body_rx = 6.0 + p.stretch * 1.4 + p.curl * 1.0;
    let body_ry = 3.4 - p.stretch * 0.5 - p.curl * 0.7;

    // Tail: sweeps back from the hip and curls upward. The horizontal reach is
    // kept strictly leftward so the tail always clears the body silhouette
    // instead of curling back inside it.
    let tail_base_x = body_cx - body_rx + 0.8;
    let tail_opts = OrbOpts {
        ambient: Some(0.42),
        rim: Some(0.20),
        specular: Some(0.12),
        ..Default::default()
    };
    for i in 1..=6 {
        let t = i as f32 / 6.0;
        let curve = (0.55 + p.tail * 0.45) * t * t;
        let tx = tail_base_x - t * (4.2 - p.curl * 1.6);
        let ty = body_cy - curve * (4.4 - p.curl * 2.6) + p.curl * 0.8;
        let r = 1.45 - t * 0.6;
        c.orb(tx, ty, r, r, FUR_DARK, &tail_opts);
    }

    // Legs: two pairs in counter-phase, so the gait reads as a four-beat walk.
    if p.curl < 0.6 {
        for (hip_x, phase) in [
            (body_cx - 3.2, 0.5),
            (body_cx + 3.0, 0.0),
            (body_cx - 1.6, 0.0),
            (body_cx + 4.4, 0.5),
        ] {
            let swing = ((p.leg + phase) * 2.0 * PI).sin();
            let knee_x = hip_x + swing * (1.6 + p.stretch * 1.4);
            let foot_y = base_y + 2.4 - p.lift * 1.2 - p.curl * 2.2;
            let lifted =
                ((p.leg + phase) * 2.0 * PI + PI / 2.0).sin().max(0.0) * (0.9 + p.stretch * 0.8);
            c.limb_with(
                hip_x,
                body_cy + body_ry * 0.6,
                knee_x,
                foot_y - lifted,
                1.35,
                FUR,
                &LimbOpts::default(),
            );
            c.orb(
                knee_x,
                foot_y - lifted,
                1.5,
                1.1,
                PAW,
                &OrbOpts {
                    ambient: Some(0.58),
                    rim: Some(0.20),
                    specular: Some(0.16),
                    ..Default::default()
                },
            );
        }
    }

    // Body, then a lighter belly band to suggest a second surface.
    c.orb(
        body_cx,
        body_cy,
        body_rx,
        body_ry,
        FUR,
        &OrbOpts {
            ambient: Some(0.36),
            rim: Some(0.26),
            ..Default::default()
        },
    );
    c.orb(
        body_cx + 0.4,
        body_cy + body_ry * 0.45,
        body_rx * 0.68,
        body_ry * 0.44,
        BELLY,
        &OrbOpts {
            ambient: Some(0.52),
            rim: Some(0.10),
            specular: Some(0.14),
            ..Default::default()
        },
    );

    // Head sits forward of and above the body. It is deliberately smaller than
    // the body and lifted clear of it, otherwise the two orbs merge into one
    // loaf-shaped silhouette with no readable neck.
    let head_cx = body_cx + body_rx * 0.92 + p.stretch * 1.0;
    let head_cy = body_cy - 3.4 + p.head_dip * 1.6 + p.curl * 2.0;
    let head_r = 2.9;

    // Ears: triangles that taper to a point at the top and sit *on* the skull.
    // The run widens as it descends -- widening upward would render a solid slab
    // across the head -- and the base overlaps the head orb, because a gap there
    // makes the pair read as antlers rather than ears.
    const EAR_HALF: [i32; 3] = [0, 1, 1];
    for (ex, lean) in [(head_cx - 1.6, -1.0f32), (head_cx + 1.6, 1.0)] {
        for row in 0..3i32 {
            let half = EAR_HALF[row as usize];
            for dx in -half..=half {
                c.set(
                    ex + dx as f32 + lean * (2 - row) as f32 * 0.35,
                    head_cy - head_r - 1.1 + row as f32,
                    g::shade(FUR_DARK, -0.18 + row as f32 * 0.14),
                );
            }
        }
    }

    c.orb(
        head_cx,
        head_cy,
        head_r,
        head_r * 0.94,
        FUR,
        &OrbOpts {
            ambient: Some(0.38),
            rim: Some(0.30),
            ..Default::default()
        },
    );
    // Muzzle
    c.orb(
        head_cx + 1.1,
        head_cy + 1.3,
        1.7,
        1.1,
        BELLY,
        &OrbOpts {
            ambient: Some(0.56),
            rim: Some(0.14),
            specular: Some(0.20),
            ..Default::default()
        },
    );

    // Eyes: mostly dark with a small bright catchlight. Filling the eye with the
    // lit colour instead reads as a pair of goggles at this size.
    for ex in [head_cx - 1.1, head_cx + 1.7] {
        if p.eye > 0.25 {
            c.set(ex, head_cy - 0.5, EYE);
            c.set(ex, head_cy - 1.5, if p.eye > 0.7 { EYE_LIT } else { EYE });
        } else {
            c.line(
                ex - 1.0,
                head_cy - 0.8,
                ex + 1.0,
                head_cy - 0.8,
                g::shade(FUR_DARK, -0.40),
            );
        }
    }

    // Nose, and a mouth that opens for the yawn.
    c.set(head_cx + 1.1, head_cy + 0.7, NOSE);
    // Threshold kept low so the mouth shrinks out of existence rather than
    // popping off in one frame.
    if p.mouth > 0.04 {
        c.ellipse(
            head_cx + 1.2,
            head_cy + 1.7 + p.mouth * 0.5,
            0.6 + p.mouth * 0.8,
            0.4 + p.mouth * 1.0,
            MOUTH,
        );
    }

    // Sleep marks drift up and to the right as they fade in. The rise is scaled
    // so each frame of the sleep cycle moves them a whole pixel; a smaller step
    // would round two neighbouring frames to identical art.
    if p.zzz > 0.05 {
        let rise = (p.zzz * 4.0).floor();
        for i in 0..3 {
            let size = (3 - i) as f32;
            let zy = head_cy - head_r - 2.0 - (i * 2) as f32 + rise;
            let zx = head_cx + 3.0 + i as f32;
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

    // Idle: slow breathing, a gentle tail sway, blinking held open.
    set.add(
        "idle",
        g::cycle(4, |t| Pose {
            lift: 0.04 + 0.04 * (t * 2.0 * PI).sin(),
            tail: (t * 2.0 * PI).sin() * 0.8,
            head_dip: 0.10 * (t * 2.0 * PI).sin(),
            ..Default::default()
        }),
        draw,
    );

    // Walk: four-beat gait with a slight body bob.
    set.add(
        "walk",
        g::cycle(4, |t| Pose {
            leg: t,
            lift: 0.10 + 0.08 * (t * 2.0 * PI).sin().abs(),
            stretch: 0.12,
            tail: (t * 2.0 * PI).sin() * 0.6,
            ..Default::default()
        }),
        draw,
    );

    // Sprint: body lower and longer, legs reaching further, tail streamed back.
    set.add(
        "walk_fast",
        g::cycle(4, |t| Pose {
            leg: t,
            lift: 0.16 + 0.14 * (t * 2.0 * PI).sin().abs(),
            stretch: 0.85,
            head_dip: 0.30,
            tail: 0.9 - 0.25 * (t * 2.0 * PI).sin(),
            ..Default::default()
        }),
        draw,
    );

    // Jump: crouch, launch, apex, fall, land. The crouch decays smoothly into
    // the sine arc rather than switching over at a threshold, which would put a
    // jump cut between the first two frames.
    set.add(
        "jump",
        g::sequence(8, |t| {
            let crouch = (1.0f32 - t / 0.34).max(0.0).powi(2);
            let arc = (((t - 0.12) / 0.88).max(0.0) * PI).sin();
            Pose {
                lift: arc - crouch * 0.26,
                stretch: 0.30 + arc * 0.5 - crouch * 0.18,
                leg: 0.25 + arc * 0.2,
                head_dip: 0.22 * crouch - 0.45 * arc,
                tail: -0.5 + arc * 1.2,
                ..Default::default()
            }
        }),
        draw,
    );

    // Yawn: mouth opens and closes while the eyes squeeze shut. The mouth arc
    // alone returns to the same value on the way down as on the way up, which
    // would make two frames identical, so the head tips and the tail sweeps
    // monotonically through the whole yawn to keep every frame distinct.
    set.add(
        "yawn",
        g::sequence(5, |t| {
            let open = (g::ease(t) * PI).sin();
            Pose {
                // A yawn is a whole-body stretch, not just a mouth. Moving the
                // body too keeps every frame doing something; with only the
                // mouth animating, the frame where it finally shuts is the one
                // big change in an otherwise static run and reads as a cut.
                lift: 0.06 + 0.30 * open,
                stretch: 0.34 * open,
                leg: 0.12 * open,
                head_dip: 0.22 - 0.70 * t - 0.45 * open,
                mouth: open,
                eye: 1.0 - open * 0.95,
                tail: -0.45 + 1.3 * t + 0.3 * open,
                ..Default::default()
            }
        }),
        draw,
    );

    // Sleep: curled, breathing, with Zzz drifting upward.
    set.add(
        "sleep",
        g::cycle(4, |t| Pose {
            curl: 1.0,
            lift: 0.02 * (t * 2.0 * PI).sin(),
            head_dip: 0.55 + 0.10 * (t * 2.0 * PI).sin(),
            eye: 0.0,
            tail: -0.6,
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
        for state in ["idle", "walk", "walk_fast", "jump", "yawn", "sleep"] {
            assert!(set.layout.contains_key(state), "missing state {}", state);
        }
    }

    #[test]
    fn sleeping_and_idle_draw_different_art() {
        // The exact failure the port exists to remove: on the old four-frame
        // overlay set these two states could resolve to the same picture.
        let set = build();
        let idle = &set.frames[set.layout["idle"][0]];
        let sleep = &set.frames[set.layout["sleep"][0]];
        assert_ne!(idle, sleep);
    }

    #[test]
    fn shut_eyes_change_the_sleeping_frame() {
        let open = draw(&Pose::default()).to_image();
        let shut = draw(&Pose {
            eye: 0.0,
            ..Default::default()
        })
        .to_image();
        assert_ne!(open, shut);
    }
}
