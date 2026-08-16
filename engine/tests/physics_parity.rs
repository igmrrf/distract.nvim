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
//! Trajectories are stored in **terminal cells**, not pixels. Lua integrates in
//! cells (`CELLS_PER_SPRITE_PX_X = 1.0`, `_Y = 0.5`) and Rust in pixels
//! (`scale_x = cell_w`, `scale_y = cell_h / 2`), so dividing x by `cell_w` and
//! y by `cell_h` puts both in the same frame with no fudge factor.

use distract_engine::ecs::World;
use distract_engine::manifest::{AssetManifest, PhysicsConfig, StateDefinition, TransitionConfig};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Spawn {
    x: f32,
    y: f32,
    #[serde(default)]
    flip_x: bool,
}

#[derive(Deserialize)]
struct Cell {
    w: f32,
    h: f32,
}

#[derive(Deserialize)]
struct Bounds {
    columns: f32,
    lines: f32,
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
}

/// Rust reproducing its own goldens is exact arithmetic, so the tolerance here
/// only absorbs JSON's decimal round-trip. The Lua side carries the wider one.
const TOLERANCE: f32 = 1e-5;

fn fixture_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR is `engine/`; fixtures are shared with the Lua suite.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("engine/ has a parent")
        .join("tests/fixtures/physics")
}

/// Runs one fixture and returns its trajectory, in cells.
///
/// Both engines start from the cat's manifest with one state's physics
/// replaced, so the sprite dimensions that boundary handling depends on
/// (24x16 sprite pixels) are identical on both sides by construction. Lua
/// falls back to the cat's art for an unregistered asset and lands on the same
/// numbers.
fn run(fixture: &Fixture) -> Vec<Sample> {
    let mut manifest = AssetManifest::default_cat();
    manifest.name = "parity_probe".to_string();

    let state = manifest
        .states
        .get_mut("idle")
        .expect("the cat manifest has an idle state");
    state.physics = fixture.physics.clone();
    state.transitions = fixture.transitions.clone();
    state.animation.loop_anim = true;
    // The cat's own idle animation is multi-frame, which alone would make the
    // world permanently non-quiescent and mask any disagreement in the rule.
    // The Lua runner uses a single frame, so this one must too.
    state.animation.frames = vec![0];

    // A landing target for `on_land`, defined identically on both sides so the
    // state a fixture lands in has the same animation, physics and quiescence
    // whichever engine ran it.
    manifest
        .states
        .insert("landed".to_string(), StateDefinition::default());

    let mut world = World::new(
        fixture.bounds.columns * fixture.cell.w,
        fixture.bounds.lines * fixture.cell.h,
    );
    world.sprite_scale_x = fixture.cell.w;
    world.sprite_scale_y = fixture.cell.h / 2.0;
    world.cell_w = fixture.cell.w;
    world.cell_h = fixture.cell.h;

    world
        .spawn(
            "parity_probe",
            Some(manifest),
            Some(fixture.spawn.x * fixture.cell.w),
            Some(fixture.spawn.y * fixture.cell.h),
            Some(fixture.spawn.flip_x),
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

    let mut trajectory = Vec::with_capacity(fixture.steps);
    for _ in 0..fixture.steps {
        world.update(fixture.dt);
        let quiescent = world.is_quiescent();
        let e = &world.entities[0];
        trajectory.push(Sample {
            x: e.x / fixture.cell.w,
            y: e.y / fixture.cell.h,
            vx: e.vx,
            vy: e.vy,
            flip_x: e.flip_x,
            state: e.current_state.clone(),
            quiescent,
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
