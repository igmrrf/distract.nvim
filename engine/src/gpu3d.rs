//! The voxel mesh pass.
//!
//! Separate from `gpu.rs` for two reasons. A render pass's depth attachment
//! applies to every pipeline in it, so depth-tested meshes and painter-ordered
//! sprites cannot share one pass. And `gpu.rs` is at its file cap, so the mesh
//! pipeline and its buffers live here; `mesh_draw.rs` holds the pure half that
//! decides what gets drawn.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::camera::Camera;
use crate::ecs::World;
use crate::mesh_draw::{MeshDraw, MeshFrame, MeshInstance, build_mesh_draws, world_needs_meshes};
use crate::meshbook::MeshBook;
use crate::render::RenderSettings;
use crate::voxel::{MeshVertex, VoxelOptions};

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct Uniforms3d {
    view_proj: [f32; 16],
    light: [f32; 4],
}

const MIN_INSTANCE_CAPACITY: usize = 64;

/// The wgpu side: one pipeline, the shared mesh buffers, and the instance buffer.
pub struct MeshPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniforms: wgpu::Buffer,

    vertex_buffer: Option<wgpu::Buffer>,
    index_buffer: Option<wgpu::Buffer>,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    /// The meshes currently uploaded, kept so per-frame draw building can look up
    /// a frame's range without rebuilding anything.
    book: Option<MeshBook>,
    /// The asset generation and voxel options the book was built from, so
    /// geometry is rebuilt only when the asset set or the settings change.
    built_from: Option<(u64, VoxelOptions)>,
}

impl MeshPipeline {
    pub fn new(device: &wgpu::Device, colour_format: wgpu::TextureFormat) -> Result<Self, String> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Distract Mesh Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader3d.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Mesh Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh Uniforms"),
            contents: bytemuck::bytes_of(&Uniforms3d {
                view_proj: Camera::default().to_uniform(),
                light: [0.0, 1.0, 0.0, 1.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Mesh Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Mesh Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Mesh Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_mesh",
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<MeshVertex>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x3, 1 => Snorm8x4, 2 => Unorm8x4
                        ],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<MeshInstance>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![3 => Float32x4, 4 => Float32x4],
                    },
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_mesh",
                targets: &[Some(wgpu::ColorTargetState {
                    format: colour_format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                // The projection flips y, which reverses every face's winding, so
                // culling would have to be authored against the flip. Per-face
                // normals already shade correctly from either side and the meshes
                // are small enough that the saved half costs nothing.
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Mesh Instance Buffer"),
            size: (MIN_INSTANCE_CAPACITY * std::mem::size_of::<MeshInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            pipeline,
            bind_group,
            uniforms,
            vertex_buffer: None,
            index_buffer: None,
            instance_buffer,
            instance_capacity: MIN_INSTANCE_CAPACITY,
            book: None,
            built_from: None,
        })
    }

    /// Rebuilds and uploads geometry when the asset set or the voxel settings
    /// changed, and does nothing at all in a session that draws no meshes.
    pub fn sync(&mut self, device: &wgpu::Device, world: &World) -> Result<(), String> {
        if !world_needs_meshes(world) {
            return Ok(());
        }
        let wanted = (
            world.asset_manager.generation(),
            world.render.voxel_options(),
        );
        if self.built_from == Some(wanted) {
            return Ok(());
        }

        let book = MeshBook::build(&world.asset_manager, wanted.1);
        if book.skipped_frames > 0 {
            log::warn!(
                "voxel mesh budget reached: {} frames have no model",
                book.skipped_frames
            );
        }
        self.upload(device, &book);
        self.book = Some(book);
        self.built_from = Some(wanted);
        Ok(())
    }

    fn upload(&mut self, device: &wgpu::Device, book: &MeshBook) {
        if book.is_empty() {
            self.vertex_buffer = None;
            self.index_buffer = None;
            return;
        }

        self.vertex_buffer = Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Mesh Vertex Buffer"),
                contents: bytemuck::cast_slice(&book.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
        );
        self.index_buffer = Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Mesh Index Buffer"),
                contents: bytemuck::cast_slice(&book.indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
        );
    }

    /// Builds this frame's draw list and uploads everything it needs.
    ///
    /// Returns the draws and, per draw, the slice of the instance buffer it
    /// covers. An empty result means the mesh pass has nothing to record.
    pub fn prepare(
        &mut self,
        gpu: (&wgpu::Device, &wgpu::Queue),
        world: &World,
        camera: &Camera,
    ) -> MeshFrame {
        let (device, queue) = gpu;
        let Some(book) = &self.book else {
            return MeshFrame::default();
        };
        let draws = build_mesh_draws(world, book, camera);
        if draws.is_empty() {
            return MeshFrame::default();
        }

        self.write_camera(queue, camera, &world.render);
        let slices = self.write_instances(device, queue, &draws);
        MeshFrame { draws, slices }
    }

    fn write_camera(&self, queue: &wgpu::Queue, camera: &Camera, settings: &RenderSettings) {
        let [x, y, z] = settings.light_direction();
        queue.write_buffer(
            &self.uniforms,
            0,
            bytemuck::bytes_of(&Uniforms3d {
                view_proj: camera.to_uniform(),
                light: [x, y, z, settings.light.ambient],
            }),
        );
    }

    /// Uploads every group's instances into one buffer and records where each
    /// group's slice starts.
    fn write_instances(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        draws: &[MeshDraw],
    ) -> Vec<std::ops::Range<u32>> {
        let mut flat: Vec<MeshInstance> = Vec::new();
        let mut ranges = Vec::with_capacity(draws.len());
        for draw in draws {
            let first = flat.len() as u32;
            flat.extend_from_slice(&draw.instances);
            ranges.push(first..flat.len() as u32);
        }
        if flat.is_empty() {
            return ranges;
        }

        if flat.len() > self.instance_capacity {
            self.instance_capacity = flat.len().next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Mesh Instance Buffer"),
                size: (self.instance_capacity * std::mem::size_of::<MeshInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&flat));
        ranges
    }

    /// Records the mesh pass into its own render pass.
    ///
    /// Its own, because the depth attachment it needs applies to every pipeline
    /// in a pass and the sprite pass has no depth.
    pub fn record(&self, target: PassTarget, frame: &MeshFrame) {
        let (Some(vertices), Some(indices)) = (&self.vertex_buffer, &self.index_buffer) else {
            return;
        };
        if frame.is_empty() {
            return;
        }

        let mut pass = target
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Mesh Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.colour,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: true,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: true,
                    }),
                    stencil_ops: None,
                }),
            });

        if let Some([x, y, width, height]) = target.scissor {
            if width > 0 && height > 0 {
                pass.set_scissor_rect(x, y, width, height);
            }
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);

        for (draw, slice) in frame.draws.iter().zip(&frame.slices) {
            let first = draw.first_index;
            pass.draw_indexed(first..first + draw.index_count, 0, slice.clone());
        }
    }
}

/// Where one mesh pass draws.
pub struct PassTarget<'target> {
    pub encoder: &'target mut wgpu::CommandEncoder,
    pub colour: &'target wgpu::TextureView,
    pub depth: &'target wgpu::TextureView,
    pub scissor: Option<[u32; 4]>,
}

/// The depth target the mesh pass tests against.
pub fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Depth Texture"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}
