//! Everything the GPU renderer builds once, and the choices it makes to build it.
//!
//! Split from `gpu.rs` because `GpuRenderer::new` was two thirds of that file and
//! none of it was reachable from a test: the whole function needs a `Window`, so
//! the only suites that touched it were the screenshot writer and the headless
//! ones, and the headless ones build their *own* pipeline descriptors rather than
//! calling this code.
//!
//! Everything here takes a `Device` or a plain slice instead, so
//! `engine/tests/gpu_setup.rs` exercises the production pipelines on a real
//! adapter and the two format choices need no GPU at all.

use bytemuck::{Pod, Zeroable};

use wgpu::util::DeviceExt;

/// Working format for the compositing pass, which sprites and models both draw
/// into before the resolve pass converts to whatever the surface wants.
///
/// sRGB, so blending happens in linear space and the encode is done by the
/// hardware on write.
pub const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// A corner of the unit quad every sprite is drawn from.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub corner: [f32; 2],
}

const VERTICES: &[Vertex] = &[
    Vertex { corner: [0.0, 0.0] },
    Vertex { corner: [0.0, 1.0] },
    Vertex { corner: [1.0, 1.0] },
    Vertex { corner: [1.0, 0.0] },
];

/// Two triangles addressing the quad's four corners.
pub const INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

/// Placement of one sprite for one frame. This is the only data that crosses to
/// the GPU per frame: 32 bytes per visible entity.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct SpriteInstance {
    /// Top-left corner in physical pixels.
    pub pos: [f32; 2],
    /// Size in physical pixels.
    pub size: [f32; 2],
    /// Atlas rectangle. `uv_min.x > uv_max.x` mirrors the sprite.
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Uniforms {
    pub viewport: [f32; 2],
    pub flags: [f32; 2],
}

/// Grows in powers of two so a busy scene does not reallocate every frame.
pub const MIN_INSTANCE_CAPACITY: usize = 64;

/// The surface's alpha convention, and whether the resolve pass has to undo the
/// premultiply to satisfy it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlphaChoice {
    pub mode: wgpu::CompositeAlphaMode,
    pub needs_unpremultiply: bool,
}

/// Picks the surface format to present in.
///
/// An sRGB format is preferred so the shader's linear blending is converted on
/// write rather than by hand.
///
/// # Panics
///
/// Panics on an empty list. A configured surface always reports at least one
/// format, and an empty list means the adapter is lying rather than that there
/// is a sensible fallback.
pub fn choose_surface_format(offered: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    assert!(
        !offered.is_empty(),
        "a surface reported no supported formats"
    );
    offered
        .iter()
        .copied()
        .find(|format| format.is_srgb())
        .unwrap_or(offered[0])
}

/// Picks the alpha mode to present in, preferring the one the compositing pass
/// already produces.
///
/// Premultiplied is what the sprite pass naturally writes. When only straight
/// alpha is offered the resolve pass divides the alpha back out, which is
/// `needs_unpremultiply`; leaving the two conventions mismatched would darken
/// every semi-transparent edge against the desktop.
///
/// # Panics
///
/// Panics on an empty list, for the reason `choose_surface_format` does.
pub fn choose_alpha_mode(offered: &[wgpu::CompositeAlphaMode]) -> AlphaChoice {
    assert!(
        !offered.is_empty(),
        "a surface reported no supported alpha modes"
    );
    for mode in [
        wgpu::CompositeAlphaMode::PreMultiplied,
        wgpu::CompositeAlphaMode::PostMultiplied,
        wgpu::CompositeAlphaMode::Inherit,
    ] {
        if offered.contains(&mode) {
            return AlphaChoice {
                mode,
                needs_unpremultiply: mode == wgpu::CompositeAlphaMode::PostMultiplied,
            };
        }
    }
    AlphaChoice {
        mode: offered[0],
        needs_unpremultiply: false,
    }
}

/// The sprite and resolve pipelines, and what they bind through.
pub struct Pipelines {
    pub sprite: wgpu::RenderPipeline,
    pub resolve: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub sampler: wgpu::Sampler,
}

/// Builds both sprite pipelines against the real `shader.wgsl`.
///
/// # Errors
///
/// Never returns `Err`; shader and pipeline faults are reported by wgpu's own
/// validation, which panics through the device error scope.
pub fn build_pipelines(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Pipelines {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Distract Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Texture Bind Group Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    // Sprites composite into a linear-working sRGB target with premultiplied
    // blending, which is the only way overlapping semi-transparent sprites come
    // out right.
    let sprite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Sprite Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_sprite",
            buffers: &[
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                },
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<SpriteInstance>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        1 => Float32x2, 2 => Float32x2, 3 => Float32x2, 4 => Float32x2
                    ],
                },
            ],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_sprite",
            targets: &[Some(wgpu::ColorTargetState {
                format: SCENE_FORMAT,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    let resolve = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Resolve Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_resolve",
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_resolve",
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                // The triangle covers the whole target, so there is nothing to
                // blend against.
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        // Pixel art: any filtering turns crisp edges to mush.
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    Pipelines {
        sprite,
        resolve,
        bind_group_layout,
        sampler,
    }
}

/// The buffers that live for the renderer's whole life.
pub struct Buffers {
    pub vertex: wgpu::Buffer,
    pub index: wgpu::Buffer,
    pub instance: wgpu::Buffer,
    pub sprite_uniforms: wgpu::Buffer,
    pub resolve_uniforms: wgpu::Buffer,
}

/// Builds the quad, the instance buffer at its minimum capacity, and both
/// uniform buffers seeded for this viewport.
pub fn build_buffers(
    device: &wgpu::Device,
    viewport: (u32, u32),
    needs_unpremultiply: bool,
) -> Buffers {
    let (width, height) = viewport;
    let dimensions = [width.max(1) as f32, height.max(1) as f32];

    Buffers {
        vertex: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        index: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Index Buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        }),
        instance: device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sprite Instance Buffer"),
            size: (MIN_INSTANCE_CAPACITY * std::mem::size_of::<SpriteInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
        sprite_uniforms: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Sprite Uniforms"),
            contents: bytemuck::bytes_of(&Uniforms {
                viewport: dimensions,
                flags: [0.0, 0.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        }),
        resolve_uniforms: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Resolve Uniforms"),
            contents: bytemuck::bytes_of(&Uniforms {
                viewport: dimensions,
                flags: [if needs_unpremultiply { 1.0 } else { 0.0 }, 0.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        }),
    }
}
