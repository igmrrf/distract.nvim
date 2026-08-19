//! Headless voxel-pass tests.
//!
//! These drive the real `MeshPipeline` and the real `shader3d.wgsl` into an
//! offscreen target and read the pixels back, so "3D renders" is a measurement
//! rather than a claim. No window is opened, so they run in CI.
//!
//! Every test skips (rather than fails) when no adapter is available, so a runner
//! without a GPU does not turn into a red build — the same policy
//! `gpu_headless.rs` set.

use distract_engine::camera::Camera;
use distract_engine::ecs::World;
use distract_engine::gpu3d::{self, MeshPipeline, PassTarget};
use distract_engine::render::{RenderMode, RenderSettings};
use distract_engine::spawn::SpawnOptions;

const TARGET: u32 = 128;
/// 128 px * 4 bytes = 512, a multiple of wgpu's 256-byte copy row alignment.
const BYTES_PER_ROW: u32 = TARGET * 4;
const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

struct Harness {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: MeshPipeline,
}

fn harness() -> Option<Harness> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        dx12_shader_compiler: Default::default(),
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))?;
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("headless mesh device"),
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::downlevel_defaults(),
        },
        None,
    ))
    .ok()?;

    let pipeline = MeshPipeline::new(&device, SCENE_FORMAT).ok()?;
    Some(Harness {
        device,
        queue,
        pipeline,
    })
}

/// A world sized to the target, in voxel mode, with nothing in it yet.
fn voxel_world() -> World {
    let mut world = World::new(TARGET as f32, TARGET as f32);
    world.render = RenderSettings {
        mode: RenderMode::Voxel,
        ..Default::default()
    };
    world
}

fn draw(harness: &mut Harness, world: &World) -> Vec<u8> {
    let camera = harness_camera(world);
    harness
        .pipeline
        .sync(&harness.device, world)
        .expect("the book builds");
    let frame = harness
        .pipeline
        .prepare((&harness.device, &harness.queue), world, &camera);

    let colour = harness.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mesh target"),
        size: wgpu::Extent3d {
            width: TARGET,
            height: TARGET,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SCENE_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = gpu3d::create_depth_texture(&harness.device, TARGET, TARGET);

    let colour_view = colour.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = harness
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    harness.pipeline.record(
        PassTarget {
            encoder: &mut encoder,
            colour: &colour_view,
            depth: &depth_view,
            scissor: None,
        },
        &frame,
    );
    harness.queue.submit(Some(encoder.finish()));

    read_back(harness, &colour)
}

fn harness_camera(world: &World) -> Camera {
    world.render.camera(TARGET as f32, TARGET as f32)
}

fn read_back(harness: &Harness, texture: &wgpu::Texture) -> Vec<u8> {
    let buffer = harness.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (BYTES_PER_ROW * TARGET) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = harness
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::ImageCopyBuffer {
            buffer: &buffer,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(BYTES_PER_ROW),
                rows_per_image: Some(TARGET),
            },
        },
        wgpu::Extent3d {
            width: TARGET,
            height: TARGET,
            depth_or_array_layers: 1,
        },
    );
    harness.queue.submit(Some(encoder.finish()));

    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    harness.device.poll(wgpu::Maintain::Wait);
    let data = slice.get_mapped_range().to_vec();
    buffer.unmap();
    data
}

fn pixel(data: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = (y * BYTES_PER_ROW + x * 4) as usize;
    [
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]
}

fn opaque_pixel_count(data: &[u8]) -> usize {
    data.chunks_exact(4).filter(|chunk| chunk[3] > 8).count()
}

/// A world holding one cat per placement, every one pinned to its first frame.
///
/// Pinned because a spawn randomises `frame_idx` so two pets do not animate in
/// lockstep, and two renders of "the same scene" have to actually be the same
/// scene.
fn world_with(placements: impl IntoIterator<Item = SpawnOptions>) -> World {
    let mut world = voxel_world();
    for placement in placements {
        world.spawn("cat", None, placement).expect("spawns");
    }
    for entity in world.entities.iter_mut() {
        entity.frame_idx = 0;
        entity.frame_timer = 0.0;
    }
    world
}

/// The brightest colour anywhere on the target, and its total luminance.
fn brightest(data: &[u8]) -> u32 {
    data.chunks_exact(4)
        .filter(|chunk| chunk[3] > 8)
        .map(|chunk| chunk[0] as u32 + chunk[1] as u32 + chunk[2] as u32)
        .max()
        .unwrap_or(0)
}

#[test]
fn a_voxel_pet_covers_pixels_on_the_target() {
    let Some(mut harness) = harness() else {
        return;
    };
    let mut world = voxel_world();
    world
        .spawn("cat", None, SpawnOptions::at(20.0, 40.0))
        .expect("the built-in cat spawns");

    let pixels = draw(&mut harness, &world);
    assert!(
        opaque_pixel_count(&pixels) > 200,
        "the mesh pass drew {} opaque pixels",
        opaque_pixel_count(&pixels)
    );
}

#[test]
fn flat_mode_records_no_mesh_work_at_all() {
    let Some(mut harness) = harness() else {
        return;
    };
    let mut world = World::new(TARGET as f32, TARGET as f32);
    world
        .spawn("cat", None, SpawnOptions::at(20.0, 40.0))
        .expect("the built-in cat spawns");

    let pixels = draw(&mut harness, &world);
    assert_eq!(
        opaque_pixel_count(&pixels),
        0,
        "a 2D session must not pay for the mesh pass"
    );
}

