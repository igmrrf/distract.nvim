//! Characterization of what the overlay renderer builds once.
//!
//! This is the suite `gpu.rs` did not have. Its construction path needs a
//! `Window`, so nothing could reach it: `gpu_headless.rs` runs the real
//! `shader.wgsl` but builds its own pipeline descriptors, which means a
//! divergence between those descriptors and the production ones passes both
//! suites silently. `gpu_setup.rs` takes a `Device` instead of a surface, so
//! everything here calls the production functions.
//!
//! The two format choices are pure and always run. The pipeline tests need an
//! adapter and **skip** without one, the same compromise `gpu_headless` and
//! `gpu3d_headless` make -- so a green run on a runner without a GPU says
//! nothing about whether the pipelines build.

use distract_engine::gpu_bindings::TextureBinding;
use distract_engine::gpu_setup::{
    self, AlphaChoice, MIN_INSTANCE_CAPACITY, SCENE_FORMAT, SpriteInstance, Uniforms,
};
use wgpu::CompositeAlphaMode::{Auto, Inherit, Opaque, PostMultiplied, PreMultiplied};
use wgpu::TextureFormat::{Bgra8Unorm, Bgra8UnormSrgb, Rgba8Unorm, Rgba8UnormSrgb};
use wgpu::util::DeviceExt;

const TARGET: u32 = 16;

struct Harness {
    device: wgpu::Device,
    queue: wgpu::Queue,
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
            label: Some("gpu_setup characterization device"),
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::downlevel_defaults(),
        },
        None,
    ))
    .ok()?;
    Some(Harness { device, queue })
}

#[test]
fn an_srgb_format_is_preferred_over_an_earlier_linear_one() {
    assert_eq!(
        Bgra8UnormSrgb,
        gpu_setup::choose_surface_format(&[Bgra8Unorm, Rgba8Unorm, Bgra8UnormSrgb]),
    );
}

#[test]
fn the_first_format_is_taken_when_none_is_srgb() {
    assert_eq!(
        Bgra8Unorm,
        gpu_setup::choose_surface_format(&[Bgra8Unorm, Rgba8Unorm]),
    );
}

#[test]
fn an_srgb_only_surface_is_taken_as_offered() {
    assert_eq!(
        Rgba8UnormSrgb,
        gpu_setup::choose_surface_format(&[Rgba8UnormSrgb]),
    );
}

#[test]
fn premultiplied_wins_whenever_it_is_offered() {
    // Offered last, and still chosen: the ladder is a preference, not a scan.
    assert_eq!(
        AlphaChoice {
            mode: PreMultiplied,
            needs_unpremultiply: false,
        },
        gpu_setup::choose_alpha_mode(&[Opaque, Inherit, PostMultiplied, PreMultiplied]),
    );
}

#[test]
fn straight_alpha_is_accepted_and_asks_the_resolve_pass_to_undo_the_premultiply() {
    assert_eq!(
        AlphaChoice {
            mode: PostMultiplied,
            needs_unpremultiply: true,
        },
        gpu_setup::choose_alpha_mode(&[Opaque, Inherit, PostMultiplied]),
    );
}

#[test]
fn inherit_is_preferred_over_opaque_because_opaque_loses_transparency() {
    assert_eq!(
        AlphaChoice {
            mode: Inherit,
            needs_unpremultiply: false,
        },
        gpu_setup::choose_alpha_mode(&[Opaque, Inherit]),
    );
}

#[test]
fn a_surface_offering_none_of_the_three_falls_back_to_what_it_does_offer() {
    // Nothing to undo: only PostMultiplied states a convention this pass has to
    // convert into.
    assert_eq!(
        AlphaChoice {
            mode: Opaque,
            needs_unpremultiply: false,
        },
        gpu_setup::choose_alpha_mode(&[Opaque, Auto]),
    );
}

#[test]
#[should_panic(expected = "a surface reported no supported formats")]
fn an_empty_format_list_is_a_lying_adapter_rather_than_a_default() {
    gpu_setup::choose_surface_format(&[]);
}

#[test]
#[should_panic(expected = "a surface reported no supported alpha modes")]
fn an_empty_alpha_mode_list_is_refused_the_same_way() {
    gpu_setup::choose_alpha_mode(&[]);
}

#[test]
fn the_production_pipelines_build_against_the_real_shader() {
    let Some(harness) = harness() else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    // Both entry-point pairs and every binding are validated here, which is what
    // `gpu.rs` could not previously assert without opening a window.
    let _pipelines = gpu_setup::build_pipelines(&harness.device, Bgra8UnormSrgb);
    harness.device.poll(wgpu::Maintain::Wait);
}

#[test]
fn the_resolve_pipeline_is_built_for_whatever_the_surface_chose() {
    let Some(harness) = harness() else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    // A non-sRGB surface is a real configuration -- it is what the fallback in
    // `choose_surface_format` produces -- and the resolve target format is the
    // one thing in the pipelines that varies with it.
    for format in [Bgra8UnormSrgb, Bgra8Unorm, Rgba8UnormSrgb] {
        let _pipelines = gpu_setup::build_pipelines(&harness.device, format);
        harness.device.poll(wgpu::Maintain::Wait);
    }
}

#[test]
fn the_instance_buffer_starts_at_the_documented_minimum_capacity() {
    let Some(harness) = harness() else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    let buffers = gpu_setup::build_buffers(&harness.device, (TARGET, TARGET), false);
    assert_eq!(
        (MIN_INSTANCE_CAPACITY * std::mem::size_of::<SpriteInstance>()) as u64,
        buffers.instance.size(),
        "64 instances at 32 bytes each: the size the growth path measures against",
    );
}

