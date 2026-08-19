//! Applying one IPC command to the world.
//!
//! This is the overlay's request boundary: every command is validated against
//! the world before it changes anything, and every outcome — including a refusal
//! — is reported back on stdout, because Neovim's idea of what is alive comes
//! only from what this sends.

use winit::event_loop::ControlFlow;
use winit::window::Window;

use crate::bounds::Bounds;
use crate::ecs::{EventContext, World};
use crate::gpu::GpuRenderer;
use crate::ipc::{IpcCommand, IpcResponse};
use crate::manifest::AssetManifest;
use crate::render::RenderSettings;
use crate::response::{emit_error, emit_response};
use crate::spawn::{Anchor, SpawnOptions};
use crate::subscription::Subscription;

/// The overlay window's size in physical pixels.
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
}

/// Everything a command may touch.
///
/// Bundled rather than passed as seven parameters, and it is the same bundle for
/// every command so no handler can reach for something the others cannot.
pub struct CommandContext<'a> {
    pub world: &'a mut World,
    pub window: &'a Window,
    pub renderer: &'a mut GpuRenderer,
    pub control_flow: &'a mut ControlFlow,
    pub subscription: &'a mut Subscription,
    pub viewport: Viewport,
}

struct ScopeRequest {
    x: Option<f32>,
    y: Option<f32>,
    width: Option<f32>,
    height: Option<f32>,
}

struct SpawnRequest {
    entity_type: String,
    manifest: Option<AssetManifest>,
    options: SpawnOptions,
}

pub fn handle(command: IpcCommand, ctx: &mut CommandContext<'_>) {
    match command {
        IpcCommand::Spawn {
            entity_type,
            path,
            manifest,
            x,
            y,
            z,
            parallax,
            anchor,
            flip_x,
            ..
        } => spawn(
            SpawnRequest {
                manifest: resolve_manifest(manifest.map(|boxed| *boxed), path, &entity_type),
                entity_type,
                options: SpawnOptions {
                    x,
                    y,
                    z,
                    parallax,
                    anchor: anchor.as_deref().and_then(Anchor::from_name),
                    flip_x,
                },
            },
            ctx,
        ),
        IpcCommand::Despawn { id } => {
            if ctx.world.despawn(id) {
                emit_response(&IpcResponse::Despawned { id });
            } else {
                emit_error("NOT_FOUND", format!("Entity #{} not found", id));
            }
        }
        IpcCommand::ClearAll => {
            ctx.world.clear_all();
            emit_response(&IpcResponse::Cleared);
        }
        IpcCommand::TriggerAction {
            id,
            asset_name,
            action,
        } => trigger_action(id, asset_name, &action, ctx),
        IpcCommand::SetState { id, state } => {
            if let Err(err) = ctx.world.set_entity_state(id, &state) {
                emit_error("SET_STATE_FAILED", err);
            }
        }
        IpcCommand::Impulse { id, vx, vy } => {
            if let Err(err) = ctx.world.apply_impulse(id, vx, vy) {
                emit_error("IMPULSE_FAILED", err);
            }
        }
        IpcCommand::UpdateViewportScope {
            x,
            y,
            width,
            height,
        } => update_scope(
            ScopeRequest {
                x,
                y,
                width,
                height,
            },
            ctx,
        ),
        IpcCommand::UpdateObstacles { obstacles } => {
            if let Err(err) = ctx.world.set_obstacles(obstacles) {
                emit_error("TOO_MANY_OBSTACLES", err);
            }
        }
        IpcCommand::UpdateRender { settings } => update_render(*settings, ctx),
        IpcCommand::SetVisible { visible } => ctx.window.set_visible(visible),
        IpcCommand::Subscribe { snapshot_ms } => {
            ctx.subscription.set_interval_ms(snapshot_ms);
            ctx.world
                .journal
                .set_enabled(ctx.subscription.is_subscribed());
        }
        IpcCommand::EditorEvent { event, context } => {
            ctx.world
                .handle_editor_event(&event, EventContext::from_json(context.as_ref()));
        }
        IpcCommand::UpdateGrid {
            width,
            height,
            cell_width,
            cell_height,
            ground_y,
            ..
        } => {
            ctx.world.set_grid(
                width,
                height,
                cell_width.map(|value| value as f32),
                cell_height.map(|value| value as f32),
                ctx.viewport.width,
                ctx.viewport.height,
            );
            if let Some(ground_y) = ground_y {
                ctx.world.set_ground_y(ground_y);
            }
        }
        IpcCommand::Ping => emit_response(&IpcResponse::Pong),
        IpcCommand::GetStatus => {
            let entities = ctx.world.get_summaries();
            emit_response(&IpcResponse::StatusReport {
                count: entities.len(),
                entities,
            });
        }
        IpcCommand::Shutdown => {
            *ctx.control_flow = ControlFlow::Exit;
        }
    }
}

