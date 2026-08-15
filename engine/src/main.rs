//! Overlay engine binary.
//!
//! This is a thin driver over the `distract_engine` library: it owns the window
//! and the event loop and nothing else. It previously re-declared the whole
//! module tree, so the binary and the library each compiled their own copy of
//! every module and ran every unit test twice.

use std::io::{self, BufRead, Write};
use std::thread;
use std::time::{Duration, Instant};

use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
};

use distract_engine::ecs::{EventContext, World};
use distract_engine::gpu::GpuRenderer;
use distract_engine::ipc::{IpcCommand, IpcResponse};
use distract_engine::platform;

fn emit_response(resp: &IpcResponse) {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    let _ = lock.write_all(resp.to_json_line().as_bytes());
    let _ = lock.flush();
}

fn emit_error(code: &str, message: impl Into<String>) {
    emit_response(&IpcResponse::Error {
        code: code.to_string(),
        message: message.into(),
    });
}

fn main() {
    let _ = env_logger::try_init();

    #[cfg(target_os = "macos")]
    let event_loop = {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        EventLoopBuilder::<IpcCommand>::with_user_event()
            .with_activation_policy(ActivationPolicy::Accessory)
            .build()
    };

    #[cfg(not(target_os = "macos"))]
    let event_loop = EventLoopBuilder::<IpcCommand>::with_user_event().build();

    let proxy = event_loop.create_proxy();

    // Async stdin reader thread for JSON-RPC.
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(content) => {
                    let trimmed = content.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<IpcCommand>(trimmed) {
                        Ok(cmd) => {
                            if proxy.send_event(cmd).is_err() {
                                break;
                            }
                        }
                        Err(err) => {
                            emit_error("PARSE_ERROR", format!("Failed to parse JSON: {}", err));
                        }
                    }
                }
                Err(_) => break,
            }
        }
        let _ = proxy.send_event(IpcCommand::Shutdown);
    });

    // Detect monitor dimensions or fall back to standard HD resolution.
    let primary_monitor = event_loop
        .primary_monitor()
        .or_else(|| event_loop.available_monitors().next());
    let (requested_w, requested_h) = match primary_monitor {
        Some(mon) => {
            let size = mon.size();
            (
                size.width.max(800).clamp(800, 3840),
                size.height.max(600).clamp(600, 2160),
            )
        }
        None => (1920, 1080),
    };

    let window = match platform::create_overlay_window(&event_loop, requested_w, requested_h) {
        Ok(win) => win,
        Err(err) => {
            emit_error(
                "WINDOW_CREATION_FAILED",
                format!("Could not initialize overlay window: {}", err),
            );
            return;
        }
    };

    // The window manager is free to grant a different size than was asked for
    // — tiling WMs, HiDPI scaling, reserved struts. Configure everything from
    // what actually exists rather than from the request.
    let granted = window.inner_size();
    let mut window_width = granted.width.max(1);
    let mut window_height = granted.height.max(1);

    let mut gpu_renderer =
        match pollster::block_on(GpuRenderer::new(&window, window_width, window_height)) {
            Ok(g) => {
                platform::configure_layer_transparency(&window);
                g
            }
            Err(err) => {
                emit_error(
                    "GPU_INIT_FAILED",
                    format!("Could not initialize GPU renderer: {}", err),
                );
                return;
            }
        };

    let mut world = World::new(window_width as f32, window_height as f32);
    if let Err(err) = gpu_renderer.sync_atlas(&world) {
        emit_error("ATLAS_FAILED", err);
        return;
    }

    emit_response(&IpcResponse::Ready {
        version: env!("CARGO_PKG_VERSION").to_string(),
    });

    let target_fps = 60;
    let frame_duration = Duration::from_micros(1_000_000 / target_fps);
    let mut last_tick = Instant::now();
    // Whether the last simulated step changed anything worth drawing. An
    // overlay of sleeping cats should cost approximately nothing, so a frame is
    // only submitted when the picture can actually differ.
    let mut needs_redraw = true;

    event_loop.run(move |event, _, control_flow| {
        let now = Instant::now();
        let next_frame = last_tick + frame_duration;
        *control_flow = ControlFlow::WaitUntil(next_frame);

        match event {
            Event::UserEvent(cmd) => {
                handle_command(
                    cmd,
                    &mut world,
                    &mut gpu_renderer,
                    control_flow,
                    window_width as f32,
                    window_height as f32,
                );
                needs_redraw = true;
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                }
                WindowEvent::Resized(size) => {
                    if size.width > 0 && size.height > 0 {
                        window_width = size.width;
                        window_height = size.height;
                        world.viewport_w = window_width as f32;
                        world.viewport_h = window_height as f32;
                        gpu_renderer.resize(window_width, window_height);
                        needs_redraw = true;
                    }
                }
                // Dragging the overlay to a monitor with a different scale
                // factor arrives here, not as a Resized. Ignoring it left the
                // surface configured for the old size with no recovery path.
                WindowEvent::ScaleFactorChanged { new_inner_size, .. }
                    if new_inner_size.width > 0 && new_inner_size.height > 0 =>
                {
                    window_width = new_inner_size.width;
                    window_height = new_inner_size.height;
                    world.viewport_w = window_width as f32;
                    world.viewport_h = window_height as f32;
                    gpu_renderer.resize(window_width, window_height);
                    needs_redraw = true;
                }
                _ => {}
            },
            Event::MainEventsCleared => {
                if now >= next_frame {
                    let dt = now.duration_since(last_tick).as_secs_f32().min(0.1);
                    last_tick = now;

                    let despawned = world.update(dt);
                    for id in despawned {
                        // Without this, Neovim's idea of what is alive silently
                        // diverges from the engine's.
                        emit_response(&IpcResponse::Despawned { id });
                        needs_redraw = true;
                    }

                    if !world.is_quiescent() {
                        needs_redraw = true;
                    }

                    if needs_redraw {
                        window.request_redraw();
                    }
                }
            }
            Event::RedrawRequested(_) => {
                needs_redraw = false;

                if let Err(err) = gpu_renderer.render_world(&world) {
                    match err {
                        // Both mean the surface no longer matches the window:
                        // Lost after a device reset, Outdated after a monitor,
                        // DPI or compositor change. Reconfigure for both.
                        wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated => {
                            gpu_renderer.resize(window_width, window_height);
                            needs_redraw = true;
                        }
                        wgpu::SurfaceError::Timeout => {
                            // Transient; the next frame will pick it up.
                            needs_redraw = true;
                        }
                        wgpu::SurfaceError::OutOfMemory => {
                            emit_error("RENDER_ERROR", "wgpu out of memory");
                            *control_flow = ControlFlow::Exit;
                        }
                    }
                }
            }
            _ => {}
        }
    });
}

