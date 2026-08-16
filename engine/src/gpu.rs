//! GPU renderer.
//!
//! Draws one instanced textured quad per visible entity from a sprite atlas
//! uploaded once, rather than compositing on the CPU and re-uploading a
//! full-screen framebuffer every frame.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::atlas::Atlas;
use crate::ecs::World;

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

const INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

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
struct Uniforms {
    viewport: [f32; 2],
    flags: [f32; 2],
}

/// Grows in powers of two so a busy scene does not reallocate every frame.
const MIN_INSTANCE_CAPACITY: usize = 64;

/// Builds the per-frame instance list from the world, z-sorted.
///
/// Kept free of any wgpu type so the mapping from entity state to draw call can
/// be tested without a GPU.
pub fn build_instances(world: &World, atlas: &Atlas) -> Vec<SpriteInstance> {
    let mut sorted: Vec<&crate::ecs::Entity> =
        world.entities.iter().filter(|e| e.is_active).collect();
    sorted.sort_by_key(|e| e.z_index);

    let (scale_x, scale_y) = (world.sprite_scale_x, world.sprite_scale_y);
    let mut out = Vec::with_capacity(sorted.len());

    for entity in sorted {
        let Some(asset) = world.asset_manager.get(&entity.asset_name) else {
            continue;
        };
        let Some(state_def) = asset.manifest.states.get(&entity.current_state) else {
            continue;
        };
        let anim = &state_def.animation;
        if anim.frames.is_empty() {
            continue;
        }

        let frame = anim.frames[entity.frame_idx % anim.frames.len()];
        let flip = entity.flip_x ^ anim.flip_x;
        let Some(uv) = atlas.uv(&entity.asset_name, frame, flip) else {
            continue;
        };

        // Depth is drawn as well as simulated: a distant sprite is smaller by
        // the same factor that damps its motion, which is the whole reason the
        // overlay can express parallax and the half-block renderer cannot.
        out.push(SpriteInstance {
            pos: [entity.x, entity.y],
            size: [
                asset.frame_w as f32 * scale_x * entity.parallax,
                asset.frame_h as f32 * scale_y * entity.parallax,
            ],
            uv_min: [uv[0], uv[1]],
            uv_max: [uv[2], uv[3]],
        });
    }

    out
}

pub struct GpuRenderer {
    pub surface: wgpu::Surface,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub width: u32,
    pub height: u32,
    /// Atlas generation currently uploaded, so the atlas is rebuilt only when
    /// the asset set actually changes.
    pub atlas_generation: Option<u64>,
    pub max_texture_dim: u32,

    sprite_pipeline: wgpu::RenderPipeline,
    resolve_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,

    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,

    sprite_uniforms: wgpu::Buffer,
    resolve_uniforms: wgpu::Buffer,

    atlas_bind_group: Option<wgpu::BindGroup>,
    /// Kept so per-frame instance building can look up UV rectangles without
    /// rebuilding or re-uploading anything.
    atlas: Option<Atlas>,
    /// Offscreen target the sprites composite into before being resolved to the
    /// swapchain in the surface's own alpha convention.
    scene_texture: wgpu::Texture,
    scene_bind_group: wgpu::BindGroup,
}