/// A spawn that names only a spritesheet gets the built-in cat's behaviour under
/// its own name, which is what makes `:DistractSpawn <path>` work with no
/// manifest at all.
fn resolve_manifest(
    manifest: Option<AssetManifest>,
    path: Option<String>,
    entity_type: &str,
) -> Option<AssetManifest> {
    if manifest.is_some() {
        return manifest;
    }
    let path = path?;
    let mut fallback = AssetManifest::default_cat();
    fallback.name = entity_type.to_string();
    fallback.spritesheet.path = Some(path);
    Some(fallback)
}

/// A scope with no size clears the restriction; a bad one is refused loudly.
fn update_scope(request: ScopeRequest, ctx: &mut CommandContext<'_>) {
    let scope = match (request.width, request.height) {
        (Some(width), Some(height)) => Some(Bounds {
            left: request.x.unwrap_or(0.0),
            top: request.y.unwrap_or(0.0),
            width,
            height,
        }),
        _ => None,
    };

    if let Err(err) = ctx.world.set_scope(scope) {
        emit_error("INVALID_VIEWPORT_SCOPE", err);
    }
}

/// Applies new render settings, and rebuilds whatever they changed.
///
/// The meshes depend on the slab size, so a settings change is an asset change
/// from the renderer's point of view. Sanitised here rather than at the point of
/// use: a value that is wrong is wrong once, and clamping per frame would hide
/// which field it was.
fn update_render(settings: RenderSettings, ctx: &mut CommandContext<'_>) {
    ctx.world.render = settings.sanitised();
    if let Err(err) = ctx.renderer.sync_assets(ctx.world) {
        emit_error("MESH_BUILD_FAILED", err);
    }
}

fn spawn(request: SpawnRequest, ctx: &mut CommandContext<'_>) {
    let id = match ctx
        .world
        .spawn(&request.entity_type, request.manifest, request.options)
    {
        Ok(id) => id,
        Err(err) => return emit_error("SPAWN_FAILED", err),
    };

    if let Err(err) = ctx.renderer.sync_assets(ctx.world) {
        return emit_error("ATLAS_FAILED", err);
    }

    let state = ctx
        .world
        .entities
        .iter()
        .find(|entity| entity.id == id)
        .map(|entity| entity.current_state.clone())
        .unwrap_or_else(|| "idle".to_string());

    emit_response(&IpcResponse::Spawned {
        id,
        asset_name: request.entity_type,
        state,
    });
}

fn trigger_action(
    id: Option<usize>,
    asset_name: Option<String>,
    action: &str,
    ctx: &mut CommandContext<'_>,
) {
    match ctx.world.trigger_action(id, asset_name.as_deref(), action) {
        Ok(triggered) => {
            for (entity_id, entity_asset, state) in triggered {
                emit_response(&IpcResponse::ActionTriggered {
                    id: entity_id,
                    asset_name: entity_asset,
                    action: action.to_string(),
                    state,
                });
            }
        }
        Err(err) => emit_error("ACTION_FAILED", err),
    }
}
