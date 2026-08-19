//! Procedurally drawn cat sprite. Port of `lua/distract/sprites/cat.lua`.
//!
//! Every state is a pose function of a few scalars (body lift, leg phase, head
//! tilt, eye opening). Frames come from sampling those scalars, so animation is
//! smooth by construction and a new state is a new curve rather than a new set
//! of hand-drawn pixels.

use std::f32::consts::PI;

use super::SpriteSet;
use crate::sprite_gen::{self as g, Canvas, Rgb};

const W: u32 = 24;
const H: u32 = 16;

// Flat, banded palette. Mirrors `lua/distract/sprites/cat.lua`. At 24x16 a sprite
// is 24 columns by eight half-block rows, and the five lighting terms this asset
// used to spend across a twelve-pixel body read as noise rather than as form: the
// cat read as a fox.
const CONTOUR: Rgb = [40, 26, 30];
const FUR: Rgb = [236, 146, 60];
const FUR_DARK: Rgb = [174, 96, 34];
const BELLY: Rgb = [252, 240, 226];
const EAR_INNER: Rgb = [240, 150, 168];
const NOSE: Rgb = [236, 118, 142];
const EYE: Rgb = [32, 28, 40];
const ZZZ: Rgb = [168, 206, 250];

/// The rim is a darker fur tone rather than the near-black contour. A near-black
/// outline is the right choice on a light page and the wrong one here: the editor
/// background is dark, so a dark rim merges into it and the silhouette loses its
/// edge -- the rendered cat looked like it had bites taken out of it. CONTOUR is
/// kept for the accents that must read as holes: eyes and an open mouth.
const RIM: Rgb = FUR_DARK;

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

/// The cat's primary motion cue, so it is drawn thick enough to read.
///
/// Five segments, not six. The sixth drew nothing at all -- at `i = 6` its centre
/// landed off the canvas's left edge with a radius under a pixel, and any sliver
/// was already covered by the fifth.
///
/// Contours first, then fills, so one segment's outline cannot be painted over the
/// next segment's body and leave a dark seam down the tail.
fn draw_tail(c: &mut Canvas, body_cx: f32, body_cy: f32, body_rx: f32, curl: f32, tail: f32) {
    let base_x = body_cx - body_rx + 1.4;
    let base_y = body_cy - 0.8;
    let mut segments = [[0.0_f32; 3]; 5];
    for index in 1..=5 {
        let t = index as f32 / 5.0;
        let rise = (0.5 + tail * 0.5) * t;
        segments[index - 1] = [
            base_x - t * (4.2 - curl * 3.0),
            base_y - rise * (4.6 - curl * 4.2) + curl * 2.0,
            1.7 - t * 0.2,
        ];
    }
    for segment in &segments {
        c.ellipse(segment[0], segment[1], segment[2], segment[2], RIM);
    }
    for segment in &segments {
        // One pixel inside the contour, which at this radius is a plus rather
        // than a square: a solid fill would leave the tail all outline and no fur.
        c.ellipse(
            segment[0],
            segment[1],
            segment[2] - 1.0,
            segment[2] - 1.0,
            FUR,
        );
    }
}

/// Four legs in two distinguishable pairs.
///
/// The hind pair is short and thick under the haunch, the fore pair thinner and
/// longer under the chest, and they swing half a cycle apart. Four identical
/// capsules was the other half of why the silhouette read as a fox.
fn draw_legs(c: &mut Canvas, body_cx: f32, body_cy: f32, body_ry: f32, base_y: f32, p: &Pose) {
    if p.curl >= 0.6 {
        return;
    }

    let reach = 1.8 + p.stretch * 1.6;
    // The hip sits at the body's lower edge and the foot on the floor, so the legs
    // are drawn *below* the barrel rather than inside it. They were inside it,
    // which is why a rendered cat had no legs at all.
    let hip_y = body_cy + body_ry - 0.4;

    for (hip_x, width, phase) in [
        (body_cx - 3.0, 2.0_f32, 0.0_f32),
        (body_cx - 1.4, 2.0, 0.5),
        (body_cx + 2.0, 2.0, 0.5),
        (body_cx + 3.4, 2.0, 0.0),
    ] {
        let cycle = (p.leg + phase) * 2.0 * PI;
        let raise = (cycle + PI * 0.5).sin().max(0.0) * (0.7 + p.lift * 1.8);
        let foot_x = hip_x + cycle.sin() * reach;
        let foot_y = base_y - raise;
        let span = (foot_y - hip_y).floor().max(1.0);

        let mut step = 0.0;
        while step <= span {
            let along = step / span;
            c.rect(
                (hip_x + (foot_x - hip_x) * along).floor(),
                hip_y + step,
                width,
                1.0,
                FUR_DARK,
            );
            step += 1.0;
        }
        c.rect(foot_x.floor(), foot_y.floor(), width + 1.0, 1.0, BELLY);
    }
}

