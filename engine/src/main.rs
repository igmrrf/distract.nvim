//! Overlay engine binary.
//!
//! A thin driver over the `distract_engine` library: it owns the window, the
//! event loop and the stdin reader, and nothing else. Command handling lives in
//! `commands.rs` and every write to stdout in `response.rs`, so this file stays
//! about the platform rather than the protocol.

use std::io::{self, BufRead};
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
};

use distract_engine::commands::{self, CommandContext, Viewport};
use distract_engine::ecs::World;
use distract_engine::gpu::GpuRenderer;
use distract_engine::ipc::{IpcCommand, IpcResponse};
use distract_engine::overlay_placement::{self, PlacementRequest};
use distract_engine::platform;
use distract_engine::response::{emit_error, emit_response, emit_warning};
use distract_engine::subscription::Subscription;

const TARGET_FPS: u64 = 60;
/// The longest step the simulation will take in one frame.
///
/// A resumed session or a blocking command would otherwise teleport every entity
/// clear across the screen in one go.
const MAX_FRAME_SECONDS: f32 = 0.1;

/// A startup failure the engine reported and could not recover from.
///
/// Reported on stdout as an `error` response *and* as a non-zero exit, because
/// Neovim watches both: the response carries the reason and the exit code is
/// what `jobstart`'s `on_exit` can act on. An engine that refused its own
/// arguments and exited 0 looked exactly like one the user had stopped.
fn main() -> ExitCode {
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

    thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let Ok(content) = line else { break };
            let trimmed = content.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<IpcCommand>(trimmed) {
                Ok(command) => {
                    if proxy.send_event(command).is_err() {
                        break;
                    }
                }
                Err(err) => emit_error("PARSE_ERROR", format!("Failed to parse JSON: {}", err)),
            }
        }
        let _ = proxy.send_event(IpcCommand::Shutdown);
    });

    let configured = match overlay_placement::from_args(std::env::args().skip(1)) {
        Ok(configured) => configured,
        Err(message) => {
            emit_error("INVALID_ARGUMENT", message);
            return ExitCode::FAILURE;
        }
    };

    // Which display, and therefore how large. Both come from the same monitor so
    // the overlay cannot be sized for one screen and positioned on another.
    let monitors = platform::monitor_geometries(&event_loop);
    let (placement, guessed) = overlay_placement::resolve(&PlacementRequest {
        configured,
        focused: platform::focused_monitor(&event_loop),
        monitors: &monitors,
    });
    if let Some(message) = guessed {
        emit_warning("OVERLAY_DISPLAY_GUESSED", message);
    }

    let window = match platform::create_overlay_window(&event_loop, placement) {
        Ok(window) => window,
        Err(err) => {
            emit_error(
                "WINDOW_CREATION_FAILED",
                format!("Could not initialize overlay window: {}", err),
            );
            return ExitCode::FAILURE;
        }
    };

    // The window manager is free to grant a different size than was asked for
    // — tiling WMs, HiDPI scaling, reserved struts. Configure everything from
    // what actually exists rather than from the request.
    let granted = window.inner_size();
    let mut viewport = Viewport {
        width: granted.width.max(1) as f32,
        height: granted.height.max(1) as f32,
    };

    let mut gpu_renderer = match pollster::block_on(GpuRenderer::new(
        &window,
        viewport.width as u32,
        viewport.height as u32,
    )) {
        Ok(renderer) => {
            platform::configure_layer_transparency(&window);
            renderer
        }
        Err(err) => {
            emit_error(
                "GPU_INIT_FAILED",
                format!("Could not initialize GPU renderer: {}", err),
            );
            return ExitCode::FAILURE;
        }
    };

    let mut world = World::new(viewport.width, viewport.height);
    if let Err(err) = gpu_renderer.sync_atlas(&world) {
        emit_error("ATLAS_FAILED", err);
        return ExitCode::FAILURE;
    }

    emit_response(&IpcResponse::Ready {
        version: env!("CARGO_PKG_VERSION").to_string(),
    });

    let frame_duration = Duration::from_micros(1_000_000 / TARGET_FPS);
    let mut last_tick = Instant::now();
    let mut subscription = Subscription::default();
    // Whether the last simulated step changed anything worth drawing. An
    // overlay of sleeping cats should cost approximately nothing, so a frame is
    // only submitted when the picture can actually differ.
    let mut needs_redraw = true;

    event_loop.run(move |event, _, control_flow| {
        let now = Instant::now();
        let next_frame = last_tick + frame_duration;
        *control_flow = ControlFlow::WaitUntil(next_frame);

        match event {
            Event::UserEvent(command) => {
                commands::handle(
                    command,
                    &mut CommandContext {
                        world: &mut world,
                        window: &window,
                        renderer: &mut gpu_renderer,
                        control_flow,
                        subscription: &mut subscription,
                        viewport,
                    },
                );
                needs_redraw = true;
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                }
                WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                    viewport = Viewport {
                        width: size.width as f32,
                        height: size.height as f32,
                    };
                    resize(&mut world, &mut gpu_renderer, viewport);
                    needs_redraw = true;
                }
                // Dragging the overlay to a monitor with a different scale
                // factor arrives here, not as a Resized. Ignoring it left the
                // surface configured for the old size with no recovery path.
                WindowEvent::ScaleFactorChanged { new_inner_size, .. }
                    if new_inner_size.width > 0 && new_inner_size.height > 0 =>
                {
                    viewport = Viewport {
                        width: new_inner_size.width as f32,
                        height: new_inner_size.height as f32,
                    };
                    resize(&mut world, &mut gpu_renderer, viewport);
                    needs_redraw = true;
                }
                _ => {}
            },
            Event::MainEventsCleared => {
                if now >= next_frame {
                    let dt = now
                        .duration_since(last_tick)
                        .as_secs_f32()
                        .min(MAX_FRAME_SECONDS);
                    last_tick = now;

                    for id in world.update(dt) {
                        // Without this, Neovim's idea of what is alive silently
                        // diverges from the engine's.
                        emit_response(&IpcResponse::Despawned { id });
                        needs_redraw = true;
                    }

                    report_to_plugins(&mut world, &mut subscription, now);

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
                    needs_redraw =
                        handle_surface_error(err, &mut gpu_renderer, viewport, control_flow);
                }
            }
            _ => {}
        }
    })
}

