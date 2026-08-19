//! How an entity moves: acceleration, gravity, boundaries and paths.
//!
//! The unit-level companion to the physics-parity fixtures, which pin whole
//! trajectories against the Lua engine. These assert the individual mechanisms
//! and the analytically checkable cases.

use distract_engine::ecs::World;
use distract_engine::manifest;
use distract_engine::manifest::{AssetManifest, PathParams, PhysicsConfig, WrapMode};
use distract_engine::spawn::SpawnOptions;

/// One entity whose only state runs `physics`, at one pixel per cell.
///
/// The scale is 1:1 so the assertions below can be written in manifest
/// units and read as the arithmetic they are.
fn path_world(physics: PhysicsConfig) -> World {
    let mut world = World::new(800.0, 600.0);
    world.sprite_scale_x = 1.0;
    world.sprite_scale_y = 1.0;

    let mut manifest = AssetManifest::default_cat();
    manifest.name = "pathprobe".to_string();
    manifest.initial_state = "idle".to_string();
    // Inherited from the cat, which walks. A probe that orbits does not, and
    // the capability gate is right to say so.
    manifest.locomotion = Some(manifest::OMNIDIRECTIONAL.to_string());
    manifest.capabilities = Default::default();
    if let Some(state) = manifest.states.get_mut("idle") {
        state.animation.frames = vec![0];
        state.physics = physics;
        state.transitions = Default::default();
    }

    world
        .spawn("pathprobe", Some(manifest), SpawnOptions::at(100.0, 200.0))
        .expect("path probe spawns");
    // Spawn desynchronises entities with a random phase, which is right for
    // two suns on screen and fatal for an analytic assertion.
    world.entities[0].path_phase = 0.0;
    world
}

/// Landing has to end the whole action, not only the state.
///
/// A golden trajectory cannot reach this: the parity fixtures describe
/// physics, and nothing in them triggers an action. The Lua engine carries
/// the same assertion by hand, which is the mitigation the harness's own
/// blind spot note asks for.
#[test]
fn landing_cancels_the_action_that_launched_the_jump() {
    let mut world = World::new(800.0, 600.0);
    world
        .spawn("cat", None, SpawnOptions::at(100.0, 200.0))
        .unwrap();
    world
        .trigger_action(Some(1), None, "jump")
        .expect("the cat declares a jump");

    assert!(
        world.entities[0].action_timer.is_some(),
        "the jump is pending"
    );

    for _ in 0..240 {
        world.update(1.0 / 60.0);
        if world.entities[0].current_state != "jump" {
            break;
        }
    }

    assert_eq!(
        world.entities[0].current_state, "idle",
        "the cat lands in idle"
    );
    assert!(
        world.entities[0].action_timer.is_none(),
        "a landing that leaves the timer running drags the cat back later"
    );
    assert!(
        !world.entities[0].is_locked,
        "a landed cat responds to the editor again"
    );
}

#[test]
fn test_bounce_wrap_mode() {
    let mut world = World::new(200.0, 200.0);
    world.sprite_scale_x = 1.0;
    world.sprite_scale_y = 1.0;
    let id = world
        .spawn("crab", None, SpawnOptions::at(190.0, 50.0))
        .unwrap();
    world.trigger_action(Some(id), None, "walk").unwrap();
    assert_eq!(world.entities[0].current_state, "walk");

    for _ in 0..10 {
        world.update(0.1);
    }

    assert!(world.entities[0].flip_x);
}

#[test]
fn test_gravity_jump_and_ground_collision() {
    let mut world = World::new(800.0, 600.0);
    let id = world
        .spawn("cat", None, SpawnOptions::at(100.0, 200.0))
        .unwrap();

    world.trigger_action(Some(id), None, "jump").unwrap();
    assert_eq!(world.entities[0].current_state, "jump");
    assert!(world.entities[0].vy < 0.0);
    assert_eq!(world.entities[0].ground_y, 200.0);

    for _ in 0..50 {
        world.update(0.05);
    }

    assert_eq!(world.entities[0].y, 200.0);
    assert_eq!(world.entities[0].vy, 0.0);
}

#[test]
fn test_sine_pathing_phase() {
    let mut world = World::new(800.0, 600.0);
    world
        .spawn("sun", None, SpawnOptions::at(100.0, 200.0))
        .unwrap();
    assert_eq!(world.entities[0].current_state, "shining");

    let before = world.entities[0].path_phase;
    world.update(0.5);
    assert!(world.entities[0].path_phase > before);
}

#[test]
fn wrap_recovers_an_entity_whose_velocity_decayed_off_screen() {
    // The old gate was `vx > 0 && x > viewport_w`. Park an entity past the
    // right edge with no velocity: it must still come back.
    let mut world = World::new(200.0, 200.0);
    world.sprite_scale_x = 1.0;
    world.sprite_scale_y = 1.0;
    world
        .spawn("cat", None, SpawnOptions::at(10.0, 10.0))
        .unwrap();
    world.entities[0].current_state = "walk".to_string();
    world.entities[0].x = 500.0;
    world.entities[0].vx = 0.0;
    world.entities[0].target_vx = 0.0;
    world.entities[0].heading_x = 0.0;

    world.update(0.016);
    assert!(
        world.entities[0].x < 200.0,
        "entity stayed off-screen at x={}",
        world.entities[0].x
    );
}

