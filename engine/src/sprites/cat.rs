//! Procedurally drawn cat sprite. Port of `lua/distract/sprites/cat.lua`.
//!
//! Every state is a pose function of a few scalars (body lift, leg phase, head
//! tilt, eye opening). Frames come from sampling those scalars, so animation is
//! smooth by construction and a new state is a new curve rather than a new set
//! of hand-drawn pixels.

use std::f32::consts::PI;

use super::SpriteSet;
use crate::sprite_gen::{self as g, Canvas, CelOrbOpts, Rgb};

const W: u32 = 24;
const H: u32 = 16;

const FUR: Rgb = [238, 142, 54];
const FUR_DARK: Rgb = [164, 76, 24];
const FUR_LIGHT: Rgb = [255, 186, 92];
const FUR_SPEC: Rgb = [255, 214, 140];
const CONTOUR: Rgb = [54, 28, 22];
const BELLY: Rgb = [254, 246, 238];
const BELLY_DARK: Rgb = [218, 202, 190];
const BELLY_SHADOW: Rgb = [184, 168, 156];
const PAW: Rgb = [255, 255, 255];
const PAW_SHADOW: Rgb = [204, 196, 192];
const NOSE: Rgb = [255, 140, 160];
const EAR_INNER: Rgb = [255, 172, 188];
const EAR_SHADOW: Rgb = [216, 128, 144];
const EAR_LIGHT: Rgb = [255, 204, 216];
const EYE: Rgb = [28, 24, 36];
const EYE_LIT: Rgb = [64, 224, 172];
const WHITE: Rgb = [255, 255, 255];
const MOUTH: Rgb = [188, 54, 72];
const MOUTH_DARK: Rgb = [132, 28, 44];
const ZZZ: Rgb = [176, 212, 255];
const ZZZ_FADE: Rgb = [140, 180, 235];

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

fn draw_tail(c: &mut Canvas, body_cx: f32, body_cy: f32, body_rx: f32, curl: f32, tail: f32) {
    let tail_base_x = body_cx - body_rx + 0.8;
    let opts = CelOrbOpts {
        shadow: Some(CONTOUR),
        highlight: Some(FUR),
        outline: Some(CONTOUR),
        outline_threshold: Some(0.82),
        ..Default::default()
    };
    for i in 1..=6 {
        let t = i as f32 / 6.0;
        let curve = (0.55 + tail * 0.45) * t * t;
        let tx = tail_base_x - t * (4.2 - curl * 1.6);
        let ty = body_cy - curve * (4.4 - curl * 2.6) + curl * 0.8;
        c.cel_orb(tx, ty, 1.45 - t * 0.6, 1.45 - t * 0.6, FUR_DARK, &opts);
    }
}

fn draw_legs(c: &mut Canvas, body_cx: f32, body_cy: f32, body_ry: f32, base_y: f32, p: &Pose) {
    if p.curl >= 0.6 {
        return;
    }
    let leg_opts = CelOrbOpts {
        shadow: Some(FUR_DARK),
        highlight: Some(FUR_LIGHT),
        outline: Some(CONTOUR),
        ..Default::default()
    };
    let paw_opts = CelOrbOpts {
        shadow: Some(PAW_SHADOW),
        highlight: Some(WHITE),
        outline: Some(CONTOUR),
        ..Default::default()
    };
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
            (((p.leg + phase) * 2.0 * PI + PI / 2.0).sin().max(0.0)) * (0.9 + p.stretch * 0.8);
        c.cel_limb(
            [hip_x, body_cy + body_ry * 0.6],
            [knee_x, foot_y - lifted],
            1.35,
            FUR,
            &leg_opts,
        );
        c.cel_orb(knee_x, foot_y - lifted, 1.5, 1.1, PAW, &paw_opts);
    }
}

fn draw_ears(c: &mut Canvas, head_cx: f32, head_cy: f32, head_r: f32, stretch: f32, mouth: f32) {
    let lean = -stretch * 0.3 + mouth * 0.2;
    for (ex, side) in [(head_cx - 1.8, -1.0_f32), (head_cx + 1.6, 1.0_f32)] {
        let top_x = ex + side * 0.6 + lean * 1.2;
        let top_y = head_cy - head_r - 2.2;
        c.triangle(
            [ex - 1.2, head_cy - head_r + 0.4],
            [ex + 1.2, head_cy - head_r + 0.4],
            [top_x, top_y],
            CONTOUR,
        );
        c.triangle(
            [ex - 0.8, head_cy - head_r + 0.2],
            [ex + 0.8, head_cy - head_r + 0.2],
            [top_x, top_y + 0.4],
            FUR,
        );
        c.triangle(
            [ex - 0.4, head_cy - head_r],
            [ex + 0.4, head_cy - head_r],
            [top_x, top_y + 0.8],
            EAR_INNER,
        );
        c.set(ex, head_cy - head_r + 0.3, EAR_SHADOW);
        c.set(ex + side * 0.3, top_y + 0.6, EAR_LIGHT);
    }
}