/// Two upright ears with a gap between them.
///
/// Three pixels wide and three tall, contoured, with one pink pixel inside. The
/// old pair were 2.4-pixel triangles that read as a single fuzzy line.
fn draw_ears(c: &mut Canvas, head_cx: f32, head_cy: f32, head_r: f32, curl: f32) {
    let tuck = curl * 1.4;
    for ex in [head_cx - 2.2, head_cx + 1.8] {
        let base = head_cy - head_r * 0.8 + tuck;
        let tip = base - 2.9 + tuck;
        c.triangle([ex - 1.4, base], [ex + 1.4, base], [ex, tip], RIM);
        c.triangle(
            [ex - 0.9, base - 0.7],
            [ex + 0.9, base - 0.7],
            [ex, tip + 1.1],
            FUR,
        );
        c.set(ex, base - 1.5, EAR_INNER);
    }
}

fn draw_eyes(c: &mut Canvas, head_cx: f32, head_cy: f32, eye: f32) {
    for ex in [head_cx - 0.8, head_cx + 1.4] {
        if eye > 0.3 {
            c.set(ex, head_cy - 0.4, EYE);
        } else {
            c.set(ex, head_cy - 0.4, CONTOUR);
            c.set(ex + 1.0, head_cy - 0.4, CONTOUR);
        }
    }
}

fn draw_head(c: &mut Canvas, head_cx: f32, head_cy: f32, head_r: f32, p: &Pose) {
    draw_ears(c, head_cx, head_cy, head_r, p.curl);
    c.blob(head_cx, head_cy, head_r, head_r * 0.92, FUR, RIM);
    // Muzzle: one light band, not a modelled snout. It is what tells the head
    // which way it faces.
    c.ellipse(head_cx + 1.1, head_cy + 1.2, 1.2, 0.6, BELLY);
    c.set(head_cx + 2.0, head_cy + 0.9, NOSE);
    draw_eyes(c, head_cx, head_cy, p.eye);
    if p.mouth > 0.04 {
        c.ellipse(
            head_cx + 1.4,
            head_cy + 1.9,
            0.6 + p.mouth * 0.7,
            0.5 + p.mouth * 0.9,
            CONTOUR,
        );
    }
}

fn draw_sleep(c: &mut Canvas, head_cx: f32, head_cy: f32, zzz: f32) {
    if zzz <= 0.05 {
        return;
    }
    let rise = (zzz * 3.0).floor();
    for index in 0..=1 {
        let size = 2.0 - index as f32;
        let zy = head_cy - 4.0 - index as f32 * 2.0 + rise;
        let zx = head_cx + 3.0 + index as f32 + rise;
        c.line(zx, zy, zx + size, zy, ZZZ);
        c.line(zx + size, zy, zx, zy + size, ZZZ);
        c.line(zx, zy + size, zx + size, zy + size, ZZZ);
    }
}

