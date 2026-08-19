//! Cross-engine physics parity, Rust half.
//!
//! The recurring defect class in this project is the Lua and Rust engines
//! drifting apart while both file headers claim "one manifest describes one
//! behaviour on both backends". Three such divergences were found and fixed by
//! hand; this is the harness that stops the next one reaching a user.
//!
//! Rust produces the golden trajectories and this test asserts it still
//! reproduces them, so a change to `World::update` that alters behaviour fails
//! here. `tests/physics_parity_spec.lua` asserts the *Lua* engine reproduces
//! the same numbers. Neither suite can pass while the two disagree, and neither
//! needs the other's runtime.
//!
//! Regenerate after an intentional behaviour change:
//!
//! ```sh
//! UPDATE_GOLDEN=1 cargo test --manifest-path engine/Cargo.toml --test physics_parity
//! ```
//!
//! Frame timing is part of the same contract. `frame_duration_seconds` exists
//! in both engines with the same precedence rule (`animation.fps` wins, else the
//! source file's per-frame delay, else 0.1s) and had no fixture guarding it. A
//! fixture declaring an `animation` block exercises it; the recorded
//! `sheet_index` is the atlas frame each engine would actually draw, which is
//! free of the 0-based/1-based `frame_idx` convention the two ports differ on.
//!
//! The middle branch needs art that carries delays, so a fixture may also
//! declare a `spritesheet`. `tests/fixtures/physics/frame_delays.gif` is a
//! 24x16 four-frame GIF whose delays are deliberately all different and all
//! unequal to the 0.1s fallback, so a run that ignored them cannot land on the
//! same trajectory by coincidence. Regenerate it with:
//!
//! ```sh
//! magick -size 24x16 -delay 4 xc:'#e03030' -delay 12 xc:'#30c040' \
//!        -delay 8 xc:'#3050d0' -delay 20 xc:'#e0d040' -loop 0 \
//!        tests/fixtures/physics/frame_delays.gif
//! ```
//!
//! GIF stores a delay in centiseconds, which is why those are 40/120/80/200ms.
//!
//! Trajectories are stored in **terminal cells**, not pixels. Lua integrates in
//! cells (`CELLS_PER_SPRITE_PX_X = 1.0`, `_Y = 0.5`) and Rust in pixels
//! (`scale_x = cell_w`, `scale_y = cell_h / 2`), so dividing x by `cell_w` and
//! y by `cell_h` puts both in the same frame with no fudge factor.

use distract_engine::ecs::World;
use distract_engine::manifest::{AssetManifest, PhysicsConfig, StateDefinition, TransitionConfig};
use distract_engine::obstacles::{Obstacle, ObstacleKind};
use distract_engine::spawn::SpawnOptions;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Spawn {
    x: f32,
    y: f32,
    #[serde(default)]
    flip_x: bool,
    /// Depth damping, applied to the entity directly.
    ///
    /// Set after the spawn on both sides, exactly as `path_phase` is: a
    /// fixture describes what the *engine* is given, not the `position`
    /// configuration and backend capabilities that would have produced it.
    #[serde(default)]
    parallax: Option<f32>,
}

/// The animation a fixture wants on the probe state.
///
/// Absent on every physics fixture: a multi-frame loop makes the world
/// permanently non-quiescent and would mask a disagreement in that rule.
/// Present only on the fixtures whose subject *is* frame timing.
#[derive(Deserialize)]
struct FixtureAnimation {
    frames: Vec<usize>,
    /// Zero means "the manifest declares no rate", which is what sends
    /// `frame_duration_seconds` down its fallback path. Rust stores `fps` as a
    /// plain `f32` and cannot express absence any other way.
    fps: f32,
    #[serde(default = "loops_by_default")]
    loop_anim: bool,
}

fn loops_by_default() -> bool {
    true
}

/// Imported art the probe is bound to.
///
/// Present only on the fixture whose subject is the per-frame delays a source
/// file carries: a procedural probe has none, so that branch of
/// `frame_duration_seconds` cannot be reached without one. The path is relative
/// to the repository root, which is the one form both harness runners can
/// resolve -- Lua against the plugin root, this side against
/// `CARGO_MANIFEST_DIR`'s parent.
#[derive(Deserialize)]
struct FixtureSpritesheet {
    path: String,
    /// Declared so both engines resample to the same footprint. 24x16 keeps the
    /// probe the size an unbound probe already is, so boundary handling stays
    /// comparable with every other fixture.
    frame_width: u32,
    frame_height: u32,
}