fn resize(world: &mut World, gpu_renderer: &mut GpuRenderer, viewport: Viewport) {
    world.viewport_w = viewport.width;
    world.viewport_h = viewport.height;
    gpu_renderer.resize(viewport.width as u32, viewport.height as u32);
}

/// Sends what the Lua plugin pipeline subscribed to, if anything.
///
/// Both halves are gated on the subscription: a session with no plugins puts
/// nothing at all on the wire per frame.
fn report_to_plugins(world: &mut World, subscription: &mut Subscription, now: Instant) {
    if let Some(dt) = subscription.poll(now) {
        emit_response(&IpcResponse::Snapshot {
            entities: world.get_summaries(),
            dt,
        });
    }

    let dropped = world.journal.dropped();
    let events = world.journal.drain();
    if !events.is_empty() {
        emit_response(&IpcResponse::WorldEvents { events, dropped });
    }
}

/// Whether the frame is worth attempting again.
fn handle_surface_error(
    err: wgpu::SurfaceError,
    gpu_renderer: &mut GpuRenderer,
    viewport: Viewport,
    control_flow: &mut ControlFlow,
) -> bool {
    match err {
        // Both mean the surface no longer matches the window: Lost after a
        // device reset, Outdated after a monitor, DPI or compositor change.
        // Reconfigure for both.
        wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated => {
            gpu_renderer.resize(viewport.width as u32, viewport.height as u32);
            true
        }
        // Transient; the next frame will pick it up.
        wgpu::SurfaceError::Timeout => true,
        wgpu::SurfaceError::OutOfMemory => {
            emit_error("RENDER_ERROR", "wgpu out of memory");
            *control_flow = ControlFlow::Exit;
            false
        }
    }
}
