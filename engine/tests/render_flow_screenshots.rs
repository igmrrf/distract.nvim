use distract_engine::compositor::Compositor;
use distract_engine::ecs::World;
use image::{ImageBuffer, Rgba, RgbaImage};
use std::fs;
use std::path::Path;

/// Creates a simulated dark terminal background (Neovim editor theme #181825 with subtle UI lines).
fn create_terminal_backdrop(w: u32, h: u32) -> RgbaImage {
    let mut img = ImageBuffer::new(w, h);
    let bg = Rgba([24, 24, 37, 255]); // Catppuccin / TokyoNight dark editor bg
    let status_bg = Rgba([30, 30, 46, 255]);
    let border_color = Rgba([49, 50, 68, 255]);
    let text_dim = Rgba([88, 91, 112, 255]);
    let text_highlight = Rgba([137, 180, 250, 255]);

    for y in 0..h {
        for x in 0..w {
            img.put_pixel(x, y, bg);
        }
    }

    // Draw simulated Neovim line numbers and code lines
    for line in 0..(h / 24) {
        let y = line * 24 + 10;
        if y + 8 < h - 40 {
            // Line number
            for px in 8..24 {
                if (line + px) % 3 != 0 {
                    img.put_pixel(px, y + 2, text_dim);
                }
            }
            // Code text line placeholder
            let line_len = ((line * 73) % 180 + 80).min(w - 60);
            for px in 35..line_len {
                if (px / 6) % 4 != 0 {
                    let col = if (line + px / 20) % 3 == 0 {
                        text_highlight
                    } else {
                        text_dim
                    };
                    img.put_pixel(px, y + 2, col);
                    img.put_pixel(px, y + 3, col);
                }
            }
        }
    }

    // Draw status line at bottom
    if h > 30 {
        let status_y = h - 30;
        for x in 0..w {
            img.put_pixel(x, status_y, border_color);
            for y in (status_y + 1)..h {
                img.put_pixel(x, y, status_bg);
            }
        }
    }

    img
}

/// Renders a world to an RGBA image overlaid on top of the simulated editor backdrop.
fn capture_world_screenshot(world: &World, width: u32, height: u32) -> RgbaImage {
    let mut backdrop = create_terminal_backdrop(width, height);
    let mut engine_frame = vec![0u8; (width * height * 4) as usize];
    Compositor::render_world(world, &mut engine_frame, width, height);

    // Alpha blend engine output over backdrop
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            let sa = engine_frame[idx + 3];
            if sa > 0 {
                let sr = engine_frame[idx];
                let sg = engine_frame[idx + 1];
                let sb = engine_frame[idx + 2];

                if sa == 255 {
                    backdrop.put_pixel(x, y, Rgba([sr, sg, sb, 255]));
                } else {
                    let dst = backdrop.get_pixel(x, y);
                    let sa_f = sa as f32 / 255.0;
                    let da_f = dst[3] as f32 / 255.0;
                    let out_a_f = sa_f + da_f * (1.0 - sa_f);

                    let out_r = ((sr as f32 * sa_f + dst[0] as f32 * da_f * (1.0 - sa_f)) / out_a_f)
                        .round() as u8;
                    let out_g = ((sg as f32 * sa_f + dst[1] as f32 * da_f * (1.0 - sa_f)) / out_a_f)
                        .round() as u8;
                    let out_b = ((sb as f32 * sa_f + dst[2] as f32 * da_f * (1.0 - sa_f)) / out_a_f)
                        .round() as u8;

                    backdrop.put_pixel(x, y, Rgba([out_r, out_g, out_b, 255]));
                }
            }
        }
    }

    backdrop
}

