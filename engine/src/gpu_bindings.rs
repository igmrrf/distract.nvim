//! How a texture reaches the sprite passes.
//!
//! Both the sprite atlas and the composited scene bind through one layout -- a
//! texture, the nearest-neighbour sampler, one uniform buffer -- so the layout is
//! built once in `gpu_setup` and each texture is bound through this. Split out
//! because it is the one part of the renderer's construction that also runs
//! later: the atlas is re-uploaded whenever the asset set changes, and the scene
//! target is rebuilt on every resize.

use crate::atlas::Atlas;
use crate::gpu_setup::SCENE_FORMAT;

/// The three things every bind group in the sprite passes binds: a texture, the
/// nearest-neighbour sampler, and one uniform buffer.
///
/// Both the atlas and the composited scene bind through the same layout, which is
/// why the layout is built once and each texture is bound through this.
pub struct TextureBinding<'a> {
    pub layout: &'a wgpu::BindGroupLayout,
    pub sampler: &'a wgpu::Sampler,
    pub uniforms: &'a wgpu::Buffer,
}

impl TextureBinding<'_> {
    /// Builds the offscreen scene texture and the bind group the resolve pass
    /// reads it through.
    pub fn scene_target(
        &self,
        device: &wgpu::Device,
        viewport: (u32, u32),
    ) -> (wgpu::Texture, wgpu::BindGroup) {
        create_scene_target(device, self, viewport)
    }

    /// Uploads a sprite atlas and returns the bind group the sprite pass draws it
    /// through.
    ///
    /// The texture is declared sRGB because sprite art is authored in sRGB: the
    /// sampler has to decode it to linear before blending and the target
    /// re-encodes on write. Declaring it `Unorm` made every sprite render washed
    /// out.
    pub fn atlas(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        atlas: &Atlas,
    ) -> wgpu::BindGroup {
        let (width, height) = (atlas.width().max(1), atlas.height().max(1));
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Sprite Atlas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            atlas.image.as_raw(),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.bind(device, &texture, "Atlas Bind Group")
    }

    fn bind(&self, device: &wgpu::Device, texture: &wgpu::Texture, label: &str) -> wgpu::BindGroup {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.uniforms.as_entire_binding(),
                },
            ],
            label: Some(label),
        })
    }
}

fn create_scene_target(
    device: &wgpu::Device,
    binding: &TextureBinding,
    viewport: (u32, u32),
) -> (wgpu::Texture, wgpu::BindGroup) {
    let (width, height) = viewport;
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
    let bind_group = binding.bind(device, &texture, "Scene Bind Group");
    (texture, bind_group)
}
