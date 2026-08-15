use crate::manifest::AssetManifest;
use serde::{Deserialize, Serialize};

/// Incoming JSON-RPC command from Neovim.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command")]
pub enum IpcCommand {
    #[serde(rename = "Spawn", alias = "spawn", alias = "spawn_asset")]
    Spawn {
        #[serde(default)]
        id: Option<usize>,
        #[serde(alias = "asset_name")]
        entity_type: String,
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        manifest: Option<Box<AssetManifest>>,
        #[serde(default)]
        x: Option<f32>,
        #[serde(default)]
        y: Option<f32>,
        #[serde(default)]
        flip_x: Option<bool>,
    },
    #[serde(rename = "Despawn", alias = "despawn")]
    Despawn { id: usize },
    #[serde(
        rename = "ClearAll",
        alias = "clear_all",
        alias = "clear_entities",
        alias = "clear"
    )]
    ClearAll,
    #[serde(rename = "TriggerAction", alias = "trigger_action")]
    TriggerAction {
        #[serde(default)]
        id: Option<usize>,
        #[serde(default)]
        asset_name: Option<String>,
        action: String,
    },
    #[serde(rename = "EditorEvent", alias = "editor_event")]
    EditorEvent {
        event: String,
        #[serde(default)]
        context: Option<serde_json::Value>,
    },
    #[serde(rename = "UpdateGrid", alias = "update_grid")]
    UpdateGrid {
        width: u32,
        height: u32,
        #[serde(default)]
        cell_width: Option<u32>,
        #[serde(default)]
        cell_height: Option<u32>,
        #[serde(default)]
        scale_factor: Option<f64>,
    },
    #[serde(rename = "Ping", alias = "ping")]
    Ping,
    #[serde(rename = "GetStatus", alias = "get_status")]
    GetStatus,
    #[serde(rename = "Shutdown", alias = "shutdown")]
    Shutdown,
}

/// Outgoing summary of an active entity in the world.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySummary {
    pub id: usize,
    pub asset_name: String,
    pub state: String,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
}

/// Outgoing JSON-RPC response from Rust Engine to Neovim.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum IpcResponse {
    #[serde(rename = "ready")]
    Ready { version: String },
    #[serde(rename = "spawned")]
    Spawned {
        id: usize,
        asset_name: String,
        state: String,
    },
    #[serde(rename = "despawned")]
    Despawned { id: usize },
    #[serde(rename = "cleared")]
    Cleared,
    #[serde(rename = "action_triggered")]
    ActionTriggered {
        id: usize,
        asset_name: String,
        action: String,
        state: String,
    },
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "status_report")]
    StatusReport {
        count: usize,
        entities: Vec<EntitySummary>,
    },
    #[serde(rename = "error")]
    Error { code: String, message: String },
}

impl IpcResponse {
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string()) + "\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_command_deserialization_all_variants() {
        let spawn_json =
            r#"{"command":"Spawn","entity_type":"cat","x":100.0,"y":200.0,"flip_x":true}"#;
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
        assert!(resp_despawned
            .to_json_line()
            .contains(r#""status":"despawned""#));

        let resp_cleared = IpcResponse::Cleared;
        assert!(resp_cleared
            .to_json_line()
            .contains(r#""status":"cleared""#));

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
}
