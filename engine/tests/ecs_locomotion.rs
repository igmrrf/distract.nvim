//! Locomotion classes, and the capability gating that refuses a manifest
//! asking for movement its own art cannot show.
//!
//! `on_land` is here too: whether it fires is decided by the locomotion class,
//! and firing it once per touchdown rather than once per resting frame is the
//! part that is easy to get wrong.

use distract_engine::ecs::World;
use distract_engine::manifest;
use distract_engine::manifest::{
    AssetManifest, PhysicsConfig, StateDefinition, TransitionConfig, WrapMode,
};
use distract_engine::spawn::SpawnOptions;

/// One entity under `physics` and `transitions`, at one pixel per cell.
fn locomotion_world(physics: PhysicsConfig, transitions: TransitionConfig) -> World {
    let mut world = World::new(800.0, 600.0);
    world.sprite_scale_x = 1.0;
    world.sprite_scale_y = 1.0;
    // One pixel per cell, so the manifest's floor and the entity's position
    // are the same number and the assertions stay readable.
    world.cell_w = 1.0;
    world.cell_h = 1.0;

    let mut manifest = AssetManifest::default_cat();
    manifest.name = "jumper".to_string();
    manifest.initial_state = "flying".to_string();
    manifest.states.clear();
    manifest.states.insert(
        "flying".to_string(),
        StateDefinition {
            physics,
            transitions,
            ..Default::default()
        },
    );
    manifest
        .states
        .insert("landed".to_string(), StateDefinition::default());

    world
        .spawn("jumper", Some(manifest), SpawnOptions::at(100.0, 200.0))
        .expect("locomotion probe spawns");
    world
}

/// Physics that falls onto a floor 20 cells below the spawn point.
fn falling(locomotion: Option<&str>) -> PhysicsConfig {
    PhysicsConfig {
        gravity: 0.6,
        ground_y: Some(220.0),
        wrap_mode: WrapMode::None,
        locomotion: locomotion.map(str::to_string),
        ..Default::default()
    }
}

#[test]
fn spawning_a_manifest_that_breaks_its_own_capabilities_is_refused() {
    // Checked where the manifest arrives, not per frame: a manifest that
    // cannot work is worth one message when it lands, not thirty a second.
    let mut world = World::new(800.0, 600.0);
    let mut manifest = AssetManifest::default_cat();
    manifest.name = "impossible".to_string();
    manifest.initial_state = "orbit".to_string();
    manifest.locomotion = Some(manifest::GROUNDED.to_string());
    manifest.states.clear();
    manifest.states.insert(
        "orbit".to_string(),
        StateDefinition {
            physics: PhysicsConfig {
                path_type: Some("orbital".to_string()),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let error = world
        .spawn("impossible", Some(manifest), SpawnOptions::default())
        .expect_err("a grounded orbit cannot be drawn, so it must not spawn");
    assert!(
        error.contains("orbit"),
        "the refusal must name the offending state, got: {error}"
    );
    assert!(
        world.entities.is_empty(),
        "a refused spawn must leave no entity behind"
    );
}

#[test]
fn a_ballistic_entity_changes_state_when_it_touches_down() {
    // The cat's jump returns through the animation's `on_finish`, so today
    // it lands when the art happens to run out rather than when it reaches
    // the ground.
    let mut world = locomotion_world(
        falling(Some("ballistic")),
        TransitionConfig {
            on_land: Some("landed".to_string()),
            ..Default::default()
        },
    );

    for _ in 0..120 {
        world.update(1.0 / 60.0);
    }

    assert_eq!(
        world.entities[0].current_state, "landed",
        "a ballistic entity that reached its floor must fire on_land"
    );
}

#[test]
fn on_land_does_not_fire_again_while_the_entity_rests_on_the_floor() {
    // Gravity re-accelerates a resting entity every tick and the clamp
    // catches it again, so a landing test written against the clamp alone
    // fires forever.
    let mut world = locomotion_world(
        falling(Some("ballistic")),
        TransitionConfig {
            on_land: Some("landed".to_string()),
            ..Default::default()
        },
    );
    // Already at rest on the floor: nothing has just landed.
    world.entities[0].y = 220.0;
    world.entities[0].vy = 0.0;
    world.entities[0].current_state = "flying".to_string();

    for _ in 0..30 {
        world.update(1.0 / 60.0);
    }

    assert_eq!(
        world.entities[0].current_state, "flying",
        "sitting on the ground is not a landing"
    );
}

#[test]
fn a_grounded_entity_ignores_on_land() {
    let mut world = locomotion_world(
        falling(Some("grounded")),
        TransitionConfig {
            on_land: Some("landed".to_string()),
            ..Default::default()
        },
    );

    for _ in 0..120 {
        world.update(1.0 / 60.0);
    }

    assert_eq!(
        world.entities[0].current_state, "flying",
        "on_land belongs to ballistic locomotion, not to every floor"
    );
}

#[test]
fn an_omitted_locomotion_is_derived_from_gravity() {
    // No manifest in the wild sets `locomotion`, so the derived value is
    // what every existing asset actually runs under.
    assert_eq!(falling(None).effective_locomotion(), "grounded");
    assert_eq!(
        PhysicsConfig::default().effective_locomotion(),
        "omnidirectional"
    );
}