fn handle_command(
    cmd: IpcCommand,
    world: &mut World,
    gpu_renderer: &mut GpuRenderer,
    control_flow: &mut ControlFlow,
    max_w: f32,
    max_h: f32,
) {
    match cmd {
        IpcCommand::Spawn {
            entity_type,
            path,
            manifest,
            x,
            y,
            flip_x,
            ..
        } => {
            let mut manifest_to_use = manifest.map(|b| *b);
            if manifest_to_use.is_none() {
                if let Some(p) = path {
                    let mut m = distract_engine::manifest::AssetManifest::default_cat();
                    m.name = entity_type.clone();
                    m.spritesheet.path = Some(p);
                    manifest_to_use = Some(m);
                }
            }

            match world.spawn(&entity_type, manifest_to_use, x, y, flip_x) {
                Ok(id) => {
                    if let Err(err) = gpu_renderer.sync_atlas(world) {
                        emit_error("ATLAS_FAILED", err);
                        return;
                    }
                    let state = world
                        .entities
                        .iter()
                        .find(|e| e.id == id)
                        .map(|e| e.current_state.clone())
                        .unwrap_or_else(|| "idle".to_string());
                    emit_response(&IpcResponse::Spawned {
                        id,
                        asset_name: entity_type,
                        state,
                    });
                }
                Err(err) => emit_error("SPAWN_FAILED", err),
            }
        }
        IpcCommand::Despawn { id } => {
            if world.despawn(id) {
                emit_response(&IpcResponse::Despawned { id });
            } else {
                emit_error("NOT_FOUND", format!("Entity #{} not found", id));
            }
        }
        IpcCommand::ClearAll => {
            world.clear_all();
            emit_response(&IpcResponse::Cleared);
        }
        IpcCommand::TriggerAction {
            id,
            asset_name,
            action,
        } => match world.trigger_action(id, asset_name.as_deref(), &action) {
            Ok(triggered_list) => {
                for (entity_id, aname, new_state) in triggered_list {
                    emit_response(&IpcResponse::ActionTriggered {
                        id: entity_id,
                        asset_name: aname,
                        action: action.clone(),
                        state: new_state,
                    });
                }
            }
            Err(err) => emit_error("ACTION_FAILED", err),
        },
        IpcCommand::EditorEvent { event, context } => {
            world.handle_editor_event(&event, EventContext::from_json(context.as_ref()));
        }
        IpcCommand::UpdateGrid {
            width,
            height,
            cell_width,
            cell_height,
            ..
        } => {
            world.set_grid(
                width,
                height,
                cell_width.map(|v| v as f32),
                cell_height.map(|v| v as f32),
                max_w,
                max_h,
            );
        }
        IpcCommand::Ping => emit_response(&IpcResponse::Pong),
        IpcCommand::GetStatus => {
            let summaries = world.get_summaries();
            emit_response(&IpcResponse::StatusReport {
                count: summaries.len(),
                entities: summaries,
            });
        }
        IpcCommand::Shutdown => {
            *control_flow = ControlFlow::Exit;
        }
    }
}
