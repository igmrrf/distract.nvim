use crate::journal::WorldEvent;
use crate::manifest::AssetManifest;
use crate::obstacles::{self, Obstacle};
use crate::render::RenderSettings;
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
        /// Depth. Dimensionless, unlike `x` and `y`, which arrive in pixels.
        #[serde(default)]
        z: Option<f32>,
        /// Motion and size multiplier Neovim derived from `z`.
        #[serde(default)]
        parallax: Option<f32>,
        /// `"bottom"`, `"top"` or `"free"`, applied when no explicit position
        /// is given. Neovim resolves its own `auto` before sending.
        #[serde(default)]
        anchor: Option<String>,
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
    /// Puts an entity into a state a Lua plugin asked for.
    ///
    /// Plugins run in Lua on every backend and never write to the simulation
    /// directly, so a hook that wants a state change requests one and it arrives
    /// here. `TriggerAction` is the user-facing verb and resolves through the
    /// manifest's `custom_actions`; this names the state outright.
    #[serde(rename = "SetState", alias = "set_state")]
    SetState { id: usize, state: String },
    /// Adds to an entity's velocity, in sprite pixels per frame at 60 FPS.
    #[serde(rename = "Impulse", alias = "impulse")]
    Impulse {
        id: usize,
        #[serde(default)]
        vx: f32,
        #[serde(default)]
        vy: f32,
    },
    /// Replaces the solid platforms and hazards entities interact with.
    ///
    /// Collected by Neovim from registered providers on a debounced cadence and
    /// pushed here, never discovered by this engine — the same rule the floor
    /// follows, and for the same reason: only the editor can run a Tree-sitter
    /// query or read a fold.
    #[serde(rename = "UpdateObstacles", alias = "update_obstacles")]
    UpdateObstacles {
        #[serde(default, deserialize_with = "obstacles::list_allowing_empty_table")]
        obstacles: Vec<Obstacle>,
    },
    /// Restricts entities to a rectangle, in overlay pixels.
    ///
    /// Only Neovim can see where a window's text area is, what is floating over
    /// it and which splits the user is working in, so the rectangle is measured
    /// there and pushed here — exactly as the floor is. Omitting every field
    /// clears the scope and returns entities to the whole window.
    #[serde(rename = "UpdateViewportScope", alias = "update_viewport_scope")]
    UpdateViewportScope {
        #[serde(default)]
        x: Option<f32>,
        #[serde(default)]
        y: Option<f32>,
        #[serde(default)]
        width: Option<f32>,
        #[serde(default)]
        height: Option<f32>,
    },
    /// Replaces the render settings: mode, camera, lighting and slab size.
    ///
    /// Pushed from Neovim exactly as the floor and the viewport scope are, and for
    /// the same reason: the configuration is the editor's, and the terminal
    /// backends have to draw under the same numbers. Every field is optional, and
    /// an omitted one keeps its default rather than resetting a neighbour.
    #[serde(rename = "UpdateRender", alias = "update_render")]
    UpdateRender { settings: Box<RenderSettings> },
    /// Shows or hides the overlay window.
    ///
    /// The window is always-on-top and belongs to this process, so an unfocused
    /// Neovim cannot hide it for itself. The simulation keeps running while
    /// hidden: an entity mid-wrap must not be stranded until focus returns.
    #[serde(rename = "SetVisible", alias = "set_visible")]
    SetVisible { visible: bool },
    /// Asks for world snapshots and world events on a bounded cadence.
    ///
    /// Off until requested: a session with no plugins puts nothing on the wire
    /// per frame. `snapshot_ms` of `None` or `0` unsubscribes.
    #[serde(rename = "Subscribe", alias = "subscribe")]
    Subscribe {
        #[serde(default)]
        snapshot_ms: Option<u64>,
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
        /// The floor in overlay pixels: the surface an entity's feet rest on.
        ///
        /// Only Neovim can measure it, because it depends on `cmdheight`, the
        /// statusline and where the buffer text ends. Sent when it moves rather
        /// than per frame, and `serde(default)` keeps a client that never sends
        /// one working.
        #[serde(default)]
        ground_y: Option<f32>,
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
    /// Every live entity, on the subscribed cadence.
    #[serde(rename = "snapshot")]
    Snapshot {
        entities: Vec<EntitySummary>,
        /// Seconds since the previous snapshot, so a Lua `on_tick` hook gets the
        /// same `dt` contract it gets from the in-terminal engines.
        dt: f32,
    },
    /// What the world did since the previous drain.
    #[serde(rename = "world_events")]
    WorldEvents {
        events: Vec<WorldEvent>,
        /// Events dropped to keep the journal bounded. Reported rather than
        /// hidden: a plugin missing a collision should be visible.
        #[serde(default)]
        dropped: usize,
    },
    #[serde(rename = "status_report")]
    StatusReport {
        count: usize,
        entities: Vec<EntitySummary>,
    },
    /// A condition the user should know about that is not a failure.
    ///
    /// The overlay opening on a guessed display is the motivating case: the
    /// window is there and working, but on the wrong screen, and reporting it as
    /// an error would say the engine failed when it did not.
    #[serde(rename = "warning")]
    Warning { code: String, message: String },
    #[serde(rename = "error")]
    Error { code: String, message: String },
}

impl IpcResponse {
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string()) + "\n"
    }
}
