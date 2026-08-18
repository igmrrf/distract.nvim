mod background;
mod decode;
mod manifest_scaffold;
mod pack;
mod rgba_sidecar;

use std::path::PathBuf;
use std::process;

use image::RgbaImage;

const DEFAULT_BG_TOLERANCE: f32 = 0.12;
const FEATHER_BAND: f32 = 0.04;
const FALLBACK_FPS: f32 = 12.0;
const MAX_FRAME_DIMENSION: u32 = 4096;

struct Args {
    gif: Option<PathBuf>,
    frames_dir: Option<PathBuf>,
    spritesheet: Option<PathBuf>,
    cell: Option<(u32, u32)>,
    row_counts: Option<Vec<usize>>,
    name: String,
    states: Option<String>,
    out: PathBuf,
    manifest_out: PathBuf,
    bg_tolerance: f32,
}

fn parse_cell(raw: &str) -> Result<(u32, u32), String> {
    let (width_text, height_text) = raw
        .split_once('x')
        .ok_or_else(|| format!("--cell '{raw}' is not WxH"))?;
    let width = width_text
        .trim()
        .parse()
        .map_err(|_| format!("--cell width '{width_text}' is not a number"))?;
    let height = height_text
        .trim()
        .parse()
        .map_err(|_| format!("--cell height '{height_text}' is not a number"))?;
    Ok((width, height))
}

fn parse_row_counts(raw: &str) -> Result<Vec<usize>, String> {
    raw.split(',')
        .map(|entry| {
            entry
                .trim()
                .parse()
                .map_err(|_| format!("--row-counts entry '{entry}' is not a number"))
        })
        .collect()
}

fn validate_source_choice(args: &Args) -> Result<(), String> {
    let source_count = [
        args.gif.is_some(),
        args.frames_dir.is_some(),
        args.spritesheet.is_some(),
    ]
    .iter()
    .filter(|is_set| **is_set)
    .count();
    if source_count != 1 {
        return Err("exactly one of --gif, --frames or --spritesheet is required".to_string());
    }

    let has_grid = args.cell.is_some() && args.row_counts.is_some();
    if args.spritesheet.is_some() && !has_grid {
        return Err("--spritesheet needs both --cell and --row-counts".to_string());
    }
    if args.spritesheet.is_none() && (args.cell.is_some() || args.row_counts.is_some()) {
        return Err("--cell and --row-counts only apply to --spritesheet".to_string());
    }
    Ok(())
}

fn parse_args_from(mut raw_args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut gif = None;
    let mut frames_dir = None;
    let mut spritesheet = None;
    let mut cell = None;
    let mut row_counts = None;
    let mut name: Option<String> = None;
    let mut states = None;
    let mut out = None;
    let mut manifest_out = None;
    let mut bg_tolerance = DEFAULT_BG_TOLERANCE;

    while let Some(flag) = raw_args.next() {
        let mut take_value = || {
            raw_args
                .next()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--gif" => gif = Some(PathBuf::from(take_value()?)),
            "--frames" => frames_dir = Some(PathBuf::from(take_value()?)),
            "--spritesheet" => spritesheet = Some(PathBuf::from(take_value()?)),
            "--cell" => cell = Some(parse_cell(&take_value()?)?),
            "--row-counts" => row_counts = Some(parse_row_counts(&take_value()?)?),
            "--name" => name = Some(take_value()?),
            "--states" => states = Some(take_value()?),
            "--out" => out = Some(PathBuf::from(take_value()?)),
            "--manifest-out" => manifest_out = Some(PathBuf::from(take_value()?)),
            "--bg-tolerance" => {
                bg_tolerance = take_value()?
                    .parse()
                    .map_err(|_| "--bg-tolerance needs a number".to_string())?;
            }
            other => return Err(format!("unknown flag '{other}'")),
        }
    }

    let name = name.ok_or("--name is required")?;

    let out = out.unwrap_or_else(|| PathBuf::from(format!("assets/{name}")));
    let manifest_out =
        manifest_out.unwrap_or_else(|| PathBuf::from(format!("lua/distract/manifests/{name}.lua")));

    let args = Args {
        gif,
        frames_dir,
        spritesheet,
        cell,
        row_counts,
        name,
        states,
        out,
        manifest_out,
        bg_tolerance,
    };
    validate_source_choice(&args)?;
    Ok(args)
}

