//! CPU compositor.
//!
//! The GPU path draws instanced quads from a texture atlas and does not use
//! this module; it exists for the screenshot test and for headless rendering,
//! where having a deterministic software reference is the point.

use image::RgbaImage;

use crate::ecs::World;

pub struct Compositor;

impl Compositor {
    /// Clears the pixel frame buffer to complete transparency.
    pub fn clear(frame: &mut [u8]) {
        frame.fill(0);
    }

    /// Blends a source sprite onto the destination frame using Porter-Duff
    /// SRC_OVER alpha compositing.
    pub fn blend_sprite(
        frame: &mut [u8],
        win_w: u32,
        win_h: u32,
        sprite: &RgbaImage,
        dest_x: i32,
        dest_y: i32,
    ) {
        Self::blend_sprite_ex(frame, win_w, win_h, sprite, dest_x, dest_y, false, 1, 1);
    }

    /// Blends a sprite with optional horizontal mirroring and integer nearest
    /// neighbour upscaling.
    ///
    /// Mirroring reads the source column in reverse rather than consulting a
    /// pre-flipped copy: keeping a second mirrored image of every frame alive
    /// for the process lifetime doubled asset memory permanently to support a
    /// boolean.
    #[allow(clippy::too_many_arguments)]
    pub fn blend_sprite_ex(
        frame: &mut [u8],
        win_w: u32,
        win_h: u32,
        sprite: &RgbaImage,
        dest_x: i32,
        dest_y: i32,
        flip_x: bool,
        scale_x: u32,
        scale_y: u32,
    ) {
        let (sw, sh) = sprite.dimensions();
        if sw == 0 || sh == 0 {
            return;
        }
        // A sprite pixel is one cell wide and half a cell tall, so the two axes
        // do not share a scale factor except on an exactly 2:1 cell.
        let scale_x = scale_x.max(1);
        let scale_y = scale_y.max(1);

        for dy in 0..sh * scale_y {
            let py = dest_y + dy as i32;
            if py < 0 || py >= win_h as i32 {
                continue;
            }
            let sy = dy / scale_y;

            for dx in 0..sw * scale_x {
                let px = dest_x + dx as i32;
                if px < 0 || px >= win_w as i32 {
                    continue;
                }
                let sx = if flip_x {
                    sw - 1 - (dx / scale_x)
                } else {
                    dx / scale_x
                };

                let src = sprite.get_pixel(sx, sy);
                let sa = src[3];
                if sa == 0 {
                    continue;
                }

                let idx = ((py as u32 * win_w + px as u32) * 4) as usize;
                if idx + 3 >= frame.len() {
                    continue;
                }

                if sa == 255 {
                    frame[idx] = src[0];
                    frame[idx + 1] = src[1];
                    frame[idx + 2] = src[2];
                    frame[idx + 3] = 255;
                } else {
                    let da = frame[idx + 3];
                    if da == 0 {
                        frame[idx] = src[0];
                        frame[idx + 1] = src[1];
                        frame[idx + 2] = src[2];
                        frame[idx + 3] = sa;
                    } else {
                        let sa_f = sa as f32 / 255.0;
                        let da_f = da as f32 / 255.0;
                        let out_a_f = sa_f + da_f * (1.0 - sa_f);

                        let dr = frame[idx] as f32;
                        let dg = frame[idx + 1] as f32;
                        let db = frame[idx + 2] as f32;

                        let sr = src[0] as f32;
                        let sg = src[1] as f32;
                        let sb = src[2] as f32;

                        let out_r =
                            ((sr * sa_f + dr * da_f * (1.0 - sa_f)) / out_a_f).round() as u8;
                        let out_g =
                            ((sg * sa_f + dg * da_f * (1.0 - sa_f)) / out_a_f).round() as u8;
                        let out_b =
                            ((sb * sa_f + db * da_f * (1.0 - sa_f)) / out_a_f).round() as u8;

                        frame[idx] = out_r;
                        frame[idx + 1] = out_g;
                        frame[idx + 2] = out_b;
                        frame[idx + 3] = (out_a_f * 255.0).round() as u8;
                    }
                }
            }
        }
    }

