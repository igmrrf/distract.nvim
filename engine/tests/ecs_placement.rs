//! Where a spawn lands, and what the terminal's cell size does to it.
//!
//! Anchoring, the pushed floor, an explicit position, draw order, parallax and
//! the sprite scale. Moved out of `ecs.rs` when that module was decomposed; every
//! item asserted on is part of the crate's public surface.

use distract_engine::ecs::{DEFAULT_CELL_H, DEFAULT_CELL_W, World};
use distract_engine::manifest::{AssetManifest, PhysicsConfig, StateDefinition};
use distract_engine::spawn::{Anchor, SpawnOptions};

/// The floor an entity of this asset would stand on, in overlay pixels.
fn resting_y(world: &World, asset_name: &str, ground_y: f32) -> f32 {
    let asset = world
        .asset_manager
        .get(asset_name)
        .expect("built-in asset is registered");
    ground_y - asset.frame_h as f32 * world.sprite_scale_y
}

#[test]
fn bottom_anchored_spawn_stands_on_the_pushed_floor() {
    let mut world = World::new(800.0, 600.0);
    world.set_ground_y(400.0);
    world
        .spawn(
            "cat",
            None,
            SpawnOptions {
                anchor: Some(Anchor::Bottom),
                ..SpawnOptions::default()
            },
        )
        .unwrap();

    let expected = resting_y(&world, "cat", 400.0);
    assert_eq!(world.entities[0].y, expected);
    assert_eq!(world.entities[0].ground_y, expected);
}

#[test]
fn top_anchored_spawn_starts_at_the_viewport_top() {
    let mut world = World::new(800.0, 600.0);
    world.set_ground_y(400.0);
    world
        .spawn(
            "cat",
            None,
            SpawnOptions {
                anchor: Some(Anchor::Top),
                ..SpawnOptions::default()
            },
        )
        .unwrap();

    assert_eq!(world.entities[0].y, 0.0);
    // The anchor says where it starts, not what it falls to.
    assert_eq!(
        world.entities[0].ground_y,
        resting_y(&world, "cat", 400.0),
        "a top-anchored entity still owns the floor it will land on"
    );
}

#[test]
fn an_explicit_position_wins_over_the_anchor() {
    let mut world = World::new(800.0, 600.0);
    world.set_ground_y(400.0);
    world
        .spawn(
            "cat",
            None,
            SpawnOptions {
                y: Some(42.0),
                anchor: Some(Anchor::Bottom),
                ..SpawnOptions::default()
            },
        )
        .unwrap();

    assert_eq!(world.entities[0].y, 42.0);
}

#[test]
fn spawning_without_a_floor_leaves_the_entity_standing_where_it_spawned() {
    let mut world = World::new(800.0, 600.0);
    world
        .spawn("cat", None, SpawnOptions::at(10.0, 20.0))
        .unwrap();

    assert_eq!(
        world.entities[0].ground_y, 20.0,
        "with no floor measured, an entity stands where it was put"
    );
}

#[test]
fn moving_the_floor_carries_a_resting_entity_with_it() {
    let mut world = World::new(800.0, 600.0);
    world.set_ground_y(400.0);
    world
        .spawn(
            "cat",
            None,
            SpawnOptions {
                anchor: Some(Anchor::Bottom),
                ..SpawnOptions::default()
            },
        )
        .unwrap();

    world.set_ground_y(300.0);

    let expected = resting_y(&world, "cat", 300.0);
    assert_eq!(world.entities[0].ground_y, expected);
    assert_eq!(
        world.entities[0].y, expected,
        "an entity already on the floor moves with it rather than hanging"
    );
}

#[test]
fn moving_the_floor_leaves_a_manifest_floor_alone() {
    let mut manifest = AssetManifest::default_cat();
    manifest.name = "floored".to_string();
    manifest
        .states
        .get_mut("idle")
        .expect("cat has idle")
        .physics
        .ground_y = Some(5.0);

    let mut world = World::new(800.0, 600.0);
    world.set_ground_y(400.0);
    world
        .spawn("floored", Some(manifest), SpawnOptions::at(10.0, 20.0))
        .unwrap();
    let declared = world.entities[0].ground_y;

    world.set_ground_y(300.0);

    assert_eq!(
        world.entities[0].ground_y, declared,
        "a manifest declares its own floor; the screen has nothing to say about it"
    );
}

