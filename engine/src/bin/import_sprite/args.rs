//! The importer's command line: what it accepts, and what it refuses.
//!
//! Split from `main.rs`, which was the argument grammar and the import pipeline
//! at once. The grammar is where the refusals live -- exactly one source, a grid
//! that a spritesheet actually needs -- and it is the half with no I/O, so its
//! tests run without touching the filesystem.

use std::path::PathBuf;

pub const DEFAULT_BG_TOLERANCE: f32 = 0.12;
pub struct Args {
    pub gif: Option<PathBuf>,
    pub frames_dir: Option<PathBuf>,
    pub spritesheet: Option<PathBuf>,
    pub cell: Option<(u32, u32)>,
    pub row_counts: Option<Vec<usize>>,
    pub name: String,
    pub states: Option<String>,
    pub out: PathBuf,
    pub manifest_out: PathBuf,
    pub bg_tolerance: f32,
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

pub fn parse_args_from(mut raw_args: impl Iterator<Item = String>) -> Result<Args, String> {
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

pub fn parse_args() -> Result<Args, String> {
    parse_args_from(std::env::args().skip(1))
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