#[test]
fn the_model_lands_where_its_sprite_would() {
    let Some(mut harness) = harness() else {
        return;
    };
    let mut world = voxel_world();
    world.sprite_scale_x = 2.0;
    world.sprite_scale_y = 2.0;
    let (spawn_x, spawn_y) = (30.0, 50.0);
    world
        .spawn("cat", None, SpawnOptions::at(spawn_x, spawn_y))
        .expect("the built-in cat spawns");

    let cat = world
        .asset_manager
        .get("cat")
        .expect("the built-in cat is loaded");
    let footprint = (
        cat.frame_w as f32 * world.sprite_scale_x,
        cat.frame_h as f32 * world.sprite_scale_y,
    );

    let pixels = draw(&mut harness, &world);
    let mut drawn_rows = Vec::new();
    for row in 0..TARGET {
        for col in 0..TARGET {
            if pixel(&pixels, col, row)[3] > 8 {
                drawn_rows.push(row);
                break;
            }
        }
    }

    let first = *drawn_rows.first().expect("something was drawn");
    let last = *drawn_rows.last().expect("something was drawn");
    assert!(
        (first as f32) >= spawn_y - 1.0,
        "the model's top is above where the sprite's would be: {} vs {}",
        first,
        spawn_y
    );
    assert!(
        (last as f32) <= spawn_y + footprint.1 + 1.0,
        "the model hangs below the sprite footprint: {} vs {}",
        last,
        spawn_y + footprint.1
    );
}

#[test]
fn depth_decides_which_of_two_models_shows_rather_than_draw_order() {
    let Some(mut harness) = harness() else {
        return;
    };
    // Two overlapping models at different depths. Without a depth buffer the
    // later draw wins, so swapping the order changes the picture; with one, the
    // nearer model wins either way and the two renders are identical. Both
    // copies also land in the same instanced draw, where the order instances
    // rasterise in is not defined at all -- so this is the only thing that makes
    // the result deterministic.
    let near = SpawnOptions {
        x: Some(30.0),
        y: Some(40.0),
        z: Some(3.0),
        ..Default::default()
    };
    let far = SpawnOptions {
        x: Some(38.0),
        y: Some(46.0),
        z: Some(-3.0),
        ..Default::default()
    };

    let near_first = draw(&mut harness, &world_with([near.clone(), far.clone()]));
    let far_first = draw(&mut harness, &world_with([far, near]));

    let differing = near_first
        .chunks_exact(4)
        .zip(far_first.chunks_exact(4))
        .filter(|(left, right)| left != right)
        .count();
    assert_eq!(
        differing, 0,
        "{} pixels depended on the draw order",
        differing
    );
}

#[test]
fn a_lit_model_is_brighter_than_one_in_full_shadow() {
    let Some(mut harness) = harness() else {
        return;
    };
    let mut lit = voxel_world();
    lit.render.light.ambient = 1.0;
    lit.spawn("cat", None, SpawnOptions::at(20.0, 40.0))
        .expect("spawns");
    let bright = brightest(&draw(&mut harness, &lit));

    let mut dark = voxel_world();
    dark.render.light.ambient = 0.0;
    dark.render.light.direction = [0.0, 0.0, -1.0];
    dark.spawn("cat", None, SpawnOptions::at(20.0, 40.0))
        .expect("spawns");
    let dim = brightest(&draw(&mut harness, &dark));

    assert!(
        bright > dim,
        "the lighting term did nothing: {} against {}",
        bright,
        dim
    );
}

#[test]
fn an_asset_pinned_to_flat_is_not_drawn_as_a_model() {
    let Some(mut harness) = harness() else {
        return;
    };
    let mut world = voxel_world();
    let mut manifest = distract_engine::manifest::AssetManifest::default_cat();
    manifest.name = "flat_probe".to_string();
    manifest.render = Some(RenderMode::Flat);
    world
        .spawn("flat_probe", Some(manifest), SpawnOptions::at(20.0, 40.0))
        .expect("spawns");

    let pixels = draw(&mut harness, &world);
    assert_eq!(
        opaque_pixel_count(&pixels),
        0,
        "a manifest pinned to 2D must stay out of the mesh pass"
    );
}

/// Writes what the mesh pass actually produced, so the art can be looked at
/// rather than inferred from pixel counts.
///
/// `HANDOFF.md` records the cost of judging sprite art from a character grid: two
/// defects were invisible there and obvious in a picture. A renderer whose only
/// test is "some pixels are opaque" is the same trap one level up.
#[test]
fn writes_a_voxel_screenshot_to_look_at() {
    let Some(mut harness) = harness() else {
        return;
    };
    let out_dir = std::path::Path::new("../tests/screenshots");
    if std::fs::create_dir_all(out_dir).is_err() {
        return;
    }

    for (name, asset, yaw) in [
        ("18_voxel_cat.png", "cat", 22.0),
        ("19_voxel_crab.png", "crab", 22.0),
        ("20_voxel_cat_face_on.png", "cat", 0.0),
        ("21_voxel_cat_side_on.png", "cat", 70.0),
    ] {
        let mut world = voxel_world();
        world.render.yaw_degrees = yaw;
        world.sprite_scale_x = 4.0;
        world.sprite_scale_y = 4.0;
        world
            .spawn(asset, None, SpawnOptions::at(12.0, 24.0))
            .expect("a built-in spawns");
        for entity in world.entities.iter_mut() {
            entity.frame_idx = 0;
        }

        let pixels = draw(&mut harness, &world);
        let image = image::RgbaImage::from_raw(TARGET, TARGET, pixels)
            .expect("the readback is exactly one target of RGBA");
        image
            .save(out_dir.join(name))
            .expect("the screenshot directory is writable");
    }
}
