#![allow(unexpected_cfgs)]

pub mod asset;
pub mod compositor;
pub mod ecs;
pub mod gpu;
pub mod ipc;
pub mod manifest;
pub mod platform;

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;


use std::io::{self, BufRead, Write};
use std::thread;
use std::time::{Duration, Instant};

use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
};

use crate::compositor::Compositor;
use crate::ecs::World;
use crate::gpu::GpuRenderer;
use crate::ipc::{IpcCommand, IpcResponse};

fn emit_response(resp: &IpcResponse) {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    let _ = lock.write_all(resp.to_json_line().as_bytes());
    let _ = lock.flush();
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

    // Spawn async Stdin reader thread for JSON-RPC
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
                            emit_response(&IpcResponse::Error {
                                code: "PARSE_ERROR".to_string(),
                                message: format!("Failed to parse JSON: {}", err),
                            });
                        }
                    }
                }
                Err(_) => break,
            }
        }
        let _ = proxy.send_event(IpcCommand::Shutdown);
    });

    // Detect monitor dimensions or fallback to standard HD resolution
    let primary_monitor = event_loop.primary_monitor().or_else(|| event_loop.available_monitors().next());
    let (mut window_width, mut window_height) = if let Some(mon) = primary_monitor {
        let size = mon.size();
        (size.width.max(800), size.height.max(600))
    } else {
        (1920, 1080)
    };

    // Cap window dimensions to safe bounds if necessary
    window_width = window_width.clamp(800, 3840);
    window_height = window_height.clamp(600, 2160);

    let window = match platform::create_overlay_window(&event_loop, window_width, window_height) {
        Ok(win) => win,
        Err(err) => {
            emit_response(&IpcResponse::Error {
                code: "WINDOW_CREATION_FAILED".to_string(),
                message: format!("Could not initialize overlay window: {}", err),
            });
            return;
        }
    };

    let mut gpu = match pollster::block_on(GpuRenderer::new(&window, window_width, window_height)) {
        Ok(g) => {
            platform::configure_layer_transparency(&window);
            g
        }
        Err(err) => {
            emit_response(&IpcResponse::Error {
                code: "GPU_INIT_FAILED".to_string(),
                message: format!("Could not initialize GPU renderer: {}", err),
            });
            return;
        }
    };

    let mut frame_buffer = vec![0u8; (window_width * window_height * 4) as usize];
    let mut world = World::new(window_width as f32, window_height as f32);

    // Announce ready state
    emit_response(&IpcResponse::Ready {
        version: "0.2.0".to_string(),
    });

    let target_fps = 60;
    let frame_duration = Duration::from_micros(1_000_000 / target_fps);
    let mut last_tick = Instant::now();

    event_loop.run(move |event, _, control_flow| {
        let now = Instant::now();
        let next_frame = last_tick + frame_duration;
        *control_flow = ControlFlow::WaitUntil(next_frame);

        match event {
            Event::UserEvent(cmd) => match cmd {
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
                            let mut m = manifest::AssetManifest::default_cat();
                            m.name = entity_type.clone();
                            m.spritesheet.path = Some(p);
                            manifest_to_use = Some(m);
                        }
                    }

                    match world.spawn(&entity_type, manifest_to_use, x, y, flip_x) {
                        Ok(id) => {
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
                        Err(err) => {
                            emit_response(&IpcResponse::Error {
                                code: "SPAWN_FAILED".to_string(),
                                message: err,
                            });
                        }
                    }
                }
                IpcCommand::Despawn { id } => {
                    if world.despawn(id) {
                        emit_response(&IpcResponse::Despawned { id });
                    } else {
                        emit_response(&IpcResponse::Error {
                            code: "NOT_FOUND".to_string(),
                            message: format!("Entity #{} not found", id),
                        });
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
                } => {
                    match world.trigger_action(id, asset_name.as_deref(), &action) {
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
                        Err(err) => {
                            emit_response(&IpcResponse::Error {
                                code: "ACTION_FAILED".to_string(),
                                message: err,
                            });
                        }
                    }
                }

                IpcCommand::EditorEvent { event, .. } => {
                    world.handle_editor_event(&event);
                }
                IpcCommand::UpdateGrid {
                    width,
                    height,
                    cell_width,
                    cell_height,
                    ..
                } => {
                    if let Some(cw) = cell_width {
                        world.cell_w = cw as f32;
                    }
                    if let Some(ch) = cell_height {
                        world.cell_h = ch as f32;
                    }
                    let calculated_w = (width as f32 * world.cell_w).max(100.0);
                    let calculated_h = (height as f32 * world.cell_h).max(100.0);
                    world.viewport_w = calculated_w.min(window_width as f32);
                    world.viewport_h = calculated_h.min(window_height as f32);
                }
                IpcCommand::Ping => {
                    emit_response(&IpcResponse::Pong);
                }
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
            },
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
                        gpu.resize(window_width, window_height);
                        frame_buffer.resize((window_width * window_height * 4) as usize, 0);
                    }
                }
                _ => {}
            },
            Event::MainEventsCleared => {
                if now >= next_frame {
                    let dt = now.duration_since(last_tick).as_secs_f32().min(0.1);
                    last_tick = now;
                    world.update(dt);
                    window.request_redraw();
                }
            }
            Event::RedrawRequested(_) => {
                Compositor::render_world(&world, &mut frame_buffer, window_width, window_height);

                if let Err(err) = gpu.render(&frame_buffer) {
                    match err {
                        wgpu::SurfaceError::Lost => gpu.resize(window_width, window_height),
                        wgpu::SurfaceError::OutOfMemory => {
                            emit_response(&IpcResponse::Error {
                                code: "RENDER_ERROR".to_string(),
                                message: "wgpu out of memory".to_string(),
                            });
                            *control_flow = ControlFlow::Exit;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::AssetManifest;

    #[test]
    fn test_manifest_deserialization() {
        let cat = AssetManifest::default_cat();
        assert_eq!(cat.name, "cat");
        assert!(cat.states.contains_key("idle"));
        assert!(cat.states.contains_key("walk"));
        assert!(cat.states.contains_key("jump"));
        assert!(cat.custom_actions.contains_key("jump"));
        assert!(cat.custom_actions.contains_key("yawn"));
    }

    #[test]
    fn test_world_spawn_and_state_machine() {
        let mut world = World::new(800.0, 600.0);
        let id = world.spawn("cat", None, Some(100.0), Some(200.0), Some(false)).unwrap();
        assert_eq!(id, 1);
        assert_eq!(world.entities.len(), 1);

        let entity = &world.entities[0];
        assert_eq!(entity.current_state, "idle");

        // Trigger typing event -> cat should transition to walk_fast
        world.handle_editor_event("typing");
        assert_eq!(world.entities[0].current_state, "walk_fast");

        // Simulate 0.1s delta time
        world.update(0.1);
        assert!(world.entities[0].vx > 0.0);
    }

    #[test]
    fn test_custom_actions_dispatch() {
        let mut world = World::new(800.0, 600.0);
        let cat_id = world.spawn("cat", None, Some(50.0), Some(50.0), None).unwrap();
        let crab_id = world.spawn("crab", None, Some(150.0), Some(50.0), None).unwrap();
        let sun_id = world.spawn("sun", None, Some(300.0), Some(50.0), None).unwrap();

        // Cat jump action
        let cat_triggered = world.trigger_action(Some(cat_id), None, "jump").unwrap();
        assert_eq!(cat_triggered.len(), 1);
        let (id, name, state) = &cat_triggered[0];
        assert_eq!(*id, cat_id);
        assert_eq!(name, "cat");
        assert_eq!(state, "jump");
        assert_eq!(world.entities[0].current_state, "jump");

        // Crab clip claws action
        let crab_triggered = world.trigger_action(Some(crab_id), None, "clip").unwrap();
        assert_eq!(crab_triggered.len(), 1);
        let (id, name, state) = &crab_triggered[0];
        assert_eq!(*id, crab_id);
        assert_eq!(name, "crab");
        assert_eq!(state, "clip_claws");

        // Sun eclipse action
        let sun_triggered = world.trigger_action(Some(sun_id), None, "eclipse").unwrap();
        assert_eq!(sun_triggered.len(), 1);
        let (id, name, state) = &sun_triggered[0];
        assert_eq!(*id, sun_id);
        assert_eq!(name, "sun");
        assert_eq!(state, "eclipse");
    }


    #[test]
    fn test_alpha_compositing() {
        use image::{ImageBuffer, Rgba};
        let mut frame = vec![0u8; 4 * 4 * 4]; // 4x4 RGBA frame

        let mut sprite = ImageBuffer::new(2, 2);
        sprite.put_pixel(0, 0, Rgba([255, 0, 0, 255])); // Opaque red
        sprite.put_pixel(1, 0, Rgba([0, 255, 0, 128])); // Semi-transparent green

        Compositor::blend_sprite(&mut frame, 4, 4, &sprite, 0, 0);

        // Check (0, 0) is solid red
        assert_eq!(frame[0], 255);
        assert_eq!(frame[1], 0);
        assert_eq!(frame[2], 0);
        assert_eq!(frame[3], 255);

        // Check (1, 0) has alpha 128
        assert_eq!(frame[4], 0);
        assert_eq!(frame[5], 255);
        assert_eq!(frame[6], 0);
        assert_eq!(frame[7], 128);
    }
}
