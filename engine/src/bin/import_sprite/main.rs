mod args;
mod background;
mod decode;
mod manifest_scaffold;
mod pack;
mod rgba_sidecar;

use std::process;

use image::RgbaImage;

use crate::args::{Args, parse_args};

const FEATHER_BAND: f32 = 0.04;
const FALLBACK_FPS: f32 = 12.0;
const MAX_FRAME_DIMENSION: u32 = 4096;

fn decode_spritesheet_source(args: &Args) -> Result<Vec<decode::DecodedFrame>, String> {
    let path = args
        .spritesheet
        .as_ref()
        .ok_or("--spritesheet is required here")?;
    let (cell_width, cell_height) = args.cell.ok_or("--spritesheet needs --cell")?;
    let row_counts = args
        .row_counts
        .as_ref()
        .ok_or("--spritesheet needs --row-counts")?;

    decode::decode_spritesheet_grid(
        path,
        &decode::GridSpec {
            cell_width,
            cell_height,
            row_counts,
        },
    )
}

fn decode_source(args: &Args) -> Result<Vec<decode::DecodedFrame>, String> {
    if let Some(path) = &args.gif {
        return decode::decode_gif(path);
    }
    if let Some(dir) = &args.frames_dir {
        return decode::decode_png_folder(dir);
    }
    decode_spritesheet_source(args)
}

fn average_fps(frames: &[decode::DecodedFrame]) -> f32 {
    let delays: Vec<u32> = frames.iter().filter_map(|frame| frame.delay_ms).collect();
    if delays.is_empty() {
        return FALLBACK_FPS;
    }
    let average_ms = delays.iter().sum::<u32>() as f32 / delays.len() as f32;
    if average_ms <= 0.0 {
        return FALLBACK_FPS;
    }
    1000.0 / average_ms
}

fn validate_frame_size(frame_width: u32, frame_height: u32) -> Result<(), String> {
    if frame_width == 0 || frame_height == 0 {
        return Err("decoded frames have a zero dimension".to_string());
    }
    if frame_width > MAX_FRAME_DIMENSION || frame_height > MAX_FRAME_DIMENSION {
        return Err(format!(
            "padded frame {frame_width}x{frame_height} exceeds the {MAX_FRAME_DIMENSION}px budget"
        ));
    }
    Ok(())
}

fn cut_out_frame(index: usize, frame: &RgbaImage, bg_tolerance: f32) -> RgbaImage {
    if background::is_already_cutout(frame) {
        eprintln!("import_sprite: frame {index} is already alpha-cutout, keeping its own edges");
        return frame.clone();
    }
    background::remove_background(frame, bg_tolerance, FEATHER_BAND)
}

fn warn_blank_frames(frames: &[RgbaImage]) {
    for (index, frame) in frames.iter().enumerate() {
        if frame.pixels().all(|pixel| pixel[3] == 0) {
            eprintln!(
                "import_sprite: frame {index} is fully transparent after background removal \
                 -- lower --bg-tolerance"
            );
        }
    }
}

fn build_manifest_text(args: &Args, layout: &SheetLayout, fps: f32) -> Result<String, String> {
    let states = match &args.states {
        Some(raw) => manifest_scaffold::parse_states_arg(raw, layout.frame_count)?,
        None => manifest_scaffold::default_state(layout.frame_count),
    };

    let params = manifest_scaffold::ManifestParams {
        name: &args.name,
        sheet_path: &layout.sheet_path_text,
        native_path: &layout.native_path_text,
        frame_width: layout.frame_width,
        frame_height: layout.frame_height,
        columns: layout.columns,
        rows: layout.rows,
        states: &states,
        fps,
    };
    Ok(manifest_scaffold::render_manifest(&params))
}

struct SheetLayout {
    frame_width: u32,
    frame_height: u32,
    frame_count: usize,
    columns: u32,
    rows: u32,
    sheet_path_text: String,
    native_path_text: String,
}