fn parse_args() -> Result<Args, String> {
    parse_args_from(std::env::args().skip(1))
}

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
        .map(|frame| background::remove_background(&frame.image, args.bg_tolerance, FEATHER_BAND))
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
mod tests {
    use super::*;

    fn args(flags: &[&str]) -> Vec<String> {
        flags.iter().map(|flag| flag.to_string()).collect()
    }

    #[test]
    fn requires_exactly_one_of_gif_or_frames() {
        let neither = parse_args_from(args(&["--name", "x"]).into_iter());
        assert!(neither.is_err());

        let both = parse_args_from(
            args(&["--gif", "a.gif", "--frames", "dir", "--name", "x"]).into_iter(),
        );
        assert!(both.is_err());
    }

    #[test]
    fn a_spritesheet_source_counts_as_the_one_source_and_needs_its_grid() {
        let without_grid =
            parse_args_from(args(&["--spritesheet", "atlas.webp", "--name", "x"]).into_iter());
        assert!(without_grid.is_err());

        let with_grid = parse_args_from(
            args(&[
                "--spritesheet",
                "atlas.webp",
                "--cell",
                "192x208",
                "--row-counts",
                "7,8,8",
                "--name",
                "dog",
            ])
            .into_iter(),
        )
        .expect("parse");
        assert_eq!(with_grid.cell, Some((192, 208)));
        assert_eq!(with_grid.row_counts, Some(vec![7, 8, 8]));

        let alongside_gif = parse_args_from(
            args(&[
                "--gif",
                "a.gif",
                "--spritesheet",
                "atlas.webp",
                "--cell",
                "1x1",
                "--row-counts",
                "1",
                "--name",
                "x",
            ])
            .into_iter(),
        );
        assert!(alongside_gif.is_err());
    }

    #[test]
    fn a_grid_flag_without_a_spritesheet_is_rejected() {
        let result =
            parse_args_from(args(&["--gif", "a.gif", "--cell", "8x8", "--name", "x"]).into_iter());
        assert!(result.is_err());
    }

    #[test]
    fn malformed_grid_flags_are_rejected() {
        assert!(parse_cell("192-208").is_err());
        assert!(parse_cell("192xwide").is_err());
        assert!(parse_row_counts("7,eight").is_err());
        assert_eq!(parse_cell("192x208"), Ok((192, 208)));
        assert_eq!(parse_row_counts(" 7 , 8 "), Ok(vec![7, 8]));
    }

    #[test]
    fn defaults_out_and_manifest_out_from_name() {
        let parsed =
            parse_args_from(args(&["--gif", "a.gif", "--name", "cat_walking"]).into_iter())
                .expect("parse");
        assert_eq!(parsed.out, PathBuf::from("assets/cat_walking"));
        assert_eq!(
            parsed.manifest_out,
            PathBuf::from("lua/distract/manifests/cat_walking.lua")
        );
        assert_eq!(parsed.bg_tolerance, DEFAULT_BG_TOLERANCE);
    }

    #[test]
    fn explicit_flags_override_defaults() {
        let parsed = parse_args_from(
            args(&[
                "--frames",
                "dir",
                "--name",
                "x",
                "--out",
                "/tmp/out",
                "--manifest-out",
                "/tmp/x.lua",
                "--bg-tolerance",
                "0.2",
            ])
            .into_iter(),
        )
        .expect("parse");
        assert_eq!(parsed.out, PathBuf::from("/tmp/out"));
        assert_eq!(parsed.manifest_out, PathBuf::from("/tmp/x.lua"));
        assert_eq!(parsed.bg_tolerance, 0.2);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
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
}
