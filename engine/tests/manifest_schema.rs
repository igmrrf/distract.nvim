//! The asset manifest schema, and the invariants a manifest must satisfy.
//!
//! Held here rather than inside `manifest.rs` for the reason `ipc.rs` and
//! `voxel.rs` both moved their own: the schema *is* a public surface, every item
//! these tests touch is exported, and the module is a single coherent set of
//! serde structs that reads worse when a test module doubles its length.
//!
//! Capability gating is the substance. A manifest may declare which locomotion
//! classes it supports, and a state asking for one it did not declare is refused
//! at load rather than producing a pet that moves in a way its art cannot show.

use distract_engine::manifest::*;

/// A one-state manifest declaring `capabilities` and running `physics`.
fn declared(allowed: Option<&[&str]>, physics: PhysicsConfig) -> AssetManifest {
    let mut manifest = AssetManifest::default_cat();
    manifest.name = "declared".to_string();
    manifest.initial_state = "only".to_string();
    manifest.locomotion = None;
    manifest.capabilities = Capabilities {
        locomotion: allowed.map(|list| list.iter().map(|s| s.to_string()).collect()),
    };
    manifest.states.clear();
    manifest.states.insert(
        "only".to_string(),
        StateDefinition {
            physics,
            ..Default::default()
        },
    );
    manifest
}

#[test]
fn a_manifest_without_capabilities_accepts_any_locomotion() {
    let manifest = declared(
        None,
        PhysicsConfig {
            locomotion: Some(OMNIDIRECTIONAL.to_string()),
            ..Default::default()
        },
    );
    assert!(manifest.validate_capabilities().is_ok());
}

#[test]
fn a_state_outside_the_declared_locomotion_is_rejected() {
    let manifest = declared(
        Some(&[GROUNDED]),
        PhysicsConfig {
            locomotion: Some(BALLISTIC.to_string()),
            gravity: 0.3,
            ..Default::default()
        },
    );
    let message = manifest
        .validate_capabilities()
        .expect_err("a ballistic state under a grounded-only asset must not load");
    assert!(
        message.contains("only") && message.contains(BALLISTIC),
        "the message must name the offending state and class, got: {message}"
    );
}

#[test]
fn an_exotic_path_requires_omnidirectional_locomotion() {
    // Anything past `linear` and `sine` writes x directly, which fights a
    // floor. The engines skip paths entirely under gravity, so a grounded
    // orbit would silently do nothing at all.
    let manifest = declared(
        None,
        PhysicsConfig {
            locomotion: Some(GROUNDED.to_string()),
            path_type: Some("orbital".to_string()),
            ..Default::default()
        },
    );
    assert!(manifest.validate_capabilities().is_err());
}

#[test]
fn sine_and_linear_paths_are_allowed_on_the_ground() {
    for path in ["linear", "sine"] {
        let manifest = declared(
            None,
            PhysicsConfig {
                locomotion: Some(GROUNDED.to_string()),
                path_type: Some(path.to_string()),
                ..Default::default()
            },
        );
        assert!(
            manifest.validate_capabilities().is_ok(),
            "{path} moves y at most, so it does not need omnidirectional"
        );
    }
}

#[test]
fn declaring_omnidirectional_while_gravity_pulls_is_a_contradiction() {
    // The gravity branch wins at runtime, so the state would clamp to a
    // floor while claiming to have none.
    let manifest = declared(
        None,
        PhysicsConfig {
            locomotion: Some(OMNIDIRECTIONAL.to_string()),
            gravity: 0.4,
            ..Default::default()
        },
    );
    assert!(manifest.validate_capabilities().is_err());
}

#[test]
fn an_unknown_locomotion_name_is_rejected() {
    let manifest = declared(
        None,
        PhysicsConfig {
            locomotion: Some("hovering".to_string()),
            ..Default::default()
        },
    );
    assert!(manifest.validate_capabilities().is_err());
}

#[test]
fn a_state_inherits_the_manifest_locomotion_when_it_names_none() {
    let mut manifest = declared(Some(&[GROUNDED]), PhysicsConfig::default());
    manifest.locomotion = Some(GROUNDED.to_string());
    assert_eq!(
        manifest.locomotion_for(&manifest.states["only"]),
        GROUNDED,
        "a walking state has no gravity, so without the asset-level default \
         it would derive omnidirectional and violate its own declaration"
    );
    assert!(manifest.validate_capabilities().is_ok());
}

#[test]
fn every_builtin_satisfies_the_capabilities_it_declares() {
    for manifest in [
        AssetManifest::default_cat(),
        AssetManifest::default_crab(),
        AssetManifest::default_sun(),
    ] {
        let name = manifest.name.clone();
        assert!(
            manifest.capabilities.locomotion.is_some(),
            "{name} should declare what it can do, or the gate proves nothing"
        );
        manifest
            .validate_capabilities()
            .unwrap_or_else(|error| panic!("{name} violates its own declaration: {error}"));
    }
}

