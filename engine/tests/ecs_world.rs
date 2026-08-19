//! The world's own lifecycle: spawning, despawning, editor events, actions,
//! and when it is allowed to stop redrawing.
//!
//! Quiescence is the substance. A world nothing is moving in must stop asking
//! for frames, and a world with an animation running must not -- getting that
//! wrong is either a pet that freezes or a redraw every tick forever.

use distract_engine::ecs::{EventContext, World};
use distract_engine::entity::{Entity, Rng};
use distract_engine::manifest::{AssetManifest, WrapMode};
use distract_engine::spawn::{EntitySeed, SpawnOptions};

fn plain(event: &str) -> EventContext {
    let _ = event;
    EventContext::default()
}

#[test]
fn test_entity_creation_and_state_change() {
    let mut ent = Entity::new(
        1,
        "cat".to_string(),
        EntitySeed {
            initial_state: "idle".to_string(),
            x: 10.0,
            y: 20.0,
            flip_x: false,
            z_index: 0,
            z: 0.0,
            parallax: 1.0,
        },
    );
    assert_eq!(ent.id, 1);
    assert_eq!(ent.current_state, "idle");
    assert_eq!(ent.state_time, 0.0);

    ent.state_time = 5.0;
    ent.frame_idx = 2;
    ent.set_state("jump".to_string());
    assert_eq!(ent.current_state, "jump");
    assert_eq!(ent.state_time, 0.0);
    assert_eq!(ent.frame_idx, 0);
}

#[test]
fn test_world_spawn_and_despawn() {
    let mut world = World::new(800.0, 600.0);
    let id1 = world
        .spawn("cat", None, SpawnOptions::at(10.0, 20.0))
        .unwrap();
    let id2 = world
        .spawn("crab", None, SpawnOptions::at(50.0, 60.0))
        .unwrap();
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(world.entities.len(), 2);

    assert!(world.despawn(id1));
    assert_eq!(world.entities.len(), 1);
    assert!(!world.despawn(999));
}

#[test]
fn test_world_clear_all() {
    let mut world = World::new(800.0, 600.0);
    world.spawn("cat", None, SpawnOptions::default()).unwrap();
    world.spawn("crab", None, SpawnOptions::default()).unwrap();
    world.spawn("sun", None, SpawnOptions::default()).unwrap();
    assert_eq!(world.entities.len(), 3);

    world.clear_all();
    assert_eq!(world.entities.len(), 0);
}

#[test]
fn test_editor_event_transitions() {
    let mut world = World::new(800.0, 600.0);
    world.spawn("cat", None, SpawnOptions::default()).unwrap();
    world.spawn("crab", None, SpawnOptions::default()).unwrap();

    world.handle_editor_event("typing", plain("typing"));
    assert_eq!(world.entities[0].current_state, "walk_fast");
    assert_eq!(world.entities[1].current_state, "walk_fast");

    world.handle_editor_event("scrolling", plain("scrolling"));
    assert_eq!(world.entities[0].current_state, "yawn");
    assert_eq!(world.entities[1].current_state, "clip_claws");
}

#[test]
fn test_timeout_transition() {
    let mut world = World::new(800.0, 600.0);
    world.spawn("cat", None, SpawnOptions::default()).unwrap();
    assert_eq!(world.entities[0].current_state, "idle");

    world.update(7.0);
    assert_eq!(world.entities[0].current_state, "sleep");
}

#[test]
fn test_action_dispatch_errors() {
    let mut world = World::new(800.0, 600.0);
    world.spawn("cat", None, SpawnOptions::default()).unwrap();

    assert!(
        world
            .trigger_action(None, Some("cat"), "nonexistent_action")
            .is_err()
    );
    assert!(world.trigger_action(Some(999), None, "jump").is_err());
}

#[test]
fn test_get_summaries() {
    let mut world = World::new(800.0, 600.0);
    world
        .spawn("cat", None, SpawnOptions::at(123.0, 456.0))
        .unwrap();
    let summaries = world.get_summaries();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].asset_name, "cat");
    assert_eq!(summaries[0].x, 123.0);
    assert_eq!(summaries[0].y, 456.0);
}

