//! The JSON-RPC contract between Neovim and the overlay engine.
//!
//! Both directions are pinned here rather than inside `ipc.rs`, because the wire
//! format *is* the public surface: `lua/distract/external.lua` writes these
//! exact field names and reads these exact `status` values, and a serde rename
//! that looks harmless from inside the crate breaks the other half of the plugin.

use distract_engine::ipc::{EntitySummary, IpcCommand, IpcResponse};
use distract_engine::journal::{self, WorldEvent};
use distract_engine::render::{RenderMode, RenderSettings};

#[test]
fn test_ipc_command_deserialization_all_variants() {
    let spawn_json = r#"{"command":"Spawn","entity_type":"cat","x":100.0,"y":200.0,"flip_x":true}"#;
    let cmd: IpcCommand = serde_json::from_str(spawn_json).unwrap();
    match cmd {
        IpcCommand::Spawn {
            entity_type,
            x,
            y,
            flip_x,
            ..
        } => {
            assert_eq!(entity_type, "cat");
            assert_eq!(x, Some(100.0));
            assert_eq!(y, Some(200.0));
            assert_eq!(flip_x, Some(true));
        }
        _ => panic!("Expected Spawn command"),
    }

    let despawn_json = r#"{"command":"Despawn","id":42}"#;
    let cmd: IpcCommand = serde_json::from_str(despawn_json).unwrap();
    match cmd {
        IpcCommand::Despawn { id } => assert_eq!(id, 42),
        _ => panic!("Expected Despawn command"),
    }

    let clear_json = r#"{"command":"ClearAll"}"#;
    let cmd: IpcCommand = serde_json::from_str(clear_json).unwrap();
    assert!(matches!(cmd, IpcCommand::ClearAll));

    let action_json = r#"{"command":"TriggerAction","id":1,"action":"jump"}"#;
    let cmd: IpcCommand = serde_json::from_str(action_json).unwrap();
    match cmd {
        IpcCommand::TriggerAction { id, action, .. } => {
            assert_eq!(id, Some(1));
            assert_eq!(action, "jump");
        }
        _ => panic!("Expected TriggerAction"),
    }

    let event_json = r#"{"command":"EditorEvent","event":"typing"}"#;
    let cmd: IpcCommand = serde_json::from_str(event_json).unwrap();
    match cmd {
        IpcCommand::EditorEvent { event, .. } => assert_eq!(event, "typing"),
        _ => panic!("Expected EditorEvent"),
    }

    let grid_json =
        r#"{"command":"UpdateGrid","width":80,"height":24,"cell_width":10,"cell_height":20}"#;
    let cmd: IpcCommand = serde_json::from_str(grid_json).unwrap();
    match cmd {
        IpcCommand::UpdateGrid {
            width,
            height,
            cell_width,
            cell_height,
            ..
        } => {
            assert_eq!(width, 80);
            assert_eq!(height, 24);
            assert_eq!(cell_width, Some(10));
            assert_eq!(cell_height, Some(20));
        }
        _ => panic!("Expected UpdateGrid"),
    }

    let ping_json = r#"{"command":"Ping"}"#;
    let cmd: IpcCommand = serde_json::from_str(ping_json).unwrap();
    assert!(matches!(cmd, IpcCommand::Ping));

    let status_json = r#"{"command":"GetStatus"}"#;
    let cmd: IpcCommand = serde_json::from_str(status_json).unwrap();
    assert!(matches!(cmd, IpcCommand::GetStatus));

    let shutdown_json = r#"{"command":"Shutdown"}"#;
    let cmd: IpcCommand = serde_json::from_str(shutdown_json).unwrap();
    assert!(matches!(cmd, IpcCommand::Shutdown));
}

#[test]
fn spawn_carries_depth_and_an_anchor() {
    let json = r#"{"command":"Spawn","entity_type":"cat","z":2.0,
                   "parallax":0.6,"anchor":"bottom"}"#;
    let cmd: IpcCommand = serde_json::from_str(json).unwrap();
    match cmd {
        IpcCommand::Spawn {
            z,
            parallax,
            anchor,
            ..
        } => {
            assert_eq!(z, Some(2.0));
            assert_eq!(parallax, Some(0.6));
            assert_eq!(anchor.as_deref(), Some("bottom"));
        }
        _ => panic!("Expected Spawn command"),
    }
}