/// One obstacle a fixture registers, in terminal cells.
///
/// Converted to pixels by this runner and read as cells by the Lua one, exactly
/// as `external.lua` and `engine.lua` treat a provider's rectangles. Absent on
/// every fixture whose subject is not obstacle physics.
#[derive(Deserialize)]
struct FixtureObstacle {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    #[serde(rename = "type")]
    kind: ObstacleKind,
}

#[derive(Deserialize)]
struct Cell {
    w: f32,
    h: f32,
}

/// The rectangle the fixture's entity may move in, in terminal cells.
///
/// `col` and `row` are absent on almost every fixture, which means "the whole
/// editor grid" and is what boundary handling measured against before a viewport
/// could be scoped. Present, they describe a rectangle inside a larger window —
/// what `positioning.scope = "buffer"` produces — and the window is sized to
/// contain it on both sides so the two engines clamp against the same edges.
#[derive(Deserialize)]
struct Bounds {
    columns: f32,
    lines: f32,
    #[serde(default)]
    col: f32,
    #[serde(default)]
    row: f32,
}

impl Bounds {
    fn is_scoped(&self) -> bool {
        self.col > 0.0 || self.row > 0.0
    }
}

#[derive(Deserialize)]
struct Fixture {
    #[serde(default)]
    #[allow(dead_code)]
    description: String,
    physics: PhysicsConfig,
    /// Transitions the probe state may fire.
    ///
    /// Empty for almost every fixture: a transition firing mid-run swaps in
    /// another state's physics and the trajectory stops describing the fixture.
    /// `on_land` is the exception, since its whole subject is *when* the state
    /// changes, which is precisely what could drift between the engines.
    #[serde(default)]
    transitions: TransitionConfig,
    spawn: Spawn,
    cell: Cell,
    bounds: Bounds,
    /// The floor, in terminal cells, pushed into the world before the spawn.
    ///
    /// Each engine derives the entity's own floor from it by subtracting the
    /// sprite height in its own units, which is arithmetic duplicated on both
    /// sides and therefore worth pinning.
    #[serde(default)]
    ground_row: Option<f32>,
    #[serde(default)]
    obstacles: Vec<FixtureObstacle>,
    #[serde(default)]
    animation: Option<FixtureAnimation>,
    #[serde(default)]
    spritesheet: Option<FixtureSpritesheet>,
    dt: f32,
    steps: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Sample {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    flip_x: bool,
    state: String,
    /// Whether the world can still change without further input.
    ///
    /// Tracked per step because the Lua side is a hand-written mirror of
    /// `World::is_quiescent`, which is exactly the kind of duplication that has
    /// drifted here before.
    quiescent: bool,
    /// The atlas frame this engine would draw at this step.
    ///
    /// Recorded rather than `frame_idx` because Lua indexes `animation.frames`
    /// from 1 and Rust from 0. Comparing the resolved sheet index compares what
    /// reaches the screen, so the convention difference cannot fail the test and
    /// a real timing drift still can.
    sheet_index: usize,
    animation_finished: bool,
}

/// Rust reproducing its own goldens is exact arithmetic, so the tolerance here
/// only absorbs JSON's decimal round-trip. The Lua side carries the wider one.
const TOLERANCE: f32 = 1e-5;

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is `engine/`; fixtures are shared with the Lua suite.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("engine/ has a parent")
        .to_path_buf()
}

fn fixture_dir() -> PathBuf {
    repo_root().join("tests/fixtures/physics")
}