fn draw_head(c: &mut Canvas, head_cx: f32, head_cy: f32, head_r: f32, p: &Pose) {
    draw_ears(c, head_cx, head_cy, head_r, p.stretch, p.mouth);
    let head_opts = CelOrbOpts {
        shadow: Some(FUR_DARK),
        highlight: Some(FUR_LIGHT),
        outline: Some(CONTOUR),
        rim: Some(0.2),
        rim_color: Some(FUR_SPEC),
        ..Default::default()
    };
    c.cel_orb(head_cx, head_cy, head_r, head_r * 0.94, FUR, &head_opts);
    let belly_opts = CelOrbOpts {
        shadow: Some(BELLY_DARK),
        highlight: Some(WHITE),
        outline: Some(CONTOUR),
        ..Default::default()
    };
    c.cel_orb(head_cx + 1.1, head_cy + 1.3, 1.7, 1.1, BELLY, &belly_opts);
    c.set(head_cx + 0.8, head_cy + 1.6, BELLY_SHADOW);
    for ex in [head_cx - 1.1, head_cx + 1.7] {
        if p.eye > 0.3 {
            c.set(ex, head_cy - 0.5, EYE);
            c.set(ex, head_cy - 1.5, EYE_LIT);
            c.set(ex + 0.5, head_cy - 1.5, WHITE);
        } else {
            c.line(ex - 1.0, head_cy - 0.8, ex + 1.0, head_cy - 0.8, CONTOUR);
        }
    }
    c.set(head_cx + 1.1, head_cy + 0.7, NOSE);
    if p.curl < 0.6 {
        c.line(
            head_cx + 2.2,
            head_cy + 0.9,
            head_cx + 4.6,
            head_cy + 0.4,
            BELLY_DARK,
        );
        c.line(
            head_cx + 2.2,
            head_cy + 1.5,
            head_cx + 4.6,
            head_cy + 2.0,
            BELLY_DARK,
        );
    }
    if p.mouth > 0.04 {
        c.ellipse(
            head_cx + 1.2,
            head_cy + 1.7 + p.mouth * 0.5,
            0.8 + p.mouth * 0.8,
            0.5 + p.mouth * 1.0,
            MOUTH,
        );
        c.set(head_cx + 1.2, head_cy + 1.8 + p.mouth * 0.5, MOUTH_DARK);
    }
}

fn draw_sleep(c: &mut Canvas, head_cx: f32, head_cy: f32, zzz: f32) {
    if zzz <= 0.05 {
        return;
    }
    let rise = (zzz * 4.0).floor() as i32;
    for i in 0..=1 {
        let size = 2 - i;
        let zy = head_cy as i32 - 3 - i * 2 + rise;
        let zx = head_cx as i32 + 3 + i + (rise as f32 * 0.5) as i32;
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

pub fn draw(p: &Pose) -> Canvas {
    let mut c = Canvas::new(W, H);
    let base_y = 12.0 - p.lift * 3.0 + p.curl * 1.5;
    let body_cx = 10.0 + p.stretch * 0.8;
    let body_cy = base_y - 2.6 + p.curl * 1.2;
    let body_rx = 6.0 + p.stretch * 1.4 + p.curl * 1.0;
    let body_ry = 3.4 - p.stretch * 0.5 - p.curl * 0.7;

    draw_tail(&mut c, body_cx, body_cy, body_rx, p.curl, p.tail);
    draw_legs(&mut c, body_cx, body_cy, body_ry, base_y, p);

    let body_opts = CelOrbOpts {
        shadow: Some(FUR_DARK),
        highlight: Some(FUR_LIGHT),
        outline: Some(CONTOUR),
        rim: Some(0.25),
        rim_color: Some(FUR_SPEC),
        ..Default::default()
    };
    c.cel_orb(body_cx, body_cy, body_rx, body_ry, FUR, &body_opts);
    let bib_opts = CelOrbOpts {
        shadow: Some(BELLY_DARK),
        highlight: Some(WHITE),
        outline: Some(CONTOUR),
        ..Default::default()
    };
    c.cel_orb(
        body_cx + 0.4,
        body_cy + body_ry * 0.45,
        body_rx * 0.68,
        body_ry * 0.44,
        BELLY,
        &bib_opts,
    );
    c.set(body_cx + 0.2, body_cy + body_ry * 0.8, BELLY_SHADOW);
    c.set(body_cx - 1.0, body_cy - body_ry + 0.8, FUR_SPEC);
    c.set(body_cx, body_cy - body_ry + 0.6, WHITE);

    let head_cx = body_cx + body_rx * 0.92 + p.stretch * 1.0;
    let head_cy = body_cy - 3.4 + p.head_dip * 1.6 + p.curl * 2.0;
    let head_r = 2.9;

    draw_head(&mut c, head_cx, head_cy, head_r, p);
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