#[test]
fn update_grid_carries_the_floor_and_survives_without_one() {
    let with_floor = r#"{"command":"UpdateGrid","width":80,"height":24,"ground_y":420.0}"#;
    let cmd: IpcCommand = serde_json::from_str(with_floor).unwrap();
    match cmd {
        IpcCommand::UpdateGrid { ground_y, .. } => assert_eq!(ground_y, Some(420.0)),
        _ => panic!("Expected UpdateGrid"),
    }

    // A client that predates the floor still has to be understood.
    let without_floor = r#"{"command":"UpdateGrid","width":80,"height":24}"#;
    let cmd: IpcCommand = serde_json::from_str(without_floor).unwrap();
    match cmd {
        IpcCommand::UpdateGrid { ground_y, .. } => assert_eq!(ground_y, None),
        _ => panic!("Expected UpdateGrid"),
    }
}

#[test]
fn the_plugin_commands_carry_what_a_hook_asked_for() {
    let set_state = r#"{"command":"SetState","id":7,"state":"jump"}"#;
    match serde_json::from_str::<IpcCommand>(set_state).unwrap() {
        IpcCommand::SetState { id, state } => {
            assert_eq!(id, 7);
            assert_eq!(state, "jump");
        }
        other => panic!("Expected SetState, got {other:?}"),
    }

    // An impulse on one axis only still has to parse.
    let impulse = r#"{"command":"Impulse","id":2,"vx":-1.5}"#;
    match serde_json::from_str::<IpcCommand>(impulse).unwrap() {
        IpcCommand::Impulse { id, vx, vy } => {
            assert_eq!(id, 2);
            assert_eq!(vx, -1.5);
            assert_eq!(vy, 0.0);
        }
        other => panic!("Expected Impulse, got {other:?}"),
    }

    let subscribe = r#"{"command":"Subscribe","snapshot_ms":100}"#;
    match serde_json::from_str::<IpcCommand>(subscribe).unwrap() {
        IpcCommand::Subscribe { snapshot_ms } => assert_eq!(snapshot_ms, Some(100)),
        other => panic!("Expected Subscribe, got {other:?}"),
    }

    let unsubscribe = r#"{"command":"Subscribe"}"#;
    match serde_json::from_str::<IpcCommand>(unsubscribe).unwrap() {
        IpcCommand::Subscribe { snapshot_ms } => assert_eq!(snapshot_ms, None),
        other => panic!("Expected Subscribe, got {other:?}"),
    }
}