fn run(args: Args) -> Result<(), String> {
    let decoded = decode_source(&args)?;

    let cutout: Vec<RgbaImage> = decoded
        .iter()
        .enumerate()
        .map(|(index, frame)| cut_out_frame(index, &frame.image, args.bg_tolerance))
        .collect();

    let (padded, frame_width, frame_height) = pack::pad_to_common_canvas(&cutout);
    validate_frame_size(frame_width, frame_height)?;
    warn_blank_frames(&padded);

    let (columns, rows) = pack::grid_dimensions(padded.len());
    let sheet = pack::pack_spritesheet(&padded, columns, frame_width, frame_height);

    std::fs::create_dir_all(&args.out)
        .map_err(|error| format!("{}: {error}", args.out.display()))?;

    let sheet_path = args.out.join(format!("{}_sheet.png", args.name));
    sheet
        .save(&sheet_path)
        .map_err(|error| format!("{}: {error}", sheet_path.display()))?;

    let native_path = args.out.join(format!("{}_frames.rgba", args.name));
    rgba_sidecar::write_rgba_sidecar(&native_path, frame_width, frame_height, &padded)?;

    let layout = SheetLayout {
        frame_width,
        frame_height,
        frame_count: padded.len(),
        columns,
        rows,
        sheet_path_text: sheet_path.display().to_string(),
        native_path_text: native_path.display().to_string(),
    };
    let manifest_text = build_manifest_text(&args, &layout, average_fps(&decoded))?;

    if let Some(parent) = args.manifest_out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    std::fs::write(&args.manifest_out, manifest_text)
        .map_err(|error| format!("{}: {error}", args.manifest_out.display()))?;

    eprintln!(
        "import_sprite: wrote {} frames to {} and {}, manifest at {}",
        padded.len(),
        sheet_path.display(),
        native_path.display(),
        args.manifest_out.display()
    );
    Ok(())
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("import_sprite: {message}");
            process::exit(1);
        }
    };

    if let Err(message) = run(args) {
        eprintln!("import_sprite: {message}");
        process::exit(1);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::args::DEFAULT_BG_TOLERANCE;
    use image::codecs::gif::GifEncoder;
    use image::{Delay, Rgba};

    fn write_fixture_gif(path: &std::path::Path) {
        let mut bytes = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut bytes);
            for color in [[200u8, 50, 50, 255], [50, 200, 50, 255], [50, 50, 200, 255]] {
                let mut image = RgbaImage::new(4, 4);
                for pixel in image.pixels_mut() {
                    *pixel = Rgba(color);
                }
                let frame =
                    image::Frame::from_parts(image, 0, 0, Delay::from_numer_denom_ms(100, 1));
                encoder.encode_frame(frame).expect("encode frame");
            }
        }
        std::fs::write(path, bytes).expect("write fixture gif");
    }

    #[test]
    fn a_full_run_produces_a_sheet_a_sidecar_and_a_manifest() {
        let gif_path = std::env::temp_dir().join("distract_import_sprite_cli_test.gif");
        write_fixture_gif(&gif_path);
        let out_dir = std::env::temp_dir().join("distract_import_sprite_cli_test_out");
        let manifest_path = std::env::temp_dir().join("distract_import_sprite_cli_test.lua");
        std::fs::remove_dir_all(&out_dir).ok();
        std::fs::remove_file(&manifest_path).ok();

        let args = Args {
            gif: Some(gif_path.clone()),
            frames_dir: None,
            spritesheet: None,
            cell: None,
            row_counts: None,
            name: "cli_test_asset".to_string(),
            states: None,
            out: out_dir.clone(),
            manifest_out: manifest_path.clone(),
            bg_tolerance: DEFAULT_BG_TOLERANCE,
        };

        run(args).expect("run");

        let sheet_path = out_dir.join("cli_test_asset_sheet.png");
        let native_path = out_dir.join("cli_test_asset_frames.rgba");
        assert!(sheet_path.exists());
        assert!(native_path.exists());
        assert!(manifest_path.exists());

        let (frame_width, frame_height, frames) =
            rgba_sidecar::read_rgba_sidecar(&native_path).expect("read sidecar");
        assert_eq!(frames.len(), 3);
        assert_eq!((frame_width, frame_height), (4, 4));

        let manifest_text = std::fs::read_to_string(&manifest_path).expect("read manifest");
        assert!(manifest_text.contains("name = \"cli_test_asset\""));
        assert!(manifest_text.contains("default = {"));

        std::fs::remove_file(&gif_path).ok();
        std::fs::remove_dir_all(&out_dir).ok();
        std::fs::remove_file(&manifest_path).ok();
    }

    /// A 2x1 grid of 4x4 cells, transparent apart from one opaque body pixel and
    /// one deliberately semi-transparent edge pixel per cell.
    fn write_cutout_atlas(path: &std::path::Path) {
        let mut atlas = RgbaImage::new(8, 4);
        for cell in 0..2u32 {
            let origin_x = cell * 4;
            atlas.put_pixel(origin_x + 1, 1, Rgba([200, 100, 50, 255]));
            atlas.put_pixel(origin_x + 2, 1, Rgba([200, 100, 50, 128]));
        }
        atlas.save(path).expect("write cutout atlas");
    }

    #[test]
    fn an_already_cutout_source_keeps_its_own_edge_alpha() {
        let atlas_path = std::env::temp_dir().join("distract_import_sprite_cutout_test.png");
        write_cutout_atlas(&atlas_path);
        let out_dir = std::env::temp_dir().join("distract_import_sprite_cutout_test_out");
        let manifest_path = std::env::temp_dir().join("distract_import_sprite_cutout_test.lua");
        std::fs::remove_dir_all(&out_dir).ok();
        std::fs::remove_file(&manifest_path).ok();

        let args = Args {
            gif: None,
            frames_dir: None,
            spritesheet: Some(atlas_path.clone()),
            cell: Some((4, 4)),
            row_counts: Some(vec![2]),
            name: "cutout_test_asset".to_string(),
            states: None,
            out: out_dir.clone(),
            manifest_out: manifest_path.clone(),
            bg_tolerance: DEFAULT_BG_TOLERANCE,
        };

        run(args).expect("run");

        let native_path = out_dir.join("cutout_test_asset_frames.rgba");
        let (frame_width, frame_height, frames) =
            rgba_sidecar::read_rgba_sidecar(&native_path).expect("read sidecar");

        assert_eq!(frames.len(), 2);
        assert_eq!((frame_width, frame_height), (4, 4));
        for frame in &frames {
            assert_eq!(
                frame.get_pixel(2, 1),
                &Rgba([200, 100, 50, 128]),
                "an antialiased edge pixel must survive import untouched"
            );
            assert_eq!(frame.get_pixel(1, 1), &Rgba([200, 100, 50, 255]));
            assert_eq!(frame.get_pixel(0, 0)[3], 0);
        }

        std::fs::remove_file(&atlas_path).ok();
        std::fs::remove_dir_all(&out_dir).ok();
        std::fs::remove_file(&manifest_path).ok();
    }
}