/// The probe manifest one fixture describes.
///
/// Both engines start from the cat's manifest with one state's physics
/// replaced, so the sprite dimensions that boundary handling depends on
/// (24x16 sprite pixels) are identical on both sides by construction. Lua
/// falls back to the cat's art for an unregistered asset and lands on the same
/// numbers.
fn probe_manifest(fixture: &Fixture) -> AssetManifest {
    let mut manifest = AssetManifest::default_cat();
    manifest.name = "parity_probe".to_string();
    // The fixtures describe physics, not an animal. Inheriting the cat's
    // declaration would have the capability gate refuse the orbiting ones,
    // which is right for a cat and wrong for a probe.
    manifest.locomotion = None;
    manifest.capabilities = Default::default();

    if let Some(ref sheet) = fixture.spritesheet {
        manifest.spritesheet.path =
            Some(repo_root().join(&sheet.path).to_string_lossy().into_owned());
        manifest.spritesheet.frame_width = Some(sheet.frame_width);
        manifest.spritesheet.frame_height = Some(sheet.frame_height);
    }

    let state = manifest
        .states
        .get_mut("idle")
        .expect("the cat manifest has an idle state");
    state.physics = fixture.physics.clone();
    state.transitions = fixture.transitions.clone();
    match fixture.animation {
        Some(ref animation) => {
            state.animation.frames = animation.frames.clone();
            state.animation.fps = animation.fps;
            state.animation.loop_anim = animation.loop_anim;
        }
        // The cat's own idle animation is multi-frame, which alone would make
        // the world permanently non-quiescent and mask any disagreement in the
        // rule. The Lua runner uses a single frame, so this one must too.
        None => {
            state.animation.loop_anim = true;
            state.animation.frames = vec![0];
        }
    }

    // A landing target for `on_land`, defined identically on both sides so the
    // state a fixture lands in has the same animation, physics and quiescence
    // whichever engine ran it.
    manifest
        .states
        .insert("landed".to_string(), StateDefinition::default());

    manifest
}

/// Runs one fixture and returns its trajectory, in cells.
fn run(fixture: &Fixture) -> Vec<Sample> {
    let manifest = probe_manifest(fixture);

    // Captured before the manifest moves into the world, so the recorded step
    // can resolve the drawn frame the same way the compositor does.
    let frames_by_state: std::collections::HashMap<String, Vec<usize>> = manifest
        .states
        .iter()
        .map(|(state_name, def)| (state_name.clone(), def.animation.frames.clone()))
        .collect();

    let mut world = World::new(
        (fixture.bounds.col + fixture.bounds.columns) * fixture.cell.w,
        (fixture.bounds.row + fixture.bounds.lines) * fixture.cell.h,
    );
    if fixture.bounds.is_scoped() {
        world
            .set_scope(Some(distract_engine::bounds::Bounds {
                left: fixture.bounds.col * fixture.cell.w,
                top: fixture.bounds.row * fixture.cell.h,
                width: fixture.bounds.columns * fixture.cell.w,
                height: fixture.bounds.lines * fixture.cell.h,
            }))
            .expect("the fixture's scope fits the window it sized");
    }
    world.sprite_scale_x = fixture.cell.w;
    world.sprite_scale_y = fixture.cell.h / 2.0;
    world.cell_w = fixture.cell.w;
    world.cell_h = fixture.cell.h;

    if let Some(ground_row) = fixture.ground_row {
        world.set_ground_y(ground_row * fixture.cell.h);
    }

    if !fixture.obstacles.is_empty() {
        world
            .set_obstacles(
                fixture
                    .obstacles
                    .iter()
                    .map(|obstacle| Obstacle {
                        x: obstacle.x * fixture.cell.w,
                        y: obstacle.y * fixture.cell.h,
                        width: obstacle.width * fixture.cell.w,
                        height: obstacle.height * fixture.cell.h,
                        kind: obstacle.kind,
                    })
                    .collect(),
            )
            .expect("a fixture registers fewer obstacles than the cap");
    }

    world
        .spawn(
            "parity_probe",
            Some(manifest),
            SpawnOptions {
                x: Some(fixture.spawn.x * fixture.cell.w),
                y: Some(fixture.spawn.y * fixture.cell.h),
                flip_x: Some(fixture.spawn.flip_x),
                ..SpawnOptions::default()
            },
        )
        .expect("parity probe spawns");

    // Spawn deliberately desynchronises entities from one another with random
    // frame and phase offsets, which is right for two cats on screen and fatal
    // for a reproducible trajectory. Zero them on both sides.
    let entity = &mut world.entities[0];
    entity.path_phase = 0.0;
    entity.frame_idx = 0;
    entity.frame_timer = 0.0;
    entity.state_time = 0.0;
    if let Some(parallax) = fixture.spawn.parallax {
        entity.parallax = parallax;
    }

    let mut trajectory = Vec::with_capacity(fixture.steps);
    for _ in 0..fixture.steps {
        world.update(fixture.dt);
        let quiescent = world.is_quiescent();
        let e = &world.entities[0];
        let frames = frames_by_state
            .get(&e.current_state)
            .expect("the recorded state is declared on the probe manifest");
        assert!(
            !frames.is_empty(),
            "a probe state must declare at least one frame"
        );
        trajectory.push(Sample {
            x: e.x / fixture.cell.w,
            y: e.y / fixture.cell.h,
            vx: e.vx,
            vy: e.vy,
            flip_x: e.flip_x,
            state: e.current_state.clone(),
            quiescent,
            sheet_index: frames[e.frame_idx % frames.len()],
            animation_finished: e.animation_finished,
        });
    }
    trajectory
}