#[test]
fn an_empty_points_table_survives_the_lua_json_encoding() {
    // `vim.json.encode` writes an empty Lua table as `{}`, not `[]`, and
    // `points` is the first array-valued field a manifest can carry. The
    // terminal backend merely ignores a points list too short to draw with,
    // so without this the same manifest would fail to parse on the overlay
    // and describe two behaviours.
    let phys: PhysicsConfig =
        serde_json::from_str(r#"{"path_type":"bezier","path_params":{"points":{}}}"#)
            .expect("an empty points table must parse");
    assert_eq!(phys.path_params.and_then(|p| p.points), Some(Vec::new()));
}

#[test]
fn a_points_table_that_is_not_a_list_is_still_an_error() {
    let err =
        serde_json::from_str::<PhysicsConfig>(r#"{"path_params":{"points":{"first":[1.0,2.0]}}}"#);
    assert!(
        err.is_err(),
        "a keyed table is a mistake worth reporting, not an empty path"
    );
}

#[test]
fn test_default_manifests() {
    let cat = AssetManifest::default_cat();
    assert_eq!(cat.name, "cat");
    assert_eq!(cat.initial_state, "idle");
    assert!(cat.states.contains_key("idle"));
    assert!(cat.states.contains_key("walk"));
    assert!(cat.states.contains_key("walk_fast"));
    assert!(cat.states.contains_key("jump"));
    assert!(cat.states.contains_key("yawn"));
    assert!(cat.states.contains_key("sleep"));
    assert!(cat.custom_actions.contains_key("jump"));
    assert!(cat.custom_actions.contains_key("yawn"));

    let crab = AssetManifest::default_crab();
    assert_eq!(crab.name, "crab");
    assert_eq!(crab.initial_state, "idle");
    assert!(crab.states.contains_key("clip_claws"));
    assert!(crab.states.contains_key("burrow"));
    assert!(crab.custom_actions.contains_key("clip"));
    assert!(crab.custom_actions.contains_key("burrow"));

    let sun = AssetManifest::default_sun();
    assert_eq!(sun.name, "sun");
    assert_eq!(sun.initial_state, "shining");
    assert!(sun.states.contains_key("eclipse"));
    assert!(sun.states.contains_key("rising"));
    assert!(sun.states.contains_key("setting"));
    assert!(sun.custom_actions.contains_key("eclipse"));
    assert!(sun.custom_actions.contains_key("rise"));
}

#[test]
fn test_custom_json_deserialization() {
    let json_data = r#"{
        "name": "custom_dragon",
        "asset_type": "sprite",
        "initial_state": "fly",
        "spritesheet": {
            "path": "assets/dragon.png",
            "frame_width": 64,
            "frame_height": 64,
            "columns": 8,
            "rows": 2
        },
        "states": {
            "fly": {
                "animation": { "frames": [0, 1, 2, 3], "fps": 10.0, "loop_anim": true, "flip_x": false },
                "physics": { "target_vx": 4.0, "target_vy": -1.0, "gravity": 0.0, "friction": 0.05, "wrap_mode": "bounce" },
                "transitions": {
                    "on_event": { "typing": "breathe_fire" },
                    "on_edge_left": "turn_right",
                    "timeout_ms": 5000,
                    "on_timeout": "glide"
                }
            }
        },
        "custom_actions": {
            "fire": {
                "target_state": "breathe_fire",
                "duration_ms": 3000,
                "return_state": "fly"
            }
        }
    }"#;

    let manifest: AssetManifest =
        serde_json::from_str(json_data).expect("Should deserialize valid manifest");
    assert_eq!(manifest.name, "custom_dragon");
    assert_eq!(manifest.spritesheet.frame_width, Some(64));
    assert_eq!(manifest.spritesheet.columns, Some(8));
    assert_eq!(manifest.initial_state, "fly");

    let fly_state = manifest.states.get("fly").unwrap();
    assert_eq!(fly_state.animation.frames, vec![0, 1, 2, 3]);
    assert_eq!(fly_state.animation.fps, 10.0);
    assert_eq!(fly_state.physics.wrap_mode, WrapMode::Bounce);
    assert_eq!(fly_state.transitions.timeout_ms, Some(5000));
    assert_eq!(fly_state.transitions.on_timeout, Some("glide".to_string()));

    let fire_action = manifest.custom_actions.get("fire").unwrap();
    assert_eq!(fire_action.target_state, "breathe_fire");
    assert_eq!(fire_action.duration_ms, Some(3000));
}

#[test]
fn test_wrap_mode_variants() {
    let modes = vec![
        (r#""wrap""#, WrapMode::Wrap),
        (r#""bounce""#, WrapMode::Bounce),
        (r#""clamp""#, WrapMode::Clamp),
        (r#""despawn""#, WrapMode::Despawn),
        (r#""none""#, WrapMode::None),
    ];

    for (json_str, expected) in modes {
        let parsed: WrapMode = serde_json::from_str(json_str).unwrap();
        assert_eq!(parsed, expected);
    }
}

#[test]
fn test_spritesheet_config_deserialization_formats() {
    // Empty array format from Lua json_encode({})
    let json_seq = r#"{"name":"crab","spritesheet":[],"initial_state":"idle"}"#;
    let manifest_seq: AssetManifest =
        serde_json::from_str(json_seq).expect("Should deserialize empty seq spritesheet");
    assert_eq!(manifest_seq.name, "crab");
    assert_eq!(manifest_seq.spritesheet.path, None);

    // Empty object format
    let json_map = r#"{"name":"crab","spritesheet":{},"initial_state":"idle"}"#;
    let manifest_map: AssetManifest =
        serde_json::from_str(json_map).expect("Should deserialize empty map spritesheet");
    assert_eq!(manifest_map.name, "crab");

    // Null format
    let json_null = r#"{"name":"crab","spritesheet":null,"initial_state":"idle"}"#;
    let manifest_null: AssetManifest =
        serde_json::from_str(json_null).expect("Should deserialize null spritesheet");
    assert_eq!(manifest_null.name, "crab");
}