#[test]
fn accel_x_is_integrated_rather_than_ignored() {
    // `accel_x`/`accel_y` were in the manifest schema and read by nothing.
    let mut world = World::new(2000.0, 2000.0);
    world.sprite_scale_x = 1.0;
    world.sprite_scale_y = 1.0;

    let mut manifest = AssetManifest::default_cat();
    manifest.name = "thruster".to_string();
    if let Some(state) = manifest.states.get_mut("idle") {
        state.physics.target_vx = 0.0;
        state.physics.accel_x = 0.5;
        state.physics.wrap_mode = WrapMode::None;
        state.transitions.timeout_ms = None;
        state.transitions.on_timeout = None;
    }
    world
        .spawn("thruster", Some(manifest), SpawnOptions::at(0.0, 0.0))
        .unwrap();

    for _ in 0..30 {
        world.update(1.0 / 60.0);
    }
    assert!(
        world.entities[0].vx > 0.1,
        "constant acceleration must build velocity, got vx={}",
        world.entities[0].vx
    );
    assert!(world.entities[0].x > 0.0, "and must move the entity");
}

#[test]
fn accel_y_moves_an_entity_with_no_gravity_and_no_floor() {
    let mut world = World::new(2000.0, 2000.0);
    world.sprite_scale_x = 1.0;
    world.sprite_scale_y = 1.0;

    let mut manifest = AssetManifest::default_cat();
    manifest.name = "drifter".to_string();
    if let Some(state) = manifest.states.get_mut("idle") {
        state.physics.gravity = 0.0;
        state.physics.accel_y = -0.4;
        state.physics.path_type = None;
        state.physics.wrap_mode = WrapMode::None;
        state.transitions.timeout_ms = None;
        state.transitions.on_timeout = None;
    }
    world
        .spawn("drifter", Some(manifest), SpawnOptions::at(50.0, 500.0))
        .unwrap();

    for _ in 0..30 {
        world.update(1.0 / 60.0);
    }
    assert!(
        world.entities[0].y < 500.0,
        "accel_y must lift an entity that has no gravity, got y={}",
        world.entities[0].y
    );
}

#[test]
fn an_orbital_path_drives_the_x_axis_too() {
    let mut world = path_world(PhysicsConfig {
        path_type: Some("orbital".to_string()),
        path_params: Some(PathParams {
            freq: Some(0.0),
            amp_x: Some(12.0),
            amp_y: Some(5.0),
            ..Default::default()
        }),
        ..Default::default()
    });
    world.update(0.1);

    let e = &world.entities[0];
    assert!(
        (e.x - 112.0).abs() < 1e-4,
        "orbital must move x: cos(0) * 12 from base_x 100, got {}",
        e.x
    );
    assert!(
        (e.y - 200.0).abs() < 1e-4,
        "orbital y at phase 0 sits on base_y, got {}",
        e.y
    );
}

#[test]
fn a_lissajous_path_offsets_x_by_its_phase_delta() {
    let mut world = path_world(PhysicsConfig {
        path_type: Some("lissajous".to_string()),
        path_params: Some(PathParams {
            freq: Some(0.0),
            amp_x: Some(10.0),
            amp_y: Some(4.0),
            phase_delta: Some(std::f32::consts::FRAC_PI_2),
            ..Default::default()
        }),
        ..Default::default()
    });
    world.update(0.1);

    let e = &world.entities[0];
    assert!(
        (e.x - 110.0).abs() < 1e-4,
        "sin(0 + pi/2) * 10 from base_x 100, got {}",
        e.x
    );
    assert!(
        (e.y - 200.0).abs() < 1e-4,
        "phase_delta is an x-axis offset only, got y {}",
        e.y
    );
}

#[test]
fn a_bezier_path_starts_on_its_first_control_point() {
    let mut world = path_world(PhysicsConfig {
        path_type: Some("bezier".to_string()),
        path_params: Some(PathParams {
            freq: Some(0.0),
            points: Some(vec![[10.0, 20.0], [30.0, 0.0], [50.0, 40.0], [70.0, 5.0]]),
            ..Default::default()
        }),
        ..Default::default()
    });
    world.update(0.1);

    let e = &world.entities[0];
    assert!(
        (e.x - 110.0).abs() < 1e-4,
        "a cubic at t=0 is its first control point, got x {}",
        e.x
    );
    assert!(
        (e.y - 220.0).abs() < 1e-4,
        "control points are relative to the spawn position, got y {}",
        e.y
    );
}

#[test]
fn the_legacy_sine_fields_describe_the_same_curve_as_path_params() {
    let mut legacy = path_world(PhysicsConfig {
        path_type: Some("sine".to_string()),
        path_amplitude: Some(15.0),
        path_frequency: Some(2.0),
        ..Default::default()
    });
    let mut modern = path_world(PhysicsConfig {
        path_type: Some("sine".to_string()),
        path_params: Some(PathParams {
            amp_y: Some(15.0),
            freq_y: Some(2.0),
            ..Default::default()
        }),
        ..Default::default()
    });

    for _ in 0..20 {
        legacy.update(0.05);
        modern.update(0.05);
    }

    assert!(
        (legacy.entities[0].y - 200.0).abs() > 1e-3,
        "the fixture has to actually be moving for this to mean anything"
    );
    assert!(
        (legacy.entities[0].y - modern.entities[0].y).abs() < 1e-5,
        "path_amplitude/path_frequency must alias amp_y/freq_y exactly, \
         got {} vs {}",
        legacy.entities[0].y,
        modern.entities[0].y
    );
}

#[test]
fn a_linear_path_does_not_by_itself_keep_the_world_awake() {
    let world = path_world(PhysicsConfig {
        path_type: Some("linear".to_string()),
        ..Default::default()
    });
    assert!(
        world.is_quiescent(),
        "`linear` overrides no position, so it produces no new pictures"
    );
}