#[test]
fn the_plugin_responses_name_their_status_and_their_payload() {
    let snapshot = IpcResponse::Snapshot {
        entities: vec![EntitySummary {
            id: 1,
            asset_name: "cat".to_string(),
            state: "walk".to_string(),
            x: 1.0,
            y: 2.0,
            vx: 3.0,
            vy: 4.0,
        }],
        dt: 0.1,
    };
    let line = snapshot.to_json_line();
    assert!(line.contains(r#""status":"snapshot""#));
    assert!(line.contains(r#""dt":0.1"#));

    let events = IpcResponse::WorldEvents {
        events: vec![WorldEvent::collision(1, journal::EDGE_LEFT)],
        dropped: 3,
    };
    let line = events.to_json_line();
    assert!(line.contains(r#""status":"world_events""#));
    assert!(line.contains(r#""event":"collision""#));
    assert!(line.contains(r#""edge":"left""#));
    assert!(line.contains(r#""dropped":3"#));
}

#[test]
fn test_ipc_response_serialization_all_variants() {
    let resp_ready = IpcResponse::Ready {
        version: "0.2.0".to_string(),
    };
    let line = resp_ready.to_json_line();
    assert!(line.contains(r#""status":"ready""#));
    assert!(line.contains(r#""version":"0.2.0""#));
    assert!(line.ends_with('\n'));

    let resp_spawned = IpcResponse::Spawned {
        id: 1,
        asset_name: "cat".to_string(),
        state: "idle".to_string(),
    };
    let line = resp_spawned.to_json_line();
    assert!(line.contains(r#""status":"spawned""#));
    assert!(line.contains(r#""id":1"#));

    let resp_action = IpcResponse::ActionTriggered {
        id: 1,
        asset_name: "cat".to_string(),
        action: "jump".to_string(),
        state: "jump".to_string(),
    };
    let line = resp_action.to_json_line();
    assert!(line.contains(r#""status":"action_triggered""#));

    let resp_despawned = IpcResponse::Despawned { id: 1 };
    assert!(
        resp_despawned
            .to_json_line()
            .contains(r#""status":"despawned""#)
    );

    let resp_cleared = IpcResponse::Cleared;
    assert!(
        resp_cleared
            .to_json_line()
            .contains(r#""status":"cleared""#)
    );

    let resp_pong = IpcResponse::Pong;
    assert!(resp_pong.to_json_line().contains(r#""status":"pong""#));

    let resp_status = IpcResponse::StatusReport {
        count: 1,
        entities: vec![EntitySummary {
            id: 1,
            asset_name: "cat".to_string(),
            state: "idle".to_string(),
            x: 10.0,
            y: 20.0,
            vx: 1.0,
            vy: 0.0,
        }],
    };
    let line = resp_status.to_json_line();
    assert!(line.contains(r#""status":"status_report""#));
    assert!(line.contains(r#""count":1"#));

    let resp_err = IpcResponse::Error {
        code: "TEST_ERR".to_string(),
        message: "Something failed".to_string(),
    };
    let line = resp_err.to_json_line();
    assert!(line.contains(r#""status":"error""#));
    assert!(line.contains("TEST_ERR"));
}

#[test]
fn update_render_carries_the_whole_settings_block() {
    // The exact shape `lua/distract/render.lua` encodes.
    let json = r#"{"command":"UpdateRender","settings":{"mode":"3d","fov_y_degrees":50.0,
        "depth_per_unit":0.08,"yaw_degrees":22.0,"voxel_max_width":48,"voxel_depth":4,
        "light":{"direction":[-0.4,0.8,-0.45],"ambient":0.42}}}"#;
    let command: IpcCommand = serde_json::from_str(json).expect("UpdateRender parses");
    match command {
        IpcCommand::UpdateRender { settings } => {
            assert_eq!(settings.mode, RenderMode::Voxel);
            assert_eq!(settings.fov_y_degrees, 50.0);
            assert_eq!(settings.voxel_depth, 4);
            assert_eq!(settings.light.ambient, 0.42);
            assert_eq!(settings.light.direction, [-0.4, 0.8, -0.45]);
        }
        _ => panic!("expected UpdateRender"),
    }
}

#[test]
fn update_render_accepts_a_settings_block_that_names_only_what_changed() {
    // A client toggling the mode must not have to restate the camera and the
    // light, and an omitted field must not reset a neighbour to zero.
    let json = r#"{"command":"UpdateRender","settings":{"mode":"3d"}}"#;
    let command: IpcCommand = serde_json::from_str(json).expect("a partial block parses");
    match command {
        IpcCommand::UpdateRender { settings } => {
            assert_eq!(settings.mode, RenderMode::Voxel);
            assert_eq!(
                settings.fov_y_degrees,
                RenderSettings::default().fov_y_degrees
            );
            assert_eq!(settings.voxel_depth, RenderSettings::default().voxel_depth);
        }
        _ => panic!("expected UpdateRender"),
    }
}

#[test]
fn the_render_mode_wire_names_are_the_ones_a_user_writes_in_their_config() {
    // `2d` and `3d` rather than `flat` and `voxel`: the name in `:help` and the
    // name on the wire have to be the same one, or a user reading an engine log
    // cannot tell what their own setting became.
    for (wire, expected) in [("2d", RenderMode::Flat), ("3d", RenderMode::Voxel)] {
        let json = format!(r#"{{"command":"UpdateRender","settings":{{"mode":"{wire}"}}}}"#);
        let command: IpcCommand = serde_json::from_str(&json).expect("mode parses");
        match command {
            IpcCommand::UpdateRender { settings } => assert_eq!(settings.mode, expected),
            _ => panic!("expected UpdateRender"),
        }
    }
}

#[test]
fn a_manifest_may_pin_its_own_render_mode_over_the_wire() {
    let json = r#"{"command":"Spawn","entity_type":"bubble","manifest":{"name":"bubble",
        "render":"2d","states":{}}}"#;
    let command: IpcCommand = serde_json::from_str(json).expect("Spawn parses");
    match command {
        IpcCommand::Spawn { manifest, .. } => {
            let manifest = manifest.expect("a manifest arrived");
            assert_eq!(manifest.render, Some(RenderMode::Flat));
        }
        _ => panic!("expected Spawn"),
    }
}