#[test]
fn a_manifest_floor_is_read_in_cells_like_every_other_position() {
    // `physics.ground_y` is a position, and manifest positions are in
    // terminal cells -- `spawn` is handed cells converted by its caller.
    // Copying the raw number into `Entity::ground_y`, which is in pixels,
    // put the same manifest's floor `cell_h` times further down on the
    // overlay than in the terminal. No built-in sets the field, so nothing
    // had exercised it.
    let mut world = World::new(800.0, 600.0);
    world.cell_w = 10.0;
    world.cell_h = 20.0;

    let mut manifest = AssetManifest::default_cat();
    manifest.name = "floored".to_string();
    manifest.initial_state = "idle".to_string();
    manifest.states.clear();
    manifest.states.insert(
        "idle".to_string(),
        StateDefinition {
            physics: PhysicsConfig {
                ground_y: Some(15.0),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    world
        .spawn("floored", Some(manifest), SpawnOptions::at(0.0, 0.0))
        .expect("floored probe spawns");

    assert_eq!(
        world.entities[0].ground_y, 300.0,
        "a floor 15 cells down is 300 pixels down at a 20-pixel cell"
    );
}

#[test]
fn a_spawned_z_overrides_the_manifests_draw_order() {
    let mut world = World::new(800.0, 600.0);
    world
        .spawn(
            "sun",
            None,
            SpawnOptions {
                z: Some(3.0),
                ..SpawnOptions::default()
            },
        )
        .unwrap();

    assert_eq!(world.entities[0].z_index, 3);
    assert_eq!(world.entities[0].z, 3.0);
}

#[test]
fn parallax_damps_how_far_an_entity_travels() {
    let mut near = World::new(800.0, 600.0);
    near.spawn("cat", None, SpawnOptions::at(0.0, 0.0)).unwrap();
    near.entities[0].vx = 2.0;

    let mut far = World::new(800.0, 600.0);
    far.spawn("cat", None, SpawnOptions::at(0.0, 0.0)).unwrap();
    far.entities[0].vx = 2.0;
    far.entities[0].parallax = 0.5;

    near.update(1.0 / 60.0);
    far.update(1.0 / 60.0);

    assert!(
        (far.entities[0].x - near.entities[0].x / 2.0).abs() < 1e-4,
        "half the parallax should cover half the ground: near {}, far {}",
        near.entities[0].x,
        far.entities[0].x
    );
}

#[test]
fn sprite_scale_follows_the_measured_cell_width() {
    let mut world = World::new(1920.0, 1080.0);
    world.set_grid(80, 24, Some(16.0), Some(36.0), 1920.0, 1080.0);
    assert_eq!(world.cell_w, 16.0);
    assert_eq!(world.cell_h, 36.0);
    assert_eq!(world.sprite_scale_x, 16.0);
    assert_eq!(world.sprite_scale_y, 18.0);
    assert_eq!(world.viewport_w, 1280.0);
    assert_eq!(world.viewport_h, 864.0);
}

#[test]
fn sprite_scale_uses_a_separate_factor_per_axis() {
    // A sprite pixel is one cell wide and half a cell tall. On a 16x36
    // HiDPI cell a uniform scale drew a 16px-tall sprite 7.1 cells tall
    // where the terminal backend drew 8.
    let mut world = World::new(1920.0, 1080.0);
    world.set_grid(80, 24, Some(16.0), Some(36.0), 1920.0, 1080.0);

    let cat = world.asset_manager.get("cat").unwrap();
    let drawn_h = cat.frame_h as f32 * world.sprite_scale_y;
    assert_eq!(
        drawn_h / world.cell_h,
        cat.frame_h as f32 / 2.0,
        "an overlay sprite must occupy the same number of cells as the terminal one"
    );
}

#[test]
fn set_grid_ignores_nonsense_cell_sizes() {
    let mut world = World::new(1920.0, 1080.0);
    world.set_grid(80, 24, Some(0.0), Some(-4.0), 1920.0, 1080.0);
    assert_eq!(world.cell_w, DEFAULT_CELL_W);
    assert_eq!(world.cell_h, DEFAULT_CELL_H);
}