#[test]
fn test_render_and_capture_all_feature_screenshots() {
    let out_dir = Path::new("../tests/screenshots");
    let _ = fs::create_dir_all(out_dir);

    let view_w = 480;
    let view_h = 320;

    // ==========================================
    // 1. CAT FEATURE FLOW
    // ==========================================
    {
        let mut world = World::new(view_w as f32, view_h as f32);
        let id = world
            .spawn("cat", None, Some(80.0), Some(200.0), None)
            .expect("Spawn cat");

        // 1.1 Idle
        let img = capture_world_screenshot(&world, view_w, view_h);
        img.save(out_dir.join("01_cat_idle.png"))
            .expect("save 01_cat_idle.png");

        // 1.2 Walk
        world
            .trigger_action(Some(id), None, "walk")
            .unwrap_or_default();
        for _ in 0..15 {
            world.update(0.05);
        }
        let img = capture_world_screenshot(&world, view_w, view_h);
        img.save(out_dir.join("02_cat_walking.png"))
            .expect("save 02_cat_walking.png");

        // 1.3 Jump Apex (impulse + gravity)
        world
            .trigger_action(Some(id), None, "jump")
            .expect("cat jump");
        for _ in 0..12 {
            world.update(0.03);
        }
        let img = capture_world_screenshot(&world, view_w, view_h);
        img.save(out_dir.join("03_cat_jump_apex.png"))
            .expect("save 03_cat_jump_apex.png");

        // 1.4 Jump Landed (clamped at ground_y)
        for _ in 0..30 {
            world.update(0.04);
        }
        let img = capture_world_screenshot(&world, view_w, view_h);
        img.save(out_dir.join("04_cat_jump_landed.png"))
            .expect("save 04_cat_jump_landed.png");

        // 1.5 Yawn
        world
            .trigger_action(Some(id), None, "yawn")
            .expect("cat yawn");
        world.update(0.2);
        let img = capture_world_screenshot(&world, view_w, view_h);
        img.save(out_dir.join("05_cat_yawn.png"))
            .expect("save 05_cat_yawn.png");

        // 1.6 Sleep
        world
            .trigger_action(Some(id), None, "sleep")
            .expect("cat sleep");
        world.update(0.1);
        let img = capture_world_screenshot(&world, view_w, view_h);
        img.save(out_dir.join("06_cat_sleep.png"))
            .expect("save 06_cat_sleep.png");
    }

    // ==========================================
    // 2. CRAB FEATURE FLOW
    // ==========================================
    {
        let mut world = World::new(view_w as f32, view_h as f32);
        let id = world
            .spawn("crab", None, Some(80.0), Some(220.0), None)
            .expect("Spawn crab");

        // 2.1 Walk
        world
            .trigger_action(Some(id), None, "walk")
            .unwrap_or_default();
        for _ in 0..10 {
            world.update(0.05);
        }
        let img = capture_world_screenshot(&world, view_w, view_h);
        img.save(out_dir.join("07_crab_walk.png"))
            .expect("save 07_crab_walk.png");

        // 2.2 Clip Claws (pinchers snapping)
        world
            .trigger_action(Some(id), None, "clip")
            .expect("crab clip");
        world.update(0.15);
        let img = capture_world_screenshot(&world, view_w, view_h);
        img.save(out_dir.join("08_crab_clip_claws.png"))
            .expect("save 08_crab_clip_claws.png");

        // 2.3 Burrow
        world
            .trigger_action(Some(id), None, "burrow")
            .expect("crab burrow");
        world.update(0.3);
        let img = capture_world_screenshot(&world, view_w, view_h);
        img.save(out_dir.join("09_crab_burrow.png"))
            .expect("save 09_crab_burrow.png");

        // 2.4 Boundary Bounce
        let mut bounce_world = World::new(200.0, 200.0);
        let bid = bounce_world
            .spawn("crab", None, Some(180.0), Some(120.0), None)
            .unwrap();
        bounce_world
            .trigger_action(Some(bid), None, "walk")
            .unwrap();
        for _ in 0..15 {
            bounce_world.update(0.05);
        }
        let img = capture_world_screenshot(&bounce_world, 200, 200);
        img.save(out_dir.join("10_crab_bounce.png"))
            .expect("save 10_crab_bounce.png");
    }

    // ==========================================
    // 3. SUN CELESTIAL FLOW
    // ==========================================
    {
        let mut world = World::new(view_w as f32, view_h as f32);
        let id = world
            .spawn("sun", None, Some(200.0), Some(140.0), None)
            .expect("Spawn sun");

        // 3.1 Rising
        world
            .trigger_action(Some(id), None, "rise")
            .expect("sun rise");
        world.update(0.2);
        let img = capture_world_screenshot(&world, view_w, view_h);
        img.save(out_dir.join("11_sun_rising.png"))
            .expect("save 11_sun_rising.png");

        // 3.2 Shining (Sine wave oscillation)
        world.entities[0].set_state("shining".to_string());
        for _ in 0..10 {
            world.update(0.1);
        }
        let img = capture_world_screenshot(&world, view_w, view_h);
        img.save(out_dir.join("12_sun_shining_sine.png"))
            .expect("save 12_sun_shining_sine.png");

        // 3.3 Solar Eclipse (dark corona)
        world
            .trigger_action(Some(id), None, "eclipse")
            .expect("sun eclipse");
        world.update(0.2);
        let img = capture_world_screenshot(&world, view_w, view_h);
        img.save(out_dir.join("13_sun_eclipse.png"))
            .expect("save 13_sun_eclipse.png");

        // 3.4 Solar Flare
        world
            .trigger_action(Some(id), None, "flare")
            .expect("sun flare");
        world.update(0.15);
        let img = capture_world_screenshot(&world, view_w, view_h);
        img.save(out_dir.join("14_sun_flare.png"))
            .expect("save 14_sun_flare.png");

        // 3.5 Setting
        world
            .trigger_action(Some(id), None, "set")
            .expect("sun set");
        world.update(0.2);
        let img = capture_world_screenshot(&world, view_w, view_h);
        img.save(out_dir.join("15_sun_setting.png"))
            .expect("save 15_sun_setting.png");
    }

    // ==========================================
    // 4. MULTI-ENTITY COMPOSITE ECOSYSTEM
    // ==========================================
    {
        let mut world = World::new(view_w as f32, view_h as f32);
        // Background Sun
        let sun_id = world
            .spawn("sun", None, Some(320.0), Some(40.0), None)
            .expect("sun");
        // Foreground Cat jumping
        let cat_id = world
            .spawn("cat", None, Some(140.0), Some(210.0), None)
            .expect("cat");
        // Foreground Crab walking
        let crab_id = world
            .spawn("crab", None, Some(260.0), Some(218.0), None)
            .expect("crab");

        world
            .trigger_action(Some(sun_id), None, "flare")
            .unwrap_or_default();
        world
            .trigger_action(Some(cat_id), None, "jump")
            .unwrap_or_default();
        world
            .trigger_action(Some(crab_id), None, "clip")
            .unwrap_or_default();

        for _ in 0..10 {
            world.update(0.04);
        }

        let composite_img = capture_world_screenshot(&world, view_w, view_h);
        composite_img
            .save(out_dir.join("16_full_ecosystem_composite.png"))
            .expect("save 16_full_ecosystem_composite.png");
    }

    // ==========================================
    // 5. FILMSTRIP SHOWCASE COMPILATION
    // ==========================================
    {
        let panel_w = 160;
        let panel_h = 110;
        let cols = 3;
        let rows = 3;
        let mut showcase = ImageBuffer::new(panel_w * cols, panel_h * rows);

        let showcase_files = [
            ("01_cat_idle.png", 0, 0),
            ("03_cat_jump_apex.png", 1, 0),
            ("06_cat_sleep.png", 2, 0),
            ("07_crab_walk.png", 0, 1),
            ("08_crab_clip_claws.png", 1, 1),
            ("09_crab_burrow.png", 2, 1),
            ("12_sun_shining_sine.png", 0, 2),
            ("13_sun_eclipse.png", 1, 2),
            ("16_full_ecosystem_composite.png", 2, 2),
        ];

        for (filename, c, r) in showcase_files {
            let p = out_dir.join(filename);
            if let Ok(src) = image::open(&p) {
                let resized = src.thumbnail_exact(panel_w, panel_h).to_rgba8();
                for y in 0..panel_h {
                    for x in 0..panel_w {
                        showcase.put_pixel(
                            c * panel_w + x,
                            r * panel_h + y,
                            *resized.get_pixel(x, y),
                        );
                    }
                }
            }
        }

        showcase
            .save(out_dir.join("17_visual_feature_showcase_strip.png"))
            .expect("save 17_visual_feature_showcase_strip.png");
    }

    println!(
        "Successfully rendered and saved all 17 feature verification screenshots to {:?}",
        out_dir
    );
}