#[test]
fn rust_physics_matches_the_golden_trajectories() {
    let dir = fixture_dir();
    let update = std::env::var("UPDATE_GOLDEN").is_ok();

    let mut cases: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|e| e == "json")
                && !p.to_string_lossy().ends_with(".golden.json")
        })
        .collect();
    cases.sort();

    assert!(!cases.is_empty(), "no fixtures found in {}", dir.display());

    for case in cases {
        let name = case.file_stem().unwrap().to_string_lossy().to_string();
        let raw = std::fs::read_to_string(&case).expect("fixture readable");
        let fixture: Fixture = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("fixture {name} does not parse: {e}"));

        let actual = run(&fixture);
        let golden_path = dir.join(format!("{name}.golden.json"));

        if update {
            std::fs::write(
                &golden_path,
                serde_json::to_string_pretty(&actual).expect("trajectory serialises"),
            )
            .expect("golden writable");
            continue;
        }

        let golden_raw = std::fs::read_to_string(&golden_path).unwrap_or_else(|_| {
            panic!(
                "no golden for {name}. Generate with \
                 UPDATE_GOLDEN=1 cargo test --manifest-path engine/Cargo.toml --test physics_parity"
            )
        });
        let expected: Vec<Sample> =
            serde_json::from_str(&golden_raw).expect("golden trajectory parses");

        assert_eq!(
            expected.len(),
            actual.len(),
            "{name}: golden has {} steps, run produced {}",
            expected.len(),
            actual.len()
        );

        for (i, (want, got)) in expected.iter().zip(actual.iter()).enumerate() {
            for (field, w, g) in [
                ("x", want.x, got.x),
                ("y", want.y, got.y),
                ("vx", want.vx, got.vx),
                ("vy", want.vy, got.vy),
            ] {
                assert!(
                    (w - g).abs() <= TOLERANCE,
                    "{name} step {i}: {field} drifted, golden {w} vs {g}"
                );
            }
            assert_eq!(want.flip_x, got.flip_x, "{name} step {i}: flip_x diverged");
            assert_eq!(want.state, got.state, "{name} step {i}: state diverged");
            assert_eq!(
                want.quiescent, got.quiescent,
                "{name} step {i}: quiescence diverged"
            );
            assert_eq!(
                want.sheet_index, got.sheet_index,
                "{name} step {i}: the drawn frame diverged"
            );
            assert_eq!(
                want.animation_finished, got.animation_finished,
                "{name} step {i}: animation_finished diverged"
            );
        }
    }
}

/// Guards against the harness's own blind spot.
///
/// Every golden is generated from Rust, which makes Rust the reference
/// implementation by construction: if Rust is wrong, both suites agree on the
/// wrong answer. These assertions are computed by hand from the manifest units
/// rather than from either engine.
#[test]
fn analytically_checkable_cases_match_hand_computed_values() {
    let dir = fixture_dir();
    let raw = std::fs::read_to_string(dir.join("constant_velocity_wrap.json"))
        .expect("constant velocity fixture readable");
    let fixture: Fixture = serde_json::from_str(&raw).expect("fixture parses");
    let trajectory = run(&fixture);

    // Velocity is seeded to target_vx at spawn and lerps toward the same value,
    // so it is constant. One step advances vx * (dt * 60) * 1 cell.
    let per_step = fixture.physics.target_vx * (fixture.dt * 60.0);
    let expected_x = fixture.spawn.x + per_step * 10.0;
    assert!(
        (trajectory[9].x - expected_x).abs() < 1e-3,
        "after 10 steps x should be {expected_x} cells, got {}",
        trajectory[9].x
    );

    // Vertical position must not move at all: no gravity, no target_vy, no path.
    assert!(
        (trajectory[9].y - fixture.spawn.y).abs() < 1e-6,
        "horizontal-only motion moved y to {}",
        trajectory[9].y
    );
}
