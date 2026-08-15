use image::RgbaImage;
use crate::ecs::World;

pub struct Compositor;

impl Compositor {
    /// Clears the pixel frame buffer to complete transparency.
    pub fn clear(frame: &mut [u8]) {
        frame.fill(0);
    }

    /// Blends a source sprite onto the destination frame using Porter-Duff SRC_OVER alpha compositing.
    pub fn blend_sprite(
        frame: &mut [u8],
        win_w: u32,
        win_h: u32,
        sprite: &RgbaImage,
        dest_x: i32,
        dest_y: i32,
    ) {
        let (sw, sh) = sprite.dimensions();

        for sy in 0..sh {
            let py = dest_y + sy as i32;
            if py < 0 || py >= win_h as i32 {
                continue;
            }

            for sx in 0..sw {
                let px = dest_x + sx as i32;
                if px < 0 || px >= win_w as i32 {
                    continue;
                }

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

                        let out_r = ((sr * sa_f + dr * da_f * (1.0 - sa_f)) / out_a_f).round() as u8;
                        let out_g = ((sg * sa_f + dg * da_f * (1.0 - sa_f)) / out_a_f).round() as u8;
                        let out_b = ((sb * sa_f + db * da_f * (1.0 - sa_f)) / out_a_f).round() as u8;

                        frame[idx] = out_r;
                        frame[idx + 1] = out_g;
                        frame[idx + 2] = out_b;
                        frame[idx + 3] = (out_a_f * 255.0).round() as u8;
                    }
                }
            }
        }
    }

    /// Renders all active entities in the world onto the frame buffer with z-index sorting.
    pub fn render_world(world: &World, frame: &mut [u8], win_w: u32, win_h: u32) {
        Self::clear(frame);

        let mut sorted_entities: Vec<&crate::ecs::Entity> = world.entities.iter().filter(|e| e.is_active).collect();
        sorted_entities.sort_by_key(|e| e.z_index);

        for entity in sorted_entities {
            if let Some(asset) = world.asset_manager.get(&entity.asset_name) {
                if let Some(state_def) = asset.manifest.states.get(&entity.current_state) {
                    let anim = &state_def.animation;
                    if anim.frames.is_empty() {
                        continue;
                    }

                    let raw_frame_idx = anim.frames[entity.frame_idx % anim.frames.len()];
                    let use_flipped = entity.flip_x ^ anim.flip_x;

                    let frame_list = if use_flipped {
                        &asset.flipped_frames
                    } else {
                        &asset.frames
                    };

                    // A manifest may declare more frames than this asset's sheet
                    // actually holds -- the in-terminal backend generates a
                    // richer frame set than the overlay's procedural fallback.
                    // Wrap rather than skip: skipping renders the entity
                    // invisible for those states, which looks like a crash.
                    if !frame_list.is_empty() {
                        let sprite = &frame_list[raw_frame_idx % frame_list.len()];
                        Self::blend_sprite(frame, win_w, win_h, sprite, entity.x as i32, entity.y as i32);
                    }
                }
            }
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

        // (0, 0) should have sprite color
        assert_eq!(frame[0], 255);
        assert_eq!(frame[1], 100);
        assert_eq!(frame[2], 50);
        assert_eq!(frame[3], 255);

        // (1, 0) should remain unchanged (0)
        assert_eq!(frame[4], 0);
        assert_eq!(frame[5], 0);
        assert_eq!(frame[6], 0);
        assert_eq!(frame[7], 0);
    }

    #[test]
    fn test_blend_sprite_clipping() {
        let mut frame = vec![0u8; 4 * 4 * 4];
        let mut sprite = ImageBuffer::new(2, 2);
        sprite.put_pixel(0, 0, Rgba([255, 255, 255, 255]));

        // Blend partially off-screen top-left (-1, -1) and bottom-right (3, 3)
        Compositor::blend_sprite(&mut frame, 4, 4, &sprite, -1, -1);
        Compositor::blend_sprite(&mut frame, 4, 4, &sprite, 3, 3);
        // No panic and valid write
    }

    #[test]
    fn test_render_world_multi_entity() {
        let mut world = World::new(100.0, 100.0);
        world.spawn("cat", None, Some(10.0), Some(10.0), None).unwrap();
        world.spawn("crab", None, Some(40.0), Some(40.0), None).unwrap();

        let mut frame = vec![0u8; 100 * 100 * 4];
        Compositor::render_world(&world, &mut frame, 100, 100);

        // Verify buffer has non-zero pixels written
        let non_zero_count = frame.iter().filter(|&&b| b > 0).count();
        assert!(non_zero_count > 0);
    }
}