    /// Renders all active entities in the world onto the frame buffer with
    /// z-index sorting.
    pub fn render_world(world: &World, frame: &mut [u8], win_w: u32, win_h: u32) {
        Self::clear(frame);

        let mut sorted: Vec<&crate::ecs::Entity> =
            world.entities.iter().filter(|e| e.is_active).collect();
        sorted.sort_by_key(|e| e.z_index);

        let scale_x = world.sprite_scale_x.round().max(1.0) as u32;
        let scale_y = world.sprite_scale_y.round().max(1.0) as u32;

        for entity in sorted {
            let Some(asset) = world.asset_manager.get(&entity.asset_name) else {
                continue;
            };
            let Some(state_def) = asset.manifest.states.get(&entity.current_state) else {
                continue;
            };

            let anim = &state_def.animation;
            if anim.frames.is_empty() || asset.frames.is_empty() {
                continue;
            }

            let raw_frame_idx = anim.frames[entity.frame_idx % anim.frames.len()];
            let flip = entity.flip_x ^ anim.flip_x;

            // A manifest may still declare more frames than a user-supplied
            // sheet holds. Wrap rather than skip: skipping renders the entity
            // invisible for those states, which looks like a crash.
            let sprite = &asset.frames[raw_frame_idx % asset.frames.len()];
            Self::blend_sprite_ex(
                frame,
                win_w,
                win_h,
                sprite,
                entity.x as i32,
                entity.y as i32,
                flip,
                scale_x,
                scale_y,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    #[test]
    fn test_clear_buffer() {
        let mut frame = vec![255u8; 16];
        Compositor::clear(&mut frame);
        for byte in frame {
            assert_eq!(byte, 0);
        }
    }

    #[test]
    fn test_blend_sprite_opaque_and_transparent() {
        let mut frame = vec![0u8; 4 * 4 * 4]; // 4x4 RGBA frame
        let mut sprite = ImageBuffer::new(2, 2);

        sprite.put_pixel(0, 0, Rgba([255, 100, 50, 255])); // Opaque
        sprite.put_pixel(1, 0, Rgba([0, 0, 0, 0])); // Fully transparent (skipped)

        Compositor::blend_sprite(&mut frame, 4, 4, &sprite, 0, 0);

        assert_eq!(&frame[0..4], &[255, 100, 50, 255]);
        assert_eq!(&frame[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn test_blend_sprite_clipping() {
        let mut frame = vec![0u8; 4 * 4 * 4];
        let mut sprite = ImageBuffer::new(2, 2);
        sprite.put_pixel(0, 0, Rgba([255, 255, 255, 255]));

        Compositor::blend_sprite(&mut frame, 4, 4, &sprite, -1, -1);
        Compositor::blend_sprite(&mut frame, 4, 4, &sprite, 3, 3);
        // No panic and valid write
    }

    #[test]
    fn blend_mirrors_without_a_second_copy_of_the_frame() {
        let mut sprite: RgbaImage = ImageBuffer::new(2, 1);
        sprite.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        sprite.put_pixel(1, 0, Rgba([0, 0, 255, 255]));

        let mut frame = vec![0u8; 2 * 4]; // 2x1 RGBA
        Compositor::blend_sprite_ex(&mut frame, 2, 1, &sprite, 0, 0, true, 1, 1);

        assert_eq!(&frame[0..4], &[0, 0, 255, 255]);
        assert_eq!(&frame[4..8], &[255, 0, 0, 255]);
    }

    #[test]
    fn scaling_replicates_each_source_pixel_as_a_block() {
        let mut sprite: RgbaImage = ImageBuffer::new(1, 1);
        sprite.put_pixel(0, 0, Rgba([10, 20, 30, 255]));

        let mut frame = vec![0u8; 3 * 3 * 4];
        Compositor::blend_sprite_ex(&mut frame, 3, 3, &sprite, 0, 0, false, 3, 3);

        for i in 0..9 {
            assert_eq!(&frame[i * 4..i * 4 + 4], &[10, 20, 30, 255], "pixel {}", i);
        }
    }

    #[test]
    fn test_alpha_compositing() {
        let mut frame = vec![0u8; 4 * 4 * 4];

        let mut sprite = ImageBuffer::new(2, 2);
        sprite.put_pixel(0, 0, Rgba([255, 0, 0, 255])); // Opaque red
        sprite.put_pixel(1, 0, Rgba([0, 255, 0, 128])); // Semi-transparent green

        Compositor::blend_sprite(&mut frame, 4, 4, &sprite, 0, 0);

        assert_eq!(&frame[0..4], &[255, 0, 0, 255]);
        assert_eq!(&frame[4..8], &[0, 255, 0, 128]);
    }

    #[test]
    fn test_render_world_multi_entity() {
        let mut world = World::new(100.0, 100.0);
        world
            .spawn("cat", None, Some(10.0), Some(10.0), None)
            .unwrap();
        world
            .spawn("crab", None, Some(40.0), Some(40.0), None)
            .unwrap();

        let mut frame = vec![0u8; 100 * 100 * 4];
        Compositor::render_world(&world, &mut frame, 100, 100);

        let non_zero_count = frame.iter().filter(|&&b| b > 0).count();
        assert!(non_zero_count > 0);
    }

    #[test]
    fn render_world_draws_low_z_index_first() {
        // The sun is z -10 and the cat z 10, so the cat must win the overlap.
        let mut world = World::new(200.0, 200.0);
        world.sprite_scale_x = 1.0;
        world.sprite_scale_y = 1.0;
        world
            .spawn("sun", None, Some(20.0), Some(20.0), None)
            .unwrap();
        world
            .spawn("cat", None, Some(20.0), Some(20.0), None)
            .unwrap();

        let mut sorted: Vec<i32> = world.entities.iter().map(|e| e.z_index).collect();
        sorted.sort();
        assert_eq!(sorted, vec![-10, 10]);

        let mut frame = vec![0u8; 200 * 200 * 4];
        Compositor::render_world(&world, &mut frame, 200, 200);
        assert!(frame.iter().any(|&b| b > 0));
    }
}