#[test]
fn test_multi_entity_action_dispatch() {
    let mut world = World::new(800.0, 600.0);
    world
        .spawn("cat", None, SpawnOptions::at(50.0, 200.0))
        .unwrap();
    world
        .spawn("cat", None, SpawnOptions::at(150.0, 200.0))
        .unwrap();
    world
        .spawn("crab", None, SpawnOptions::at(300.0, 200.0))
        .unwrap();

    let triggered = world.trigger_action(None, Some("cat"), "jump").unwrap();
    assert_eq!(triggered.len(), 2);
    assert_eq!(world.entities[0].current_state, "jump");
    assert_eq!(world.entities[1].current_state, "jump");
    assert_eq!(world.entities[2].current_state, "idle");
}

#[test]
fn update_reports_entities_it_despawns() {
    let mut world = World::new(200.0, 200.0);
    world.sprite_scale_x = 1.0;
    world.sprite_scale_y = 1.0;
    let mut manifest = AssetManifest::default_cat();
    manifest.name = "runner".to_string();
    if let Some(state) = manifest.states.get_mut("idle") {
        state.physics.wrap_mode = WrapMode::Despawn;
        state.physics.target_vx = 40.0;
        state.transitions.timeout_ms = None;
        state.transitions.on_timeout = None;
    }
    let id = world
        .spawn("runner", Some(manifest), SpawnOptions::at(190.0, 50.0))
        .unwrap();

    let mut reported = Vec::new();
    for _ in 0..40 {
        reported.extend(world.update(0.1));
    }

    assert_eq!(reported, vec![id], "despawn must be reported to Neovim");
    assert!(world.entities.is_empty());
}

#[test]
fn spawn_desynchronises_identical_entities() {
    let mut world = World::new(800.0, 600.0);
    for _ in 0..8 {
        world
            .spawn("cat", None, SpawnOptions::at(10.0, 10.0))
            .unwrap();
    }
    let phases: std::collections::HashSet<u32> = world
        .entities
        .iter()
        .map(|e| (e.path_phase * 1000.0) as u32)
        .collect();
    assert!(phases.len() > 1, "all entities share one path phase");

    let timers: std::collections::HashSet<u32> = world
        .entities
        .iter()
        .map(|e| (e.frame_timer * 100_000.0) as u32)
        .collect();
    assert!(timers.len() > 1, "all entities share one frame timer");
}

#[test]
fn entities_turn_toward_the_cursor_when_they_react() {
    let mut world = World::new(800.0, 600.0);
    world.cell_w = 10.0;
    world
        .spawn("cat", None, SpawnOptions::at(400.0, 300.0))
        .unwrap();

    // Cursor far to the left: a cat that starts walking should face it.
    world.handle_editor_event(
        "moving",
        EventContext {
            cursor_col: Some(2.0),
            cursor_row: Some(1.0),
        },
    );
    assert_eq!(world.entities[0].current_state, "walk");
    assert_eq!(world.entities[0].heading_x, -1.0);
    assert!(world.entities[0].flip_x);
}

#[test]
fn spawn_surfaces_a_broken_manifest_instead_of_degrading() {
    let mut world = World::new(800.0, 600.0);
    let mut manifest = AssetManifest::default_cat();
    manifest.name = "broken".to_string();
    manifest.asset_type = "sprite".to_string();
    manifest.spritesheet.path = Some("/nowhere/at/all.png".to_string());

    let err = world
        .spawn("broken", Some(manifest), SpawnOptions::default())
        .unwrap_err();
    assert!(err.contains("not found"), "unexpected message: {}", err);
}

#[test]
fn rng_desynchronises_adjacent_seeds() {
    let a = Rng::new(1).next_u64();
    let b = Rng::new(2).next_u64();
    assert_ne!(a, b);
}

#[test]
fn a_still_state_with_no_animation_is_quiescent() {
    let mut world = World::new(800.0, 600.0);
    assert!(world.is_quiescent(), "an empty world has nothing to draw");

    let mut manifest = AssetManifest::default_cat();
    manifest.name = "statue".to_string();
    manifest.initial_state = "idle".to_string();
    if let Some(state) = manifest.states.get_mut("idle") {
        state.animation.frames = vec![0];
        state.physics.target_vx = 0.0;
        state.physics.path_type = None;
        state.transitions.timeout_ms = None;
    }
    world
        .spawn("statue", Some(manifest), SpawnOptions::at(10.0, 10.0))
        .unwrap();
    world.entities[0].vx = 0.0;
    world.entities[0].vy = 0.0;
    assert!(world.is_quiescent());
}

#[test]
fn an_animating_entity_is_not_quiescent() {
    let mut world = World::new(800.0, 600.0);
    world
        .spawn("cat", None, SpawnOptions::at(10.0, 10.0))
        .unwrap();
    assert!(!world.is_quiescent());
}