impl GpuRenderer {
    pub async fn new(window: &Window, width: u32, height: u32) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            dx12_shader_compiler: Default::default(),
        });

        let surface = unsafe {
            instance
                .create_surface(window)
                .map_err(|e| format!("Failed to create wgpu surface: {}", e))?
        };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| "Failed to find a suitable GPU adapter".to_string())?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Distract GPU Device"),
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .map_err(|e| format!("Failed to create device: {}", e))?;

        let max_texture_dim = device.limits().max_texture_dimension_2d;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        // Prefer a premultiplied surface, because that is what the compositing
        // pass naturally produces. When only a straight-alpha mode is offered,
        // the resolve pass divides the alpha back out instead of leaving the
        // two conventions mismatched.
        let alpha_mode = if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
        {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::Inherit)
        {
            wgpu::CompositeAlphaMode::Inherit
        } else {
            surface_caps.alpha_modes[0]
        };
        let needs_unpremultiply = alpha_mode == wgpu::CompositeAlphaMode::PostMultiplied;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

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

        // Sprites composite into a linear-working sRGB target with
        // premultiplied blending, which is the only way overlapping
        // semi-transparent sprites come out right.
        let sprite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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

        let resolve_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                    format: config.format,
                    // The triangle covers the whole target, so there is nothing
                    // to blend against.
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

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Index Buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sprite Instance Buffer"),
            size: (MIN_INSTANCE_CAPACITY * std::mem::size_of::<SpriteInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sprite_uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Sprite Uniforms"),
            contents: bytemuck::bytes_of(&Uniforms {
                viewport: [width.max(1) as f32, height.max(1) as f32],
                flags: [0.0, 0.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let resolve_uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Resolve Uniforms"),
            contents: bytemuck::bytes_of(&Uniforms {
                viewport: [width.max(1) as f32, height.max(1) as f32],
                flags: [if needs_unpremultiply { 1.0 } else { 0.0 }, 0.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
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

        let (scene_texture, scene_bind_group) = Self::create_scene_target(
            &device,
            &bind_group_layout,
            &sampler,
            &resolve_uniforms,
            width,
            height,
        );

        Ok(Self {
            surface,
            device,
            queue,
            config,
            width: width.max(1),
            height: height.max(1),
            atlas_generation: None,
            max_texture_dim,
            sprite_pipeline,
            resolve_pipeline,
            bind_group_layout,
            sampler,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            instance_capacity: MIN_INSTANCE_CAPACITY,
            sprite_uniforms,
            resolve_uniforms,
            atlas_bind_group: None,
            atlas: None,
            scene_texture,
            scene_bind_group,
        })
    }

    fn create_scene_target(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        uniforms: &wgpu::Buffer,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::BindGroup) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Scene Texture"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SCENE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniforms.as_entire_binding(),
                },
            ],
            label: Some("Scene Bind Group"),
        });
        (texture, bind_group)
    }

    /// Uploads a new sprite atlas. Cheap to call: it returns immediately when
    /// the asset set has not changed since the last upload.
    pub fn sync_atlas(&mut self, world: &World) -> Result<(), String> {
        let generation = world.asset_manager.generation();
        if self.atlas_generation == Some(generation) && self.atlas_bind_group.is_some() {
            return Ok(());
        }

        let atlas = Atlas::build(&world.asset_manager, self.max_texture_dim)?;
        self.upload_atlas(&atlas);
        self.atlas = Some(atlas);
        self.atlas_generation = Some(generation);
        Ok(())
    }

    /// Draws the world's current state. One instanced draw call, whatever the
    /// entity count.
    pub fn render_world(&mut self, world: &World) -> Result<(), wgpu::SurfaceError> {
        let instances = match &self.atlas {
            Some(atlas) => build_instances(world, atlas),
            None => Vec::new(),
        };
        self.render(&instances)
    }

    fn upload_atlas(&mut self, atlas: &Atlas) {
        let (w, h) = (atlas.width().max(1), atlas.height().max(1));
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Sprite Atlas"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // sRGB: sprite art is authored in sRGB, so the sampler has to
            // decode it to linear before blending and the target re-encodes on
            // write. Declaring it Unorm made every sprite render washed out.
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            atlas.image.as_raw(),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.atlas_bind_group = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.sprite_uniforms.as_entire_binding(),
                },
            ],
            label: Some("Atlas Bind Group"),
        }));
    }

    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        if new_width == 0 || new_height == 0 {
            return;
        }
        self.width = new_width;
        self.height = new_height;
        self.config.width = new_width;
        self.config.height = new_height;
        self.surface.configure(&self.device, &self.config);

        self.queue.write_buffer(
            &self.sprite_uniforms,
            0,
            bytemuck::bytes_of(&Uniforms {
                viewport: [new_width as f32, new_height as f32],
                flags: [0.0, 0.0],
            }),
        );

        let (texture, bind_group) = Self::create_scene_target(
            &self.device,
            &self.bind_group_layout,
            &self.sampler,
            &self.resolve_uniforms,
            new_width,
            new_height,
        );
        self.scene_texture = texture;
        self.scene_bind_group = bind_group;
    }

    fn ensure_instance_capacity(&mut self, needed: usize) {
        if needed <= self.instance_capacity {
            return;
        }
        let capacity = needed.next_power_of_two().max(MIN_INSTANCE_CAPACITY);
        self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sprite Instance Buffer"),
            size: (capacity * std::mem::size_of::<SpriteInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.instance_capacity = capacity;
    }

    pub fn render(&mut self, instances: &[SpriteInstance]) -> Result<(), wgpu::SurfaceError> {
        self.ensure_instance_capacity(instances.len());
        if !instances.is_empty() {
            self.queue
                .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
        }

        let output = self.surface.get_current_texture()?;
        let surface_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let scene_view = self
            .scene_texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Pass 1: composite sprites.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Sprite Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &scene_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });

            if let (Some(bind_group), false) = (&self.atlas_bind_group, instances.is_empty()) {
                pass.set_pipeline(&self.sprite_pipeline);
                pass.set_bind_group(0, bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..INDICES.len() as u32, 0, 0..instances.len() as u32);
            }
        }

        // Pass 2: hand the composited scene to the swapchain in the alpha
        // convention the surface asked for.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Resolve Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });
            pass.set_pipeline(&self.resolve_pipeline);
            pass.set_bind_group(0, &self.scene_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

/// Working format for the compositing pass. sRGB so blending happens in linear
/// space and the encode is done by the hardware on write.
const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::Atlas;
    use crate::ecs::World;
    use crate::spawn::SpawnOptions;

    fn world_with(entities: &[(&str, f32, f32)]) -> World {
        let mut world = World::new(800.0, 600.0);
        world.sprite_scale_x = 1.0;
        world.sprite_scale_y = 1.0;
        for (name, x, y) in entities {
            world.spawn(name, None, SpawnOptions::at(*x, *y)).unwrap();
        }
        world
    }

    #[test]
    fn one_instance_per_visible_entity() {
        let world = world_with(&[("cat", 10.0, 20.0), ("crab", 30.0, 40.0)]);
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();
        let instances = build_instances(&world, &atlas);
        assert_eq!(instances.len(), 2);
    }

    #[test]
    fn instances_carry_pixel_position_and_scaled_size() {
        let mut world = world_with(&[("cat", 10.0, 20.0)]);
        world.sprite_scale_x = 4.0;
        world.sprite_scale_y = 4.0;
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();
        let instances = build_instances(&world, &atlas);

        let cat = world.asset_manager.get("cat").unwrap();
        assert_eq!(instances[0].pos, [10.0, 20.0]);
        assert_eq!(
            instances[0].size,
            [cat.frame_w as f32 * 4.0, cat.frame_h as f32 * 4.0]
        );
    }

    #[test]
    fn parallax_draws_a_distant_sprite_smaller() {
        let mut world = world_with(&[("cat", 10.0, 20.0)]);
        world.entities[0].parallax = 0.5;
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();
        let instances = build_instances(&world, &atlas);

        let cat = world.asset_manager.get("cat").unwrap();
        assert_eq!(
            instances[0].size,
            [cat.frame_w as f32 * 0.5, cat.frame_h as f32 * 0.5],
            "depth has to be visible, not only felt in the physics"
        );
    }

    #[test]
    fn instances_are_sorted_back_to_front_by_z_index() {
        // The sun is z -10, the cat z 10, so the sun must be drawn first.
        let world = world_with(&[("cat", 10.0, 10.0), ("sun", 20.0, 20.0)]);
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();
        let instances = build_instances(&world, &atlas);
        assert_eq!(instances[0].pos, [20.0, 20.0], "sun should draw first");
        assert_eq!(instances[1].pos, [10.0, 10.0]);
    }

    #[test]
    fn a_flipped_entity_gets_mirrored_uvs_not_a_second_frame() {
        let mut world = world_with(&[("cat", 10.0, 10.0)]);
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();

        let facing = build_instances(&world, &atlas);
        world.entities[0].flip_x = true;
        let mirrored = build_instances(&world, &atlas);

        assert_eq!(mirrored[0].uv_min[0], facing[0].uv_max[0]);
        assert_eq!(mirrored[0].uv_max[0], facing[0].uv_min[0]);
        assert_eq!(mirrored[0].uv_min[1], facing[0].uv_min[1]);
    }

    #[test]
    fn an_entity_in_an_unknown_state_is_skipped_rather_than_drawn_wrong() {
        let mut world = world_with(&[("cat", 10.0, 10.0)]);
        world.entities[0].current_state = "no_such_state".to_string();
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();
        assert!(build_instances(&world, &atlas).is_empty());
    }

    #[test]
    fn inactive_entities_produce_no_instances() {
        let mut world = world_with(&[("cat", 10.0, 10.0)]);
        world.entities[0].is_active = false;
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();
        assert!(build_instances(&world, &atlas).is_empty());
    }

    #[test]
    fn per_frame_upload_is_bytes_not_megabytes() {
        // The point of the rewrite: a full-screen framebuffer at 4K is 33 MB a
        // frame. Three sprites should cost under a hundred bytes.
        let world = world_with(&[("cat", 1.0, 1.0), ("crab", 2.0, 2.0), ("sun", 3.0, 3.0)]);
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();
        let instances = build_instances(&world, &atlas);
        let bytes = std::mem::size_of_val(&instances[..]);
        assert_eq!(bytes, 3 * 32);
        assert!(bytes < 128);
    }

    #[test]
    fn animation_position_maps_through_the_manifest_frame_list() {
        let mut world = world_with(&[("cat", 0.0, 0.0)]);
        let atlas = Atlas::build(&world.asset_manager, 8192).unwrap();

        let cat = world.asset_manager.get("cat").unwrap();
        let idle = cat.manifest.states["idle"].animation.frames.clone();
        assert!(idle.len() > 1, "idle should be animated");

        world.entities[0].frame_idx = 0;
        let first = build_instances(&world, &atlas)[0];
        world.entities[0].frame_idx = 1;
        let second = build_instances(&world, &atlas)[0];

        assert_ne!(
            first.uv_min, second.uv_min,
            "advancing the animation must select a different atlas rect"
        );
        assert_eq!(first.uv_min[0], atlas.uv("cat", idle[0], false).unwrap()[0]);
        assert_eq!(
            second.uv_min[0],
            atlas.uv("cat", idle[1], false).unwrap()[0]
        );
    }
}