#[test]
fn a_zero_sized_viewport_still_produces_buffers_a_pass_can_bind() {
    let Some(harness) = harness() else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    // A window can report 0x0 while it is being created, and a uniform of zero
    // width would divide by zero in the vertex shader.
    let buffers = gpu_setup::build_buffers(&harness.device, (0, 0), false);
    assert_eq!(
        std::mem::size_of::<Uniforms>() as u64,
        buffers.sprite_uniforms.size(),
    );
    harness.device.poll(wgpu::Maintain::Wait);
}

#[test]
fn the_scene_target_is_the_size_it_was_asked_for_in_the_compositing_format() {
    let Some(harness) = harness() else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    let pipelines = gpu_setup::build_pipelines(&harness.device, Bgra8UnormSrgb);
    let buffers = gpu_setup::build_buffers(&harness.device, (TARGET, TARGET), false);
    let (texture, _bind_group) = TextureBinding {
        layout: &pipelines.bind_group_layout,
        sampler: &pipelines.sampler,
        uniforms: &buffers.resolve_uniforms,
    }
    .scene_target(&harness.device, (TARGET, 4));

    assert_eq!(TARGET, texture.width());
    assert_eq!(4, texture.height());
    assert_eq!(SCENE_FORMAT, texture.format());
}

#[test]
fn a_zero_sized_scene_target_is_clamped_rather_than_refused() {
    let Some(harness) = harness() else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    let pipelines = gpu_setup::build_pipelines(&harness.device, Bgra8UnormSrgb);
    let buffers = gpu_setup::build_buffers(&harness.device, (TARGET, TARGET), false);
    let (texture, _bind_group) = TextureBinding {
        layout: &pipelines.bind_group_layout,
        sampler: &pipelines.sampler,
        uniforms: &buffers.resolve_uniforms,
    }
    .scene_target(&harness.device, (0, 0));

    // wgpu rejects a zero-extent texture outright, so the clamp is what keeps a
    // window still being created from taking the process down.
    assert_eq!(1, texture.width());
    assert_eq!(1, texture.height());
}

#[test]
fn a_sprite_drawn_through_the_production_pipeline_lands_where_it_was_placed() {
    let Some(harness) = harness() else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    let pixels = draw_one_sprite(&harness);

    // A quarter-width sprite at the origin: opaque inside, untouched outside.
    assert_eq!(
        [255, 0, 0, 255],
        pixel(&pixels, 1, 1),
        "the sprite's own area must be its colour",
    );
    assert_eq!(
        [0, 0, 0, 0],
        pixel(&pixels, TARGET - 1, TARGET - 1),
        "the far corner is outside the sprite and must stay clear",
    );
}

/// Draws one 4x4 red sprite at the origin of a 16x16 scene target, through the
/// production pipeline, sampler, buffers and shader.
fn draw_one_sprite(harness: &Harness) -> Vec<u8> {
    let pipelines = gpu_setup::build_pipelines(&harness.device, SCENE_FORMAT);
    let buffers = gpu_setup::build_buffers(&harness.device, (TARGET, TARGET), false);
    // Not `create_scene_target`'s texture: that one carries no `COPY_SRC`,
    // because production resolves it through a bind group and never reads it
    // back. Same format and size, so the pipeline sees what it does in the
    // overlay.
    let scene = harness.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("readable scene target"),
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

    let atlas = opaque_red_texture(harness);
    let atlas_bind_group = harness
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &pipelines.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&pipelines.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buffers.sprite_uniforms.as_entire_binding(),
                },
            ],
            label: None,
        });

    harness.queue.write_buffer(
        &buffers.instance,
        0,
        bytemuck::bytes_of(&SpriteInstance {
            pos: [0.0, 0.0],
            size: [4.0, 4.0],
            uv_min: [0.0, 0.0],
            uv_max: [1.0, 1.0],
        }),
    );

    let view = scene.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = harness
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
        });
        pass.set_pipeline(&pipelines.sprite);
        pass.set_bind_group(0, &atlas_bind_group, &[]);
        pass.set_vertex_buffer(0, buffers.vertex.slice(..));
        pass.set_vertex_buffer(1, buffers.instance.slice(..));
        pass.set_index_buffer(buffers.index.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..gpu_setup::INDICES.len() as u32, 0, 0..1);
    }
    harness.queue.submit([encoder.finish()]);

    read_back(harness, &scene)
}

fn opaque_red_texture(harness: &Harness) -> wgpu::TextureView {
    let texture = harness.device.create_texture_with_data(
        &harness.queue,
        &wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        &[255, 0, 0, 255],
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn read_back(harness: &Harness, texture: &wgpu::Texture) -> Vec<u8> {
    let row_bytes = TARGET * 4;
    let padded = row_bytes.div_ceil(256) * 256;
    let buffer = harness.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (padded * TARGET) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = harness
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &buffer,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(TARGET),
            },
        },
        wgpu::Extent3d {
            width: TARGET,
            height: TARGET,
            depth_or_array_layers: 1,
        },
    );
    harness.queue.submit([encoder.finish()]);

    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    harness.device.poll(wgpu::Maintain::Wait);

    let mapped = buffer.slice(..).get_mapped_range();
    let mut out = Vec::with_capacity((row_bytes * TARGET) as usize);
    for row in 0..TARGET {
        let start = (row * padded) as usize;
        out.extend_from_slice(&mapped[start..start + row_bytes as usize]);
    }
    drop(mapped);
    buffer.unmap();
    out
}

fn pixel(data: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * TARGET + x) * 4) as usize;
    [
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]
}
