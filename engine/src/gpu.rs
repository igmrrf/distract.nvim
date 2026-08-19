//! GPU renderer.
//!
//! Draws one instanced textured quad per visible entity from a sprite atlas
//! uploaded once, rather than compositing on the CPU and re-uploading a
//! full-screen framebuffer every frame.

use winit::window::Window;

use crate::atlas::Atlas;
use crate::bounds::Bounds;
use crate::ecs::World;
use crate::gpu_bindings::TextureBinding;
use crate::gpu_setup::{self, Buffers, Pipelines, Uniforms};
use crate::gpu3d::{self, MeshPipeline, PassTarget};
use crate::mesh_draw::MeshFrame;
use crate::sprite_draw::build_instances;

pub use crate::gpu_setup::{INDICES, MIN_INSTANCE_CAPACITY, SCENE_FORMAT, SpriteInstance, Vertex};

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
    /// The rectangle the sprite pass is clipped to, when Neovim scoped the
    /// viewport to less than the whole window. `None` draws everywhere.
    pub scissor: Option<[u32; 4]>,

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

    /// The voxel pass, and what it depth-tests against.
    mesh: MeshPipeline,
    depth_texture: wgpu::Texture,
    /// This frame's mesh work, built in `render_world` and recorded in `render`.
    /// Held the same way and for the same reason `scissor` is: `render` is one
    /// pass-recording path and both passes need what the world said.
    mesh_frame: MeshFrame,
}

impl GpuRenderer {
    /// Opens a surface on `window` and builds everything drawn through it.
    ///
    /// # Errors
    ///
    /// Returns a message naming the stage that failed: surface creation, adapter
    /// selection, device creation, or the mesh pipeline.
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
        let alpha = gpu_setup::choose_alpha_mode(&surface_caps.alpha_modes);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: gpu_setup::choose_surface_format(&surface_caps.formats),
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: alpha.mode,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let Pipelines {
            sprite: sprite_pipeline,
            resolve: resolve_pipeline,
            bind_group_layout,
            sampler,
        } = gpu_setup::build_pipelines(&device, config.format);

        let Buffers {
            vertex: vertex_buffer,
            index: index_buffer,
            instance: instance_buffer,
            sprite_uniforms,
            resolve_uniforms,
        } = gpu_setup::build_buffers(&device, (width, height), alpha.needs_unpremultiply);

        let (scene_texture, scene_bind_group) = TextureBinding {
            layout: &bind_group_layout,
            sampler: &sampler,
            uniforms: &resolve_uniforms,
        }
        .scene_target(&device, (width, height));

        let mesh = MeshPipeline::new(&device, SCENE_FORMAT)?;
        let depth_texture = gpu3d::create_depth_texture(&device, width, height);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            width: width.max(1),
            height: height.max(1),
            atlas_generation: None,
            scissor: None,
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
            mesh,
            depth_texture,
            mesh_frame: MeshFrame::default(),
        })
    }

    /// Uploads whatever the current asset set needs: the sprite atlas always, and
    /// the voxel meshes when something is drawn as a model.
    ///
    /// Cheap to call: each half returns immediately when nothing it depends on has
    /// changed since its last upload.
    pub fn sync_assets(&mut self, world: &World) -> Result<(), String> {
        self.mesh.sync(&self.device, world)?;

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
        let camera = world.render.camera(self.width as f32, self.height as f32);
        self.mesh_frame = self
            .mesh
            .prepare((&self.device, &self.queue), world, &camera);
        // A scoped viewport is the only reason to clip: without one the bounds
        // are the window and the whole target is fair game. With one, a wrapped
        // quad drawn at a complementary position would otherwise spill into the
        // part of the window the scope excluded.
        self.scissor = world.scope.map(|scope| self.clip_rect(scope));
        self.render(&instances)
    }

    /// A scope clipped to the surface, in the whole pixels a scissor needs.
    fn clip_rect(&self, scope: Bounds) -> [u32; 4] {
        let left = scope.left.max(0.0).min(self.width as f32);
        let top = scope.top.max(0.0).min(self.height as f32);
        let right = scope.right().max(left).min(self.width as f32);
        let bottom = scope.bottom().max(top).min(self.height as f32);
        [
            left as u32,
            top as u32,
            (right - left) as u32,
            (bottom - top) as u32,
        ]
    }

    fn upload_atlas(&mut self, atlas: &Atlas) {
        self.atlas_bind_group = Some(
            self.sprite_binding()
                .atlas(&self.device, &self.queue, atlas),
        );
    }

    /// What the sprite pass binds the atlas through.
    fn sprite_binding(&self) -> TextureBinding<'_> {
        TextureBinding {
            layout: &self.bind_group_layout,
            sampler: &self.sampler,
            uniforms: &self.sprite_uniforms,
        }
    }

    /// What the resolve pass binds the composited scene through.
    fn scene_binding(&self) -> TextureBinding<'_> {
        TextureBinding {
            layout: &self.bind_group_layout,
            sampler: &self.sampler,
            uniforms: &self.resolve_uniforms,
        }
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

        self.depth_texture = gpu3d::create_depth_texture(&self.device, new_width, new_height);

        let (texture, bind_group) = self
            .scene_binding()
            .scene_target(&self.device, (new_width, new_height));
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

        // Pass 1: voxel models, depth-tested. First, so the flat pass draws over
        // them: a sprite in a 3D session is deliberately flat furniture.
        let depth_view = self
            .depth_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.mesh.record(
            PassTarget {
                encoder: &mut encoder,
                colour: &scene_view,
                depth: &depth_view,
                scissor: self.scissor,
            },
            &self.mesh_frame,
        );

        // Pass 2: composite sprites, loading whatever the mesh pass left rather
        // than clearing it away.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Sprite Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &scene_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if self.mesh_frame.is_empty() {
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });

            if let (Some(bind_group), false) = (&self.atlas_bind_group, instances.is_empty()) {
                if let Some([x, y, width, height]) = self.scissor {
                    if width > 0 && height > 0 {
                        pass.set_scissor_rect(x, y, width, height);
                    }
                }
                pass.set_pipeline(&self.sprite_pipeline);
                pass.set_bind_group(0, bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..INDICES.len() as u32, 0, 0..instances.len() as u32);
            }
        }

        // Pass 3: hand the composited scene to the swapchain in the alpha
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