pub fn draw(p: &Pose) -> Canvas {
    let mut c = Canvas::new(W, H);
    // Laid out so the whole canvas is used: ear tips on the top row, head above
    // the shoulder, body across the middle, paws on the floor row. An asset's
    // cell footprint is its whole canvas, so empty rows at the bottom would float
    // the cat above the floor it is anchored to.
    let base_y = 15.0 - p.lift * 2.6 + p.curl * 1.2;
    let body_cx = 9.0 + p.stretch * 0.6;
    let body_cy = base_y - 5.4 + p.curl * 1.6;
    let body_rx = 4.9 + p.stretch * 1.1 + p.curl * 0.9;
    let body_ry = 2.3 - p.stretch * 0.25 - p.curl * 0.3;

    draw_tail(&mut c, body_cx, body_cy, body_rx, p.curl, p.tail);
    draw_legs(&mut c, body_cx, body_cy, body_ry, base_y, p);

    // Haunch first, so the barrel's contour closes over where the two meet: a
    // cat's rear is its most recognisable line after the ears and the tail.
    c.blob(body_cx - body_rx * 0.6, body_cy + 0.2, 2.5, 2.5, FUR, RIM);
    c.blob(body_cx, body_cy, body_rx, body_ry, FUR, RIM);
    // One band, one row tall. A thicker one read as a cream stripe down a sausage
    // rather than as a belly.
    c.ellipse(
        body_cx + 0.4,
        body_cy + body_ry - 1.2,
        body_rx * 0.5,
        0.6,
        BELLY,
    );

    let head_cx = body_cx + body_rx * 0.92 + p.stretch * 0.9;
    let head_cy = body_cy - 4.6 + p.head_dip * 1.6 + p.curl * 3.2;
    draw_head(&mut c, head_cx, head_cy, 2.3, p);

    if p.curl >= 0.6 {
        draw_sleep(&mut c, head_cx, head_cy, p.zzz);
    }

    c
}

pub fn build() -> SpriteSet {
    let mut set = SpriteSet::new(W, H);
    set.add(
        "idle",
        g::cycle(4, |t| Pose {
            lift: 0.04 + 0.04 * (t * 2.0 * PI).sin(),
            tail: (t * 2.0 * PI).sin() * 0.8,
            head_dip: 0.10 * (t * 2.0 * PI).sin(),
            eye: 1.0,
            ..Default::default()
        }),
        draw,
    );
    set.add(
        "walk",
        g::cycle(4, |t| Pose {
            leg: t,
            lift: 0.10 + 0.08 * (t * 2.0 * PI).sin().abs(),
            stretch: 0.12,
            tail: (t * 2.0 * PI).sin() * 0.6,
            eye: 1.0,
            ..Default::default()
        }),
        draw,
    );
    set.add(
        "walk_fast",
        g::cycle(4, |t| Pose {
            leg: t,
            lift: 0.16 + 0.14 * (t * 2.0 * PI).sin().abs(),
            stretch: 0.85,
            head_dip: 0.30,
            tail: 0.9 - 0.25 * (t * 2.0 * PI).sin(),
            eye: 1.0,
            ..Default::default()
        }),
        draw,
    );
    set.add(
        "jump",
        g::sequence(8, |t| {
            let crouch = (1.0 - t / 0.34).max(0.0).powi(2);
            let arc = (((t - 0.12) / 0.88).max(0.0) * PI).sin();
            Pose {
                lift: arc - crouch * 0.26,
                stretch: 0.30 + arc * 0.5 - crouch * 0.18,
                leg: 0.25 + arc * 0.2,
                head_dip: 0.22 * crouch - 0.45 * arc,
                tail: -0.5 + arc * 1.2,
                eye: 1.0,
                ..Default::default()
            }
        }),
        draw,
    );
    set.add(
        "yawn",
        g::sequence(5, |t| {
            let open = (g::ease(t) * PI).sin();
            Pose {
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
            assert!(
                set.layout.contains_key(state),
                "cat layout is missing state '{state}'"
            );
        }
    }

    #[test]
    fn shut_eyes_change_the_sleeping_frame() {
        let awake = draw(&Pose::default());
        let asleep = draw(&Pose {
            curl: 1.0,
            eye: 0.0,
            ..Default::default()
        });
        assert_ne!(awake.to_image().as_raw(), asleep.to_image().as_raw());
    }

    #[test]
    fn sleeping_and_idle_draw_different_art() {
        let set = build();
        let idle_first = &set.frames[set.layout["idle"][0]];
        let sleep_first = &set.frames[set.layout["sleep"][0]];
        assert_ne!(idle_first.as_raw(), sleep_first.as_raw());
    }
}
